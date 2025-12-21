use std::time::Duration;

use crate::models::requests::{
    AuthorizationRequest, PaymentRequest, ReferenceRequest, ReferenceType, StatusRequest,
};
use crate::models::responses::ApiResponse;
use crate::services::helpers::{
    debug_outgoing_response, debug_purchase_status, extract_purchase_status, is_ambiguous,
    PlexoServiceError, PurchaseOutcome,
};
use crate::services::plexo_service::{self};
use actix_web::{web, HttpResponse, Result as ActixResult};
use log::{error, info};
use serde_json::Value;
use tokio::time::sleep;

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
    println!("=== PURCHASE START ===");
    println!(
        "tokio runtime? {}",
        tokio::runtime::Handle::try_current().is_ok()
    );

    let payment_req = request.into_inner();

    // MetaReference = order id (ClientReferenceId)
    let meta_reference = payment_req.Request.ClientReferenceId.clone();
    let client_name = payment_req.Client.clone(); // grab before moving payment_req

    // Add a heartbeat task to prove the runtime is still working
    let heartbeat = tokio::spawn(async {
        for i in 0..20 {
            tokio::time::sleep(Duration::from_secs(1)).await;
            println!("💓 Heartbeat {i}");
        }
    });

    println!("=== SPAWNING PAYMENT TASK ===");
    let handle = tokio::spawn(async move {
        println!("=== INSIDE PAYMENT TASK ===");
        plexo_service::send_payment_request(payment_req).await
    });

    let abort = handle.abort_handle();

    println!("=== ENTERING SELECT ===");
    let purchase_res: Result<Value, PlexoServiceError> = tokio::select! {
        joined = handle => {
            println!("=== SELECT: HANDLE COMPLETED ===");
            heartbeat.abort();
            match joined {
                Ok(inner) => inner,
                Err(join_err) => {
                    println!("join error: {join_err}");
                    Err(PlexoServiceError::Timeout)
                }
            }
        }
        _ = sleep(Duration::from_secs(12)) => {
            println!("=== SELECT: TIMEOUT BRANCH ===");
            println!("⏱ purchase timed out (controller) -> aborting purchase task and falling back to status");
            heartbeat.abort();
            abort.abort();
            Err(PlexoServiceError::Timeout)
        }
    };
    println!("=== SELECT COMPLETED ===");

    match purchase_res {
        Ok(raw_purchase) => {
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
        Err(e) => {
            error!("Error processing payment request: {e}");

            // 2) If ambiguous => fallback to status
            if is_ambiguous(&e) {
                info!(
                    "Ambiguous purchase error; falling back to Status for meta_reference={meta_reference}"
                );

                let status_req = StatusRequest {
                    client: client_name, // or use env if you prefer
                    request: ReferenceRequest {
                        reference_type: ReferenceType::ClientPurchaseReferenceId, // = 1 ✅
                        meta_reference,
                    },
                };

                match plexo_service::send_status_request(status_req).await {
                    Ok(raw_status) => {
                        let st = extract_purchase_status(&raw_status);

                        let outcome = match st {
                            Some(0) => PurchaseOutcome::Approved {
                                source: "status",
                                raw: raw_status,
                            },
                            Some(_) => PurchaseOutcome::Declined {
                                source: "status",
                                raw: raw_status,
                            },
                            None => PurchaseOutcome::Pending {
                                source: "status",
                                raw: raw_status,
                            },
                        };

                        let mut http = match outcome {
                            PurchaseOutcome::Pending { .. } => HttpResponse::Accepted(),
                            _ => HttpResponse::Ok(),
                        };

                        let resp = ApiResponse {
                            success: true,
                            data: Some(outcome),
                            error: None,
                        };

                        debug_outgoing_response("status fallback OK", &resp);
                        Ok(http.json(resp))
                    }
                    Err(se) => {
                        error!("Status fallback failed: {se}");

                        let resp: ApiResponse<PurchaseOutcome> = ApiResponse {
                            success: false,
                            data: Some(PurchaseOutcome::Unknown {
                                source: "purchase",
                                error: format!(
                                    "purchase failed ({e}); status fallback failed ({se})"
                                ),
                            }),
                            error: Some("Ambiguous gateway error".to_string()),
                        };

                        debug_outgoing_response("status fallback FAILED", &resp);
                        Ok(HttpResponse::BadGateway().json(resp))
                    }
                }
            } else {
                // 3) Non-ambiguous => return error as before
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
                    PlexoServiceError::JoinError(_) => {
                        // task aborted / runtime failure
                        actix_web::http::StatusCode::GATEWAY_TIMEOUT
                    }
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
