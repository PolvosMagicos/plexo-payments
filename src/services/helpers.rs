use std::time::Duration;

use crate::{
    models::{
        common::LosslessNumber,
        requests::{ReferenceRequest, ReferenceType, StatusRequest},
        responses::ApiResponse,
    },
    services::{crypto::CryptoError, plexo_service},
};
use actix_web::HttpResponse;
use once_cell::sync::Lazy;
use rand::{rng, Rng};
use reqwest::{Client, StatusCode};
use serde::Serialize;
use serde_json::{json, Value};
use thiserror::Error;
use tokio::time::{sleep, timeout};

pub const PURCHASE_STATUS_PTR: &str = "/Object/Object/Response/Transactions/Purchase/Status";

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

    #[error("Task join error: {0}")]
    JoinError(#[from] tokio::task::JoinError),
}

#[derive(Serialize)]
#[serde(tag = "outcome", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PurchaseOutcome {
    Approved { source: &'static str, raw: Value },
    Declined { source: &'static str, raw: Value },
    Pending { source: &'static str, raw: Value },
    Unknown { source: &'static str, error: String },
}

static HTTP_CLIENT: Lazy<Client> = Lazy::new(|| {
    let connect_timeout = env_duration("PLEXO_CONNECT_TIMEOUT_MS", 5_000);

    Client::builder()
        .use_rustls_tls()
        .http1_only() // WCF .svc endpoints can be flaky on HTTP/2
        .connect_timeout(connect_timeout)
        .pool_max_idle_per_host(0)
        .pool_idle_timeout(None)
        .tcp_keepalive(None)
        .no_proxy()
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

pub async fn post_json_with_max_retries<T: Serialize + ?Sized>(
    url: &str,
    payload: &T,
    attempt_timeout: Duration,
    max_retries: u32,
) -> Result<reqwest::Response, PlexoServiceError> {
    let base_backoff = env_u64("PLEXO_BACKOFF_BASE_MS", 250);
    let max_backoff = env_u64("PLEXO_BACKOFF_MAX_MS", 5_000);

    let client = &*HTTP_CLIENT;
    let mut attempt: u32 = 0;

    loop {
        println!(
            "[HTTP] attempt={} about to build request to {}",
            attempt, url
        );

        let req = client
            .post(url)
            .json(payload)
            .timeout(attempt_timeout)
            .header("Connection", "close")
            .header("Accept-Encoding", "identity");
        println!("[HTTP] request built");

        let fut = req.send();
        println!(
            "[HTTP] send() future created; hard timeout={:?}",
            attempt_timeout
        );

        // ✅ HARD timeout around send()
        let res = timeout(attempt_timeout, fut)
            .await
            .map_err(|_| PlexoServiceError::Timeout)?;

        println!("[HTTP] send() future resolved");

        match res {
            Ok(rsp) => {
                println!("[HTTP] response headers received, status={}", rsp.status());

                let status = rsp.status();
                if status.is_success() {
                    return Ok(rsp);
                }

                if attempt < max_retries && should_retry_status(status) {
                    attempt += 1;
                    let delay = backoff_delay_ms(attempt, base_backoff, max_backoff);
                    println!("[HTTP] retryable status={}, sleeping {}ms", status, delay);
                    sleep(Duration::from_millis(delay)).await;
                    continue;
                }

                return Err(PlexoServiceError::HttpStatusError(status));
            }
            Err(e) => {
                println!("[HTTP] send() error: {e}");

                let is_timeout = e.is_timeout();
                let retryable = should_retry_error(&e);

                if attempt < max_retries && (is_timeout || retryable) {
                    attempt += 1;
                    let delay = backoff_delay_ms(attempt, base_backoff, max_backoff);
                    println!("[HTTP] retryable error, sleeping {}ms", delay);
                    sleep(Duration::from_millis(delay)).await;
                    continue;
                }

                if is_timeout {
                    return Err(PlexoServiceError::Timeout);
                }

                return Err(PlexoServiceError::HttpRequestError(e));
            }
        }
    }
}

pub async fn post_with_retry<T: Serialize + ?Sized>(
    url: &str,
    payload: &T,
    attempt_timeout: Duration,
) -> Result<reqwest::Response, PlexoServiceError> {
    let max_retries = env_u32("PLEXO_MAX_RETRIES", 3);
    post_json_with_max_retries(url, payload, attempt_timeout, max_retries).await
}

pub async fn post_no_retry<T: Serialize + ?Sized>(
    url: &str,
    payload: &T,
    attempt_timeout: Duration,
) -> Result<reqwest::Response, PlexoServiceError> {
    post_json_with_max_retries(url, payload, attempt_timeout, 0).await
}

// SERVICER HELPERS
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

// CONTROLLER HELPERS
pub fn is_ambiguous(err: &PlexoServiceError) -> bool {
    matches!(err, PlexoServiceError::Timeout)
        || matches!(err, PlexoServiceError::HttpStatusError(code)
            if *code == reqwest::StatusCode::BAD_GATEWAY
            || *code == reqwest::StatusCode::SERVICE_UNAVAILABLE
            || *code == reqwest::StatusCode::GATEWAY_TIMEOUT)
        || matches!(err, PlexoServiceError::HttpRequestError(_))
}

pub fn extract_purchase_status(v: &Value) -> Option<i64> {
    v.pointer("/Object/Object/Response/Transactions/Purchase/Status")
        .and_then(|x| x.as_i64())
}

pub fn debug_purchase_status(v: &Value) {
    // 1) Print full JSON response
    match serde_json::to_string(v) {
        Ok(pretty) => println!("🔎 Full purchase response:\n{pretty}"),
        Err(_) => println!("🔎 Full purchase response (raw): {v}"),
    }

    // 2) Print pointer path
    println!("🔎 Status JSON pointer path: {}", PURCHASE_STATUS_PTR);

    // 3) Print raw value at path
    let at_ptr = v.pointer(PURCHASE_STATUS_PTR);
    println!("🔎 Value at path: {}", at_ptr.unwrap_or(&Value::Null));

    // 4) Print parsed numeric value (if any)
    let parsed = at_ptr.and_then(|x| x.as_i64());
    println!("🔎 Parsed status as i64: {:?}", parsed);

    // Optional: show type info for debugging weird formats (string/float/etc.)
    if let Some(val) = at_ptr {
        println!(
            "🔎 Value JSON type: {}",
            match val {
                Value::Null => "null",
                Value::Bool(_) => "bool",
                Value::Number(_) => "number",
                Value::String(_) => "string",
                Value::Array(_) => "array",
                Value::Object(_) => "object",
            }
        );
    }
}

pub fn debug_outgoing_response<T: Serialize>(label: &str, body: &T) {
    match serde_json::to_string(body) {
        Ok(pretty) => println!("📤 Outgoing response ({label}):\n{pretty}"),
        Err(e) => println!("📤 Outgoing response ({label}) - failed to serialize: {e}"),
    }
}

pub async fn fallback_status(
    client_name: String,
    meta_reference: String,
) -> Result<HttpResponse, PlexoServiceError> {
    let status_req = StatusRequest {
        client: client_name,
        request: ReferenceRequest {
            reference_type: ReferenceType::ClientPurchaseReferenceId,
            meta_reference,
        },
    };

    let raw_status = plexo_service::send_status_request(status_req).await?;

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

    let resp = ApiResponse {
        success: true,
        data: Some(outcome),
        error: None,
    };

    let response = match resp.data.as_ref() {
        Some(PurchaseOutcome::Pending { .. }) => HttpResponse::Accepted().json(resp),
        _ => HttpResponse::Ok().json(resp),
    };

    Ok(response)
}
