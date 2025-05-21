use actix_web::{middleware, web, App, HttpResponse, HttpServer};
use dotenvy::dotenv;
use log::info;

mod api;
mod models;
mod services;

use api::plexo_controller::{authorize, purchase};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Initialize logger
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));
    // Load .env file
    dotenv().ok();

    // Get configuration from environment
    let host = std::env::var("HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = std::env::var("PORT")
        .unwrap_or_else(|_| "8080".to_string())
        .parse::<u16>()
        .expect("PORT must be a number");

    info!("Starting server at {}:{}", host, port);

    // Initialize services
    services::crypto::init().expect("Failed to initialize crypto service");

    HttpServer::new(|| {
        App::new()
            .wrap(middleware::Logger::default())
            // Register API routes
            .service(
                web::scope("/api")
                    .route("/authorize", web::post().to(authorize))
                    .route("/purchase", web::post().to(purchase)),
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
