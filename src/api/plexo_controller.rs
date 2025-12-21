use std::time::Duration;

use crate::models::requests::{AuthorizationRequest, PaymentRequest, StatusRequest};
use crate::models::responses::ApiResponse;
use crate::services::helpers::{
    debug_outgoing_response, debug_purchase_status, extract_purchase_status, fallback_status,
    is_ambiguous, PlexoServiceError, PurchaseOutcome,
};
use crate::services::plexo_service::{self};
use actix_web::{web, HttpResponse, Result as ActixResult};
use log::{error, info};
use serde_json::Value;
use tokio::sync::oneshot;
use tokio::time::timeout;

pub async fn authorize(request: web::Json<AuthorizationRequest>) -> ActixResult<HttpResponse> {
    info!("Received authorization request");

    match plexo_service::send_authorization_request(request.into_inner()).await {
        Ok(response) => {
            info!("Successfully processed authorization request");
            Ok(HttpResponse::Ok().json(ApiResponse {
                success: true,
                data: Some(response),
                error: None,
            }))
        }
        Err(e) => {
            error!("Error processing authorization request: {e}");

            let status_code = match e {
                PlexoServiceError::Timeout => actix_web::http::StatusCode::GATEWAY_TIMEOUT,
                PlexoServiceError::HttpRequestError(_) => actix_web::http::StatusCode::BAD_GATEWAY,
                PlexoServiceError::SerializationError(_) => {
                    actix_web::http::StatusCode::BAD_REQUEST
                }
                PlexoServiceError::SigningError(_) => {
                    actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
                }
                PlexoServiceError::HttpStatusError(_) => actix_web::http::StatusCode::BAD_GATEWAY,

                PlexoServiceError::JoinError(_) => {
                    // task aborted / runtime failure
                    actix_web::http::StatusCode::GATEWAY_TIMEOUT
                }
            };

            Ok(HttpResponse::build(status_code).json(ApiResponse::<()> {
                success: false,
                data: None,
                error: Some(e.to_string()),
            }))
        }
    }
}

