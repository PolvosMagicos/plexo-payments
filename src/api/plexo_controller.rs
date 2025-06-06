use crate::models::requests::{AuthorizationRequest, PaymentRequest, StatusRequest};
use crate::models::responses::ApiResponse;
use crate::services::plexo_service::{self, PlexoServiceError};
use actix_web::{web, HttpResponse, Result as ActixResult};
use log::{error, info};

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
            error!("Error processing authorization request: {:?}", e);

            let status_code = match e {
                PlexoServiceError::Timeout => actix_web::http::StatusCode::GATEWAY_TIMEOUT,
                PlexoServiceError::HttpRequestError(_) => actix_web::http::StatusCode::BAD_GATEWAY,
                PlexoServiceError::SerializationError(_) => {
                    actix_web::http::StatusCode::BAD_REQUEST
                }
                PlexoServiceError::SigningError(_) => {
                    actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
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

    match plexo_service::send_payment_request(request.into_inner()).await {
        Ok(response) => {
            info!("Successfully processed payment request");
            Ok(HttpResponse::Ok().json(ApiResponse {
                success: true,
                data: Some(response),
                error: None,
            }))
        }
        Err(e) => {
            error!("Error processing payment request: {}", e);

            let status_code = match e {
                PlexoServiceError::Timeout => actix_web::http::StatusCode::GATEWAY_TIMEOUT,
                PlexoServiceError::HttpRequestError(_) => actix_web::http::StatusCode::BAD_GATEWAY,
                PlexoServiceError::SerializationError(_) => {
                    actix_web::http::StatusCode::BAD_REQUEST
                }
                PlexoServiceError::SigningError(_) => {
                    actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
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
            error!("Error processing status request: {}", e);

            let status_code = match e {
                PlexoServiceError::Timeout => actix_web::http::StatusCode::GATEWAY_TIMEOUT,
                PlexoServiceError::HttpRequestError(_) => actix_web::http::StatusCode::BAD_GATEWAY,
                PlexoServiceError::SerializationError(_) => {
                    actix_web::http::StatusCode::BAD_REQUEST
                }
                PlexoServiceError::SigningError(_) => {
                    actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
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
