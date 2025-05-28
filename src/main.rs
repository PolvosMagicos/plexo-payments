use actix_cors::Cors;
use actix_web::{middleware, web, App, HttpResponse, HttpServer};
use dotenvy::dotenv;
use log::info;

mod api;
mod models;
mod services;

use api::plexo_controller::{authorize, purchase, status};
use services::middleware::{ServiceAuthConfig, ServiceAuthMiddleware};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Initialize logger
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));
    // Load .env file
    dotenv().ok();
    let secret_key =
        std::env::var("SECRET_KEY").expect("SECRET_KEY environment variable is required");
    let header_name =
        std::env::var("HEADER_NAME").expect("HEADER_NAME environment variable is required");
    let service_name =
        std::env::var("SERVICE_NAME").expect("SERVICE_NAME environment variable is required");

    // Get configuration from environment
    let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port = std::env::var("PORT")
        .unwrap_or_else(|_| "8080".to_string())
        .parse::<u16>()
        .expect("PORT must be a number");

    info!("Starting server at {}:{}", host, port);

    // Initialize services
    services::crypto::init().expect("Failed to initialize crypto service");

    let auth_config = ServiceAuthConfig::new(secret_key, "my-service")
        .with_rate_limit(100, 60)
        .with_header_name(&header_name)
        .unwrap();

    HttpServer::new(move || {
        App::new()
            .wrap(ServiceAuthMiddleware::new(auth_config.clone()))
            .wrap(middleware::Logger::default())
            .wrap(
                Cors::default()
                    .allow_any_origin()
                    .allowed_methods(vec!["GET", "POST", "PUT", "PATCH", "DELETE"])
                    .allowed_headers(vec![
                        actix_web::http::header::AUTHORIZATION,
                        actix_web::http::header::ACCEPT,
                        actix_web::http::header::CONTENT_TYPE,
                    ])
                    .max_age(3600),
            )
            // Register API routes
            .service(
                web::scope("/api")
                    .route("/authorize", web::post().to(authorize))
                    .route("/purchase", web::post().to(purchase))
                    .route("/status", web::post().to(status)),
            )
            // Add a health check endpoint
            .route(
                "/health",
                web::get().to(|| async { HttpResponse::Ok().body("Service is running") }),
            )
    })
    .bind((host, port))?
    .run()
    .await
}
