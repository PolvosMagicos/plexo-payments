use actix_cors::Cors;
use actix_web::{middleware, web, App, HttpResponse, HttpServer};
use dotenvy::dotenv;
use log::info;

mod api;
mod models;
mod services;

use api::plexo_controller::{authorize, purchase, status};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Initialize logger
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));
    // Load .env file
    dotenv().ok();

    // Get configuration from environment
    let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port = std::env::var("PORT")
        .unwrap_or_else(|_| "80".to_string())
        .parse::<u16>()
        .expect("PORT must be a number");

    info!("Starting server at {}:{}", host, port);

    // Initialize services
    services::crypto::init().expect("Failed to initialize crypto service");

    HttpServer::new(|| {
        App::new()
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
