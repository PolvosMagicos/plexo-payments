use crate::models::common::LosslessNumber;
use crate::models::requests::{AuthorizationRequest, PaymentRequest, StatusRequest};
use crate::services::crypto::{get_crypto_service, CryptoError};
use log::{error, info};
use reqwest::Client;
use serde_json::{json, Value};
use thiserror::Error;

const PLEXO_AUTH_URL: &str = "https://testing.plexo.com.uy:4043/SecurePaymentGateway.svc/Auth";
const PLEXO_PURCHASE_URL: &str =
    "https://testing.plexo.com.uy:4043/SecurePaymentGateway.svc/Operation/Purchase";
const PLEXO_STATUS_URL: &str =
    "https://testing.plexo.com.uy:4043/SecurePaymentGateway.svc/Operation/Status";

#[derive(Error, Debug)]
pub enum PlexoServiceError {
    #[error("Failed to sign request: {0}")]
    SigningError(#[from] CryptoError),

    #[error("HTTP request error: {0}")]
    HttpRequestError(#[from] reqwest::Error),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
}

pub async fn send_authorization_request(
    auth_request: AuthorizationRequest,
) -> Result<Value, PlexoServiceError> {
    // Convert request to Value and remove null values before signing
    let mut request_value = json!(auth_request);
    clean_nulls(&mut request_value);
    println!("signed_payload");
    println!("signed_payload: {:#?}", request_value);

    // Sign the payload
    let crypto_service = get_crypto_service()?;
    let signed_payload = crypto_service.create_signed_payload(&request_value)?;

    info!("Sending authorization request to Plexo");

    // Send the request to Plexo
    let client = Client::new();
    let response = client
        .post(PLEXO_AUTH_URL)
        .json(&signed_payload)
        .send()
        .await?
        .json::<Value>()
        .await?;

    info!("Received authorization response from Plexo");

    Ok(response)
}

pub async fn send_payment_request(
    payment_request: PaymentRequest,
) -> Result<Value, PlexoServiceError> {
    // Convert request to Value and remove null values before signing
    let mut request_value = json!(payment_request);
    clean_nulls(&mut request_value);
    println!("signed_payload");
    println!("signed_payload: {:#?}", request_value);

    // Sign the payload
    let crypto_service = get_crypto_service()?;
    let signed_payload = crypto_service.create_signed_payload(&request_value)?;

    info!("Sending payment request to Plexo");

    // Send the request to Plexo
    let client = Client::new();
    let response = client
        .post(PLEXO_PURCHASE_URL)
        .json(&signed_payload)
        .send()
        .await?
        .json::<Value>()
        .await?;

    info!("Received payment response from Plexo");

    Ok(response)
}

pub async fn send_status_request(
    status_request: StatusRequest,
) -> Result<Value, PlexoServiceError> {
    // Convert request to Value and remove null values before signing
    let mut request_value = json!(status_request);
    clean_nulls(&mut request_value);
    println!("signed_payload");
    println!("signed_payload: {:#?}", request_value);

    // Sign the payload
    let crypto_service = get_crypto_service()?;
    let signed_payload = crypto_service.create_signed_payload(&request_value)?;

    info!("Sending payment request to Plexo");

    // Send the request to Plexo
    let client = Client::new();
    let response = client
        .post(PLEXO_STATUS_URL)
        .json(&signed_payload)
        .send()
        .await?
        .json::<Value>()
        .await?;

    info!("Received payment response from Plexo");

    Ok(response)
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
