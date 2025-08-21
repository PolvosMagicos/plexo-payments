use crate::models::common::LosslessNumber;
use crate::models::requests::{AuthorizationRequest, PaymentRequest, StatusRequest};
use crate::services::crypto::get_crypto_service;
use crate::services::helpers::post_with_retry;
use crate::services::helpers::PlexoServiceError;
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

// Helper function to recursively remove null values from a JSON Value
// and properly format LosslessNumber fields
fn clean_nulls(value: &mut Value) {
    match value {
        Value::Object(map) => {
            // Collect keys to remove (can't modify while iterating)
            let null_keys: Vec<String> = map
                .iter()
                .filter_map(|(k, v)| if v.is_null() { Some(k.clone()) } else { None })
                .collect();

            // Remove null values
            for key in null_keys {
                map.remove(&key);
            }

            // Recursively process remaining values and handle special formatting
            for (key, v) in map.iter_mut() {
                // Check if this field should be treated as a LosslessNumber
                if is_lossless_number_field(key) {
                    if let Value::String(s) = v {
                        // Convert string to properly formatted number
                        let lossless = LosslessNumber::new(s.clone());
                        let formatted = lossless.format_for_json();
                        // Try to parse as number for JSON
                        if let Ok(num) = formatted.parse::<f64>() {
                            *v = json!(num);
                        }
                    }
                }
                clean_nulls(v);
            }
        }
        Value::Array(arr) => {
            // Remove null values from array
            arr.retain(|item| !item.is_null());

            // Recursively process remaining items
            for item in arr.iter_mut() {
                clean_nulls(item);
            }
        }
        _ => {} // Nothing to do for primitive values
    }
}

// Helper function to determine if a field should be treated as a LosslessNumber
fn is_lossless_number_field(field_name: &str) -> bool {
    matches!(
        field_name,
        "BilledAmount" | "TaxedAmount" | "VATAmount" | "Amount" | "LoyaltyProgramAmount"
    )
}
