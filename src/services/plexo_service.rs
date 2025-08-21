use crate::models::requests::{AuthorizationRequest, PaymentRequest, StatusRequest};
use crate::services::crypto::get_crypto_service;
use crate::services::helpers::PlexoServiceError;
use crate::services::helpers::{clean_nulls, post_with_retry};
use log::info;
use serde_json::{json, Value};

const PLEXO_AUTH_URL: &str = "https://testing.plexo.com.uy:4043/SecurePaymentGateway.svc/Auth";
const PLEXO_PURCHASE_URL: &str =
    "https://testing.plexo.com.uy:4043/SecurePaymentGateway.svc/Operation/Purchase";
const PLEXO_STATUS_URL: &str =
    "https://testing.plexo.com.uy:4043/SecurePaymentGateway.svc/Operation/Status";

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
    let response = post_with_retry(PLEXO_AUTH_URL, &signed_payload).await?;
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
    let response = post_with_retry(PLEXO_PURCHASE_URL, &signed_payload).await?;

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
    let response = post_with_retry(PLEXO_STATUS_URL, &signed_payload).await?;

    let parsed_response = response.json::<Value>().await?;

    info!("Received status response from Plexo");

    println!("status request response: {request_value}");

    Ok(parsed_response)
}
