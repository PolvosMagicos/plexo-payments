use crate::models::responses::{ApiResponse, SignedObject, SignedRequest};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use lazy_static::lazy_static;
use log::{error, info};
use openssl::hash::MessageDigest;
use openssl::pkcs12::Pkcs12;
use openssl::pkey::PKey;
use openssl::rsa::Rsa;
use openssl::sign::Signer;
use openssl::x509::X509;
use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use std::sync::{Arc, Mutex, Once};
use tempfile::NamedTempFile;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CryptoError {
    #[error("Failed to initialize crypto service: {0}")]
    InitializationError(String),

    #[error("Failed to sign payload: {0}")]
    SigningError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("OpenSSL error: {0}")]
    OpenSslError(#[from] openssl::error::ErrorStack),

    #[error("JSON serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
}

lazy_static! {
    static ref CRYPTO_SERVICE: Arc<Mutex<Option<CryptoService>>> = Arc::new(Mutex::new(None));
    static ref INIT: Once = Once::new();
}

pub struct CryptoService {
    private_key: PKey<openssl::pkey::Private>,
    fingerprint: String,
}

pub fn init() -> Result<(), CryptoError> {
    let mut initialized = false;

    INIT.call_once(|| {
        // In a real app, load these from env vars or secure storage
        let pfx_base64 =
            std::env::var("PFX_BASE64").expect("PFX_BASE64 environment variable is required");
        let pfx_password =
            std::env::var("PFX_PASSWORD").expect("PFX_PASSWORD environment variable is required");

        match CryptoService::new(&pfx_base64, &pfx_password) {
            Ok(service) => {
                let mut guard = CRYPTO_SERVICE.lock().unwrap();
                *guard = Some(service);
                initialized = true;
            }
            Err(e) => {
                error!("Failed to initialize crypto service: {}", e);
            }
        }
    });

    if initialized {
        Ok(())
    } else {
        Err(CryptoError::InitializationError(
            "Failed to initialize crypto service".to_string(),
        ))
    }
}

impl CryptoService {
    fn new(pfx_base64: &str, pfx_password: &str) -> Result<Self, CryptoError> {
        let pfx_data = BASE64.decode(pfx_base64).map_err(|e| {
            CryptoError::InitializationError(format!("Failed to decode PFX base64: {}", e))
        })?;

        // Create a temporary file to store PFX data
        let mut temp_file = NamedTempFile::new()?;
        temp_file.write_all(&pfx_data)?;
        let temp_path = temp_file.path().to_string_lossy().to_string();

        // Extract private key and fingerprint
        let (private_key, fingerprint) = Self::extract_from_pfx(&temp_path, pfx_password)?;

        info!("Crypto service initialized. Fingerprint: {}", fingerprint);

        Ok(CryptoService {
            private_key,
            fingerprint,
        })
    }

    fn extract_from_pfx(
        pfx_path: &str,
        password: &str,
    ) -> Result<(PKey<openssl::pkey::Private>, String), CryptoError> {
        // Load PFX file
        let pfx_data = fs::read(pfx_path)?;
        let pkcs12 = Pkcs12::from_der(&pfx_data).map_err(|e| {
            CryptoError::InitializationError(format!("Failed to parse PKCS12 data: {}", e))
        })?;

        // Parse PFX with password
        let parsed = pkcs12.parse2(password).map_err(|e| {
            CryptoError::InitializationError(format!("Failed to parse PKCS12 with password: {}", e))
        })?;

        // Get private key and certificate
        let cert = parsed.cert.ok_or_else(|| {
            CryptoError::InitializationError("No certificate found in PFX".to_string())
        })?;

        // Get private key
        let private_key = parsed.pkey.ok_or_else(|| {
            CryptoError::InitializationError("No private key found in PFX".to_string())
        })?;

        // Calculate SHA1 fingerprint
        let fingerprint_data = cert.digest(MessageDigest::sha1())?;
        let fingerprint = fingerprint_data
            .iter()
            .map(|b| format!("{:02X}", b))
            .collect::<Vec<String>>()
            .join("");

        Ok((private_key, fingerprint))
    }

