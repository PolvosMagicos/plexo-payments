use std::time::{Duration, Instant};

use crate::models::requests::{AuthorizationRequest, PaymentRequest, StatusRequest};
use crate::models::responses::ApiResponse;
use crate::services::helpers::{
    debug_outgoing_response, debug_purchase_status, extract_purchase_status, fallback_status,
    is_ambiguous, PlexoServiceError, PurchaseOutcome,
};
use crate::services::plexo_service::{self};
use actix_web::{web, HttpResponse, Responder, Result as ActixResult};
use log::{error, info};
use serde::Serialize;
use tokio::net::TcpStream;
use tokio::time::timeout;

#[derive(Clone)]
pub struct AppState {
    pub started_at: Instant,
}

#[derive(Serialize)]
struct HealthResponse {
    ok: bool,
    service: &'static str,
    timestamp_utc: String,
    uptime_secs: u64,
    version: &'static str,

    plexo_tcp_ok: Option<bool>,
    plexo_tcp_error: Option<String>,
}

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

    let meta_reference = payment_req.Request.ClientReferenceId.clone();
    let client_name = payment_req.Client.clone();

    match plexo_service::send_payment_request(payment_req).await {
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

            // ✅ Timeouts / connect / 502 / etc should be treated as ambiguous -> fallback
            if is_ambiguous(&e) {
                match fallback_status(client_name, meta_reference).await {
                    Ok(response) => Ok(response),
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
                            error: Some("Gateway timeout".to_string()),
                        };
                        Ok(HttpResponse::GatewayTimeout().json(resp))
                    }
                }
            } else {
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

pub async fn health(state: web::Data<AppState>) -> impl Responder {
    let uptime = state.started_at.elapsed().as_secs();

    let plexo_host =
        std::env::var("PLEXO_BASE_URL").unwrap_or_else(|_| "testing.plexo.com.uy".to_string());
    let plexo_port: u16 = std::env::var("PLEXO_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(4043);

    let addr = format!("{plexo_host}:{plexo_port}");

    // ✅ async tcp connect + timeout
    let plexo_check = timeout(Duration::from_secs(2), TcpStream::connect(&addr)).await;

    let (plexo_tcp_ok, plexo_tcp_error) = match plexo_check {
        Ok(Ok(_sock)) => (Some(true), None),
        Ok(Err(e)) => (Some(false), Some(format!("tcp connect error: {e}"))),
        Err(_) => (Some(false), Some("tcp connect timeout".to_string())),
    };

    HttpResponse::Ok().json(HealthResponse {
        ok: true,
        service: "plexo-back",
        timestamp_utc: chrono::Utc::now().to_rfc3339(),
        uptime_secs: uptime,
        version: env!("CARGO_PKG_VERSION"),
        plexo_tcp_ok,
        plexo_tcp_error,
    })
}