pub async fn purchase(request: web::Json<PaymentRequest>) -> ActixResult<HttpResponse> {
    info!("Received payment request");

    let payment_req = request.into_inner();

    // MetaReference = order id (ClientReferenceId)
    let meta_reference = payment_req.Request.ClientReferenceId.clone();
    let client_name = payment_req.Client.clone();

    // Spawn purchase work and return result through a channel
    let (tx, rx) = oneshot::channel::<Result<Value, PlexoServiceError>>();

    tokio::spawn(async move {
        let res = plexo_service::send_payment_request(payment_req).await;
        let _ = tx.send(res); // ignore if receiver dropped
    });

    // Hard upper bound for "purchase" from the controller perspective.
    // Even if reqwest wedges, you will still respond.
    let purchase_res = timeout(Duration::from_secs(12), rx).await;

    match purchase_res {
        // purchase finished in time
        Ok(Ok(Ok(raw_purchase))) => {
            debug_purchase_status(&raw_purchase);
            let st = extract_purchase_status(&raw_purchase);

            let outcome = match st {
                Some(0) => PurchaseOutcome::Approved {
                    source: "purchase",
                    raw: raw_purchase,
                },
                Some(_) => PurchaseOutcome::Declined {
                    source: "purchase",
                    raw: raw_purchase,
                },
                None => PurchaseOutcome::Unknown {
                    source: "purchase",
                    error: "Missing Purchase.Status in response".to_string(),
                },
            };

            let resp = ApiResponse {
                success: true,
                data: Some(outcome),
                error: None,
            };

            debug_outgoing_response("purchase OK", &resp);
            Ok(HttpResponse::Ok().json(resp))
        }

        // purchase returned an error in time
        Ok(Ok(Err(e))) => {
            error!("Error processing payment request: {e}");

            // Ambiguous => fallback to status
            if is_ambiguous(&e) {
                match fallback_status(client_name, meta_reference).await {
                    Ok(response) => Ok(response),
                    Err(se) => {
                        error!("Status fallback failed: {se}");

                        let resp: ApiResponse<PurchaseOutcome> = ApiResponse {
                            success: false,
                            data: Some(PurchaseOutcome::Unknown {
                                source: "purchase",
                                error: format!("purchase timed out; status fallback failed ({se})"),
                            }),
                            error: Some("Gateway timeout".to_string()),
                        };

                        Ok(HttpResponse::GatewayTimeout().json(resp))
                    }
                }
            } else {
                // Non-ambiguous => return error as before
                let status_code = match e {
                    PlexoServiceError::Timeout => actix_web::http::StatusCode::GATEWAY_TIMEOUT,
                    PlexoServiceError::HttpRequestError(_) => {
                        actix_web::http::StatusCode::BAD_GATEWAY
                    }
                    PlexoServiceError::SerializationError(_) => {
                        actix_web::http::StatusCode::BAD_REQUEST
                    }
                    PlexoServiceError::SigningError(_) => {
                        actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
                    }
                    PlexoServiceError::HttpStatusError(_) => {
                        actix_web::http::StatusCode::BAD_GATEWAY
                    }
                    PlexoServiceError::JoinError(_) => actix_web::http::StatusCode::GATEWAY_TIMEOUT,
                };

                let resp = ApiResponse::<()> {
                    success: false,
                    data: None,
                    error: Some(e.to_string()),
                };

                debug_outgoing_response("purchase FAILED non-ambiguous", &resp);
                Ok(HttpResponse::build(status_code).json(resp))
            }
        }

        // channel dropped (task panicked) => treat as ambiguous and fallback
        Ok(Err(_recv_closed)) => {
            error!("Purchase task dropped (oneshot receiver closed) -> fallback to status");

            match fallback_status(client_name, meta_reference).await {
                Ok(response) => Ok(response),
                Err(se) => {
                    error!("Status fallback failed: {se}");

                    let resp: ApiResponse<PurchaseOutcome> = ApiResponse {
                        success: false,
                        data: Some(PurchaseOutcome::Unknown {
                            source: "purchase",
                            error: format!("purchase timed out; status fallback failed ({se})"),
                        }),
                        error: Some("Gateway timeout".to_string()),
                    };

                    Ok(HttpResponse::GatewayTimeout().json(resp))
                }
            }
        }

        // controller-level timeout => fallback to status immediately
        Err(_elapsed) => {
            error!("⏱ purchase timed out at controller level -> fallback to status");

            match fallback_status(client_name, meta_reference).await {
                Ok(response) => Ok(response),
                Err(se) => {
                    error!("Status fallback failed: {se}");

                    let resp: ApiResponse<PurchaseOutcome> = ApiResponse {
                        success: false,
                        data: Some(PurchaseOutcome::Unknown {
                            source: "purchase",
                            error: format!("purchase timed out; status fallback failed ({se})"),
                        }),
                        error: Some("Gateway timeout".to_string()),
                    };

                    Ok(HttpResponse::GatewayTimeout().json(resp))
                }
            }
        }
    }
}

pub async fn status(request: web::Json<StatusRequest>) -> ActixResult<HttpResponse> {
    info!("Received payment request");

    match plexo_service::send_status_request(request.into_inner()).await {
        Ok(response) => {
            info!("Successfully processed payment request");
            Ok(HttpResponse::Ok().json(ApiResponse {
                success: true,
                data: Some(response),
                error: None,
            }))
        }
        Err(e) => {
            error!("Error processing status request: {e}");

            let status_code = match e {
                PlexoServiceError::Timeout => actix_web::http::StatusCode::GATEWAY_TIMEOUT,
                PlexoServiceError::HttpRequestError(_) => actix_web::http::StatusCode::BAD_GATEWAY,
                PlexoServiceError::SerializationError(_) => {
                    actix_web::http::StatusCode::BAD_REQUEST
                }
                PlexoServiceError::SigningError(_) => {
                    actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
                }
                PlexoServiceError::HttpStatusError(_) => actix_web::http::StatusCode::BAD_GATEWAY,

                PlexoServiceError::JoinError(_) => {
                    // task aborted / runtime failure
                    actix_web::http::StatusCode::GATEWAY_TIMEOUT
                }
            };

            Ok(HttpResponse::build(status_code).json(ApiResponse::<()> {
                success: false,
                data: None,
                error: Some(e.to_string()),
            }))
        }
    }
}
