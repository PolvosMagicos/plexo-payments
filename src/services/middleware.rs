use actix_web::{
    body::{BoxBody, MessageBody},
    dev::{Service, ServiceRequest, ServiceResponse, Transform},
    http::{header::HeaderName, StatusCode},
    Error, HttpResponse,
};
use dashmap::DashMap;
use futures_util::Future;
use std::{
    future::{ready, Ready},
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::{Duration, Instant},
};
use subtle::ConstantTimeEq;

/// Configuration for service-to-service API key middleware
#[derive(Clone)]
pub struct ServiceAuthConfig {
    /// Single trusted service key (for simplicity in service-to-service communication)
    service_key: Arc<Vec<u8>>,
    /// Custom header name (defaults to "x-service-key")
    header_name: HeaderName,
    /// Strict rate limiting to prevent abuse
    rate_limit: ServiceRateLimit,
    /// Service identifier for metrics
    service_name: String,
}

#[derive(Clone)]
pub struct ServiceRateLimit {
    max_requests: u32, // Conservative limit for service calls
    window: Duration,  // Short window for burst protection
    storage: Arc<DashMap<String, (u32, Instant)>>,
}

impl ServiceAuthConfig {
    /// Create new configuration for service-to-service auth
    pub fn new(service_key: String, service_name: &str) -> Self {
        Self {
            service_key: Arc::new(service_key.into_bytes()),
            header_name: HeaderName::from_static("x-service-key"),
            rate_limit: ServiceRateLimit {
                max_requests: 1000, // Default conservative limit
                window: Duration::from_secs(60),
                storage: Arc::new(DashMap::new()),
            },
            service_name: service_name.to_string(),
        }
    }

    /// Set custom header name
    pub fn with_header_name(
        mut self,
        name: &str,
    ) -> Result<Self, actix_web::http::header::InvalidHeaderName> {
        self.header_name = HeaderName::try_from(name)?;
        Ok(self)
    }

    /// Configure rate limiting suitable for service-to-service communication
    pub fn with_rate_limit(mut self, max_requests: u32, window_seconds: u64) -> Self {
        self.rate_limit = ServiceRateLimit {
            max_requests,
            window: Duration::from_secs(window_seconds),
            storage: Arc::new(DashMap::new()),
        };
        self
    }

    /// Start the background cleanup task for rate limiting
    pub fn start_cleanup_task(&self) {
        let storage = self.rate_limit.storage.clone();
        let window = self.rate_limit.window;

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(window / 2);
            loop {
                interval.tick().await;
                let now = Instant::now();
                storage.retain(|_, (_, timestamp)| now.duration_since(*timestamp) < window);
            }
        });
    }
}

pub struct ServiceAuthMiddleware {
    config: ServiceAuthConfig,
}

impl ServiceAuthMiddleware {
    pub fn new(config: ServiceAuthConfig) -> Self {
        config.start_cleanup_task();
        Self { config }
    }
}

impl<S, B> Transform<S, ServiceRequest> for ServiceAuthMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    B: MessageBody + 'static,
{
    type Response = ServiceResponse<BoxBody>;
    type Error = Error;
    type Transform = ServiceAuthMiddlewareService<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(ServiceAuthMiddlewareService {
            service: Arc::new(service),
            config: self.config.clone(),
        }))
    }
}

pub struct ServiceAuthMiddlewareService<S> {
    service: Arc<S>,
    config: ServiceAuthConfig,
}

impl<S, B> Service<ServiceRequest> for ServiceAuthMiddlewareService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    B: MessageBody + 'static,
{
    type Response = ServiceResponse<BoxBody>;
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

    fn poll_ready(&self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let config = self.config.clone();
        let service = self.service.clone();

        Box::pin(async move {
            // Extract service key
            let service_key = match req.headers().get(&config.header_name) {
                Some(key) => key,
                None => {
                    return Ok(create_service_error_response(
                        req,
                        StatusCode::UNAUTHORIZED,
                        "Missing service authentication",
                    ))
                }
            };

            // Constant-time comparison
            let key_bytes = match service_key.to_str() {
                Ok(key) => key.as_bytes(),
                Err(_) => {
                    return Ok(create_service_error_response(
                        req,
                        StatusCode::BAD_REQUEST,
                        "Invalid service key format",
                    ))
                }
            };

            if key_bytes.ct_eq(&config.service_key).unwrap_u8() != 1 {
                return Ok(create_service_error_response(
                    req,
                    StatusCode::FORBIDDEN,
                    "Invalid service credentials",
                ));
            }

            // Strict rate limiting
            let service_id = &config.service_name;
            let mut entry = config
                .rate_limit
                .storage
                .entry(service_id.to_string())
                .or_insert((0, Instant::now()));

            let (count, last_request) = &mut *entry;
            let now = Instant::now();

            if now.duration_since(*last_request) >= config.rate_limit.window {
                *count = 0;
                *last_request = now;
            }

            if *count >= config.rate_limit.max_requests {
                return Ok(create_service_error_response(
                    req,
                    StatusCode::TOO_MANY_REQUESTS,
                    "Service rate limit exceeded",
                ));
            }

            *count += 1;

            // Authentication successful, proceed with request
            let res = service.call(req).await?;
            Ok(res.map_into_boxed_body())
        })
    }
}

fn create_service_error_response(
    req: ServiceRequest,
    status: StatusCode,
    message: &str,
) -> ServiceResponse<BoxBody> {
    let response = HttpResponse::build(status).json(serde_json::json!({
        "error": status.canonical_reason().unwrap_or("Service Error"),
        "message": message,
        "service_error": true,
        "timestamp": chrono::Utc::now().to_rfc3339(),
    }));

    req.into_response(response).map_into_boxed_body()
}
