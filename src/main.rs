use axum::{
    routing::{get, post, options},
    Router,
};
use std::net::SocketAddr;
use tower_http::cors::{Any, CorsLayer};
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;
use std::env;

mod logging;
mod proxy;
mod ui;

#[tokio::main]
async fn main() {
    // 1. Initialize Logging (Stdout only for now, file logging handled per request)
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    // Ensure log directory exists
    let log_dir = "log";
    if !std::path::Path::new(log_dir).exists() {
        std::fs::create_dir(log_dir).expect("Failed to create log directory");
    }

    // 2. Setup CORS
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // 3. Build Router
    let app = Router::new()
        .route("/", get(ui::dashboard))
        .route("/api/transactions", get(ui::list_transactions))
        .route("/test", get(test_handler))
        .route("/v1/chat/completions", post(proxy::chat_completions))
        // Generic OPTIONS handler for preflight checks
        .route("/v1/chat/completions", options(options_handler))
        .layer(cors);

    // 4. Start Server
    let port = env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    let addr_str = format!("0.0.0.0:{}", port);
    let addr: SocketAddr = addr_str.parse().expect("Invalid address");

    info!("Listening on {}", addr);
    
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn test_handler() -> &'static str {
    "OpenAI Proxy is running!"
}

async fn options_handler() {
    // Just return 200 OK with CORS headers (handled by middleware)
}