    // Sort keys alphabetically and handle nulls according to Plexo requirements
    fn canonize_json(&self, value: &Value) -> Result<String, CryptoError> {
        match value {
            Value::Object(map) => {
                let mut result = String::from("{");

                // Sort keys alphabetically
                let mut keys: Vec<&String> = map.keys().collect();
                keys.sort();

                let mut is_first = true;

                for key in keys {
                    let val = &map[key];

                    // Skip null values completely as per Plexo requirements
                    if val.is_null() {
                        continue;
                    }

                    if !is_first {
                        result.push(',');
                    }
                    is_first = false;

                    // Add key
                    result.push('"');
                    result.push_str(key);
                    result.push('"');
                    result.push(':');

                    // Add value (recursively canonized)
                    let canonized_value = match val {
                        Value::Object(_) => self.canonize_json(val)?,
                        Value::Array(arr) => {
                            let mut array_result = String::from("[");
                            let mut is_first_item = true;

                            for item in arr {
                                // Skip null array items
                                if item.is_null() {
                                    continue;
                                }

                                if !is_first_item {
                                    array_result.push(',');
                                }
                                is_first_item = false;

                                // Recursively canonize array items
                                match item {
                                    Value::Object(_) => {
                                        array_result.push_str(&self.canonize_json(item)?)
                                    }
                                    Value::Array(_) => {
                                        array_result.push_str(&self.canonize_json(item)?)
                                    }
                                    Value::String(s) => {
                                        array_result.push_str(&format!("\"{}\"", s))
                                    }
                                    _ => array_result.push_str(&item.to_string()),
                                }
                            }

                            array_result.push(']');
                            array_result
                        }
                        Value::String(s) => format!("\"{}\"", s),
                        // For numbers, booleans, etc. - use direct string representation without quotes
                        _ => val.to_string(),
                    };

                    result.push_str(&canonized_value);
                }

                result.push('}');
                Ok(result)
            }
            Value::Array(arr) => {
                let mut result = String::from("[");
                let mut is_first = true;

                for item in arr {
                    if !is_first {
                        result.push(',');
                    }
                    is_first = false;

                    match item {
                        Value::Object(_) => result.push_str(&self.canonize_json(item)?),
                        Value::Array(_) => result.push_str(&self.canonize_json(item)?),
                        Value::String(s) => result.push_str(&format!("\"{}\"", s)),
                        _ => result.push_str(&item.to_string()),
                    }
                }

                result.push(']');
                Ok(result)
            }
            _ => Ok(value.to_string()),
        }
    }

    fn sign_payload(&self, payload: &Value) -> Result<(String, i64), CryptoError> {
        // Generate expiration time (5 minutes in the future)
        let expiration = chrono::Utc::now().timestamp() + (5 * 60);

        // Create the object to sign with required fields
        let object_to_sign = json!({
            "Fingerprint": self.fingerprint,
            "Object": payload,
            "UTCUnixTimeExpiration": expiration
        });

        // Canonize the JSON
        let canonized_json = self.canonize_json(&object_to_sign)?;

        info!("Canonized JSON: {}", canonized_json);

        // Convert to UTF-8 bytes
        let data_to_sign = canonized_json.as_bytes();

        // Create a signer using RSA-SHA512
        let mut signer = Signer::new(MessageDigest::sha512(), &self.private_key)
            .map_err(|e| CryptoError::SigningError(format!("Failed to create signer: {}", e)))?;

        // Sign the data
        let signature = signer
            .sign_oneshot_to_vec(data_to_sign)
            .map_err(|e| CryptoError::SigningError(format!("Failed to sign payload: {}", e)))?;

        // Encode the signature to base64
        let base64_signature = BASE64.encode(&signature);

        Ok((base64_signature, expiration))
    }

    pub fn create_signed_payload(&self, payload: &Value) -> Result<SignedRequest, CryptoError> {
        let (signature, expiration) = self.sign_payload(payload)?;

        Ok(SignedRequest {
            Object: SignedObject {
                Fingerprint: self.fingerprint.clone(),
                Object: payload.clone(),
                UTCUnixTimeExpiration: expiration,
            },
            Signature: signature,
        })
    }
}

// Singleton access to crypto service
pub fn get_crypto_service() -> Result<Arc<CryptoService>, CryptoError> {
    let guard = CRYPTO_SERVICE.lock().unwrap();

    match &*guard {
        Some(service) => Ok(Arc::new(service.clone())),
        None => Err(CryptoError::InitializationError(
            "Crypto service not initialized".to_string(),
        )),
    }
}

// Implement Clone for CryptoService
impl Clone for CryptoService {
    fn clone(&self) -> Self {
        // Create a new instance with the same private key and fingerprint
        CryptoService {
            private_key: self.private_key.clone(),
            fingerprint: self.fingerprint.clone(),
        }
    }
}
