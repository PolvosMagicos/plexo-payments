use crate::models::requests::{AuthorizationRequest, PaymentRequest, StatusRequest};
use crate::services::crypto::get_crypto_service;
use crate::services::helpers::{clean_nulls, post_with_retry};
use crate::services::helpers::{post_no_retry, PlexoServiceError};
use log::info;
use serde_json::{json, Value};
use std::time::Duration;

use once_cell::sync::Lazy;

fn join(base: &str, path: &str) -> String {
    format!(
        "{}/{}",
        base.trim_end_matches('/'),
        path.trim_start_matches('/')
    )
}

pub static PLEXO_BASE_URL: Lazy<String> = Lazy::new(|| {
    std::env::var("PLEXO_BASE_URL")
        .expect("PLEXO_BASE_URL environment variable is required (e.g. https://plexo.com.uy:4043)")
});

pub static PLEXO_AUTH_URL: Lazy<String> =
    Lazy::new(|| join(&PLEXO_BASE_URL, "SecurePaymentGateway.svc/Auth"));

pub static PLEXO_PURCHASE_URL: Lazy<String> = Lazy::new(|| {
    join(
        &PLEXO_BASE_URL,
        "SecurePaymentGateway.svc/Operation/Purchase",
    )
});

pub static PLEXO_STATUS_URL: Lazy<String> =
    Lazy::new(|| join(&PLEXO_BASE_URL, "SecurePaymentGateway.svc/Operation/Status"));

pub async fn send_authorization_request(
    auth_request: AuthorizationRequest,
) -> Result<Value, PlexoServiceError> {
    // Convert request to Value and remove null values before signing
    let mut request_value = json!(auth_request);
    clean_nulls(&mut request_value);
    println!("signed_payload: {request_value}");

    // Sign the payload
    let crypto_service = get_crypto_service()?;
    let signed_payload = crypto_service.create_signed_payload(&request_value)?;

    info!("Sending authorization request to Plexo");

    // Send the request to Plexo
    let response =
        post_with_retry(&PLEXO_AUTH_URL, &signed_payload, Duration::from_secs(5)).await?;
    let parsed_response = response.json::<Value>().await?;

    info!("Received authorization response from Plexo");

    println!("signed payload response: {parsed_response}");

    Ok(parsed_response)
}

pub async fn send_payment_request(
    payment_request: PaymentRequest,
) -> Result<Value, PlexoServiceError> {
    // Convert request to Value and remove null values before signing
    let mut request_value = json!(payment_request);
    clean_nulls(&mut request_value);
    println!("payment request: {request_value}");

    // Sign the payload
    let crypto_service = get_crypto_service()?;
    let signed_payload = crypto_service.create_signed_payload(&request_value)?;

    info!("Sending payment request to Plexo");

    // Send the request to Plexo
    let response =
        post_no_retry(&PLEXO_PURCHASE_URL, &signed_payload, Duration::from_secs(8)).await?;

    let parsed_response = response.json::<Value>().await?;

    info!("Received payment response from Plexo");

    println!("payment request response: {parsed_response}");

    Ok(parsed_response)
}

pub async fn send_status_request(
    status_request: StatusRequest,
) -> Result<Value, PlexoServiceError> {
    // Convert request to Value and remove null values before signing
    let mut request_value = json!(status_request);
    clean_nulls(&mut request_value);
    println!("status request: {request_value}");

    // Sign the payload
    let crypto_service = get_crypto_service()?;
    let signed_payload = crypto_service.create_signed_payload(&request_value)?;

    info!("Sending payment request to Plexo");

    // Send the request to Plexo
    let response =
        post_with_retry(&PLEXO_STATUS_URL, &signed_payload, Duration::from_secs(10)).await?;

    let parsed_response = response.json::<Value>().await?;

    info!("Received status response from Plexo");

    println!("status request response: {request_value}");

    Ok(parsed_response)
}
