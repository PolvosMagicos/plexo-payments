use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct SignedRequest {
    pub Object: SignedObject,
    pub Signature: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SignedObject {
    pub Fingerprint: String,
    pub Object: serde_json::Value,
    pub UTCUnixTimeExpiration: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}
