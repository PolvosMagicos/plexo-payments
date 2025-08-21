use std::time::Duration;

use crate::{models::common::LosslessNumber, services::crypto::CryptoError};
use once_cell::sync::Lazy;
use rand::{rng, Rng};
use reqwest::{Client, StatusCode};
use serde::Serialize;
use serde_json::{json, Value};
use thiserror::Error;
use tokio::time::sleep;

#[derive(Error, Debug)]
pub enum PlexoServiceError {
    #[error("Failed to sign request: {0}")]
    SigningError(#[from] CryptoError),

    #[error("HTTP request error: {0}")]
    HttpRequestError(#[from] reqwest::Error),

    #[error("HTTP request timeout")]
    Timeout,

    #[error("HTTP status error: {0}")]
    HttpStatusError(StatusCode),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
}

static HTTP_CLIENT: Lazy<Client> = Lazy::new(|| {
    let connect_timeout = env_duration("PLEXO_CONNECT_TIMEOUT_MS", 5_000);
    let request_timeout = env_duration("PLEXO_REQUEST_TIMEOUT_MS", 10_000);
    let tcp_keepalive = env_duration("PLEXO_TCP_KEEPALIVE_MS", 30_000);

    Client::builder()
        .http1_only() // WCF .svc endpoints can be flaky on HTTP/2
        .connect_timeout(connect_timeout)
        .timeout(request_timeout)
        .tcp_keepalive(Some(tcp_keepalive))
        .pool_max_idle_per_host(8)
        .build()
        .expect("failed to build reqwest client")
});

fn env_duration(var: &str, default_ms: u64) -> Duration {
    std::env::var(var)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map(Duration::from_millis)
        .unwrap_or(Duration::from_millis(default_ms))
}

fn env_u32(var: &str, default_val: u32) -> u32 {
    std::env::var(var)
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(default_val)
}

fn env_u64(var: &str, default_val: u64) -> u64 {
    std::env::var(var)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(default_val)
}

fn should_retry_status(status: StatusCode) -> bool {
    matches!(
        status,
        StatusCode::BAD_GATEWAY | StatusCode::SERVICE_UNAVAILABLE | StatusCode::GATEWAY_TIMEOUT
    )
}

fn should_retry_error(err: &reqwest::Error) -> bool {
    err.is_timeout() || err.is_connect() || err.is_request() || err.is_body() || err.is_decode()
}

fn backoff_delay_ms(attempt: u32, base_ms: u64, max_ms: u64) -> u64 {
    // Exponential backoff with jitter
    let exp = base_ms.saturating_mul(1u64 << attempt.min(10));
    let capped = exp.min(max_ms);
    let jitter = rng().random_range(0..=capped / 4);
    capped + jitter
}

pub async fn post_with_retry<T: Serialize + ?Sized>(
    url: &str,
    payload: &T,
    attempt_timeout: Duration,
) -> Result<reqwest::Response, PlexoServiceError> {
    let max_retries = env_u32("PLEXO_MAX_RETRIES", 3);
    let base_backoff = env_u64("PLEXO_BACKOFF_BASE_MS", 250);
    let max_backoff = env_u64("PLEXO_BACKOFF_MAX_MS", 5_000);

    let client = &*HTTP_CLIENT;
    let mut attempt: u32 = 0;

    loop {
        let res = client
            .post(url)
            .timeout(attempt_timeout)
            .json(payload)
            .send()
            .await;

        match res {
            Ok(rsp) => {
                let status = rsp.status();
                if status.is_success() {
                    return Ok(rsp);
                }

                if attempt < max_retries && should_retry_status(status) {
                    attempt += 1;
                    let delay = backoff_delay_ms(attempt, base_backoff, max_backoff);
                    sleep(Duration::from_millis(delay)).await;
                    continue;
                }

                return Err(PlexoServiceError::HttpStatusError(status));
            }
            Err(e) => {
                if e.is_timeout() {
                    if attempt < max_retries {
                        attempt += 1;
                        let delay = backoff_delay_ms(attempt, base_backoff, max_backoff);
                        sleep(Duration::from_millis(delay)).await;
                        continue;
                    }
                    return Err(PlexoServiceError::Timeout);
                }

                if attempt < max_retries && should_retry_error(&e) {
                    attempt += 1;
                    let delay = backoff_delay_ms(attempt, base_backoff, max_backoff);
                    sleep(Duration::from_millis(delay)).await;
                    continue;
                }

                return Err(PlexoServiceError::HttpRequestError(e));
            }
        }
    }
}

// Helper function to recursively remove null values from a JSON Value
// and properly format LosslessNumber fields
pub fn clean_nulls(value: &mut Value) {
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
