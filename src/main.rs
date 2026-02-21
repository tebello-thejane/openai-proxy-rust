use axum::{
    routing::{get, post, options},
    Router,
};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::broadcast;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;
use clap::Parser;

mod logging;
mod proxy;
mod ui;
mod ws;

/// OpenAI API Proxy Server
#[derive(Parser, Debug)]
#[command(name = "openai-proxy")]
#[command(about = "A proxy server for OpenAI API with logging and monitoring")]
struct Cli {
    /// Port number to listen on
    #[arg(short, long, default_value = "3000")]
    port: u16,

    /// Destination URL for proxying requests
    #[arg(short, long, default_value = "https://api.openai.com/v1/chat/completions")]
    dest: String,
}

#[tokio::main]
async fn main() {
    // Parse CLI arguments
    let cli = Cli::parse();

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

    // 3. Create broadcast channel for WebSocket notifications
    let (tx, _rx) = broadcast::channel::<String>(100);
    let tx = Arc::new(tx);

    // Store destination URL and broadcast sender for the proxy handler
    let dest_url = cli.dest.clone();
    let proxy_tx = tx.clone();

    // 4. Build Router
    let app = Router::new()
        .route("/", get(serve_index))
        .route("/api/transactions", get(ui::list_transactions))
        .route("/test", get(test_handler))
        .route("/v1/chat/completions", post(move |req| {
            proxy::chat_completions(req, dest_url.clone(), proxy_tx.clone())
        }))
        // Generic OPTIONS handler for preflight checks
        .route("/v1/chat/completions", options(options_handler))
        .route("/ws", get(ws::ws_handler))
        .nest_service("/static", ServeDir::new("static"))
        .layer(cors)
        .with_state(tx);

    // 5. Start Server
    let addr_str = format!("0.0.0.0:{}", cli.port);
    let addr: SocketAddr = addr_str.parse().expect("Invalid address");

    info!("Listening on {}", addr);
    info!("Proxying to: {}", cli.dest);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn test_handler() -> &'static str {
    "OpenAI Proxy is running!"
}

async fn options_handler() {
    // Just return 200 OK with CORS headers (handled by middleware)
}

async fn serve_index() -> impl axum::response::IntoResponse {
    match tokio::fs::read_to_string("static/index.html").await {
        Ok(content) => axum::response::Html(content),
        Err(_) => axum::response::Html("<h1>Error loading dashboard</h1>".to_string()),
    }
}
