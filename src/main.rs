use axum::{
    routing::{get, post},
    Router,
};
use reqwest::Client;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::broadcast;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;
use tracing::info;
use tracing_subscriber::EnvFilter;
use clap::Parser;

mod download;
mod fragments;
mod logging;
mod metrics;
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

#[derive(Clone)]
pub struct AppState {
    pub client: Client,
    pub dest_url: String,
    pub tx_broadcast: Arc<broadcast::Sender<String>>,
    pub metrics: Arc<metrics::MetricsDb>,
}

#[tokio::main]
async fn main() {
    // Parse CLI arguments
    let cli = Cli::parse();

    // Load .env file if present (silently ignore if missing)
    dotenv::dotenv().ok();

    // 1. Initialize Logging — respects RUST_LOG env var, defaults to info
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    // Ensure log directory exists (idempotent, no TOCTOU race)
    std::fs::create_dir_all("log").ok();

    // Initialize Metrics database
    let metrics_db = match metrics::MetricsDb::new("sqlite:metrics.db?mode=rwc").await {
        Ok(db) => {
            info!("Metrics database initialized");
            Arc::new(db)
        }
        Err(e) => {
            tracing::error!("Failed to initialize metrics database: {}", e);
            std::process::exit(1);
        }
    };

    // 2. Setup CORS
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // 3. Create shared state
    let (tx, _rx) = broadcast::channel::<String>(100);
    let state = AppState {
        client: Client::new(),
        dest_url: cli.dest.clone(),
        tx_broadcast: Arc::new(tx),
        metrics: metrics_db,
    };

    // 4. Build Router
    let app = Router::new()
        .route("/", get(serve_index))
        .route("/api/transactions", get(ui::list_transactions))
        .route("/api/transactions/summary", get(ui::list_transactions_summary))
        .route("/api/transactions/:id", get(ui::get_transaction))
        .route("/api/transactions/:id/conversation", get(download::download_conversation))
        .route("/api/transactions/:id/response", get(download::download_response))
        .route("/api/metrics/dashboard", get(ui::get_dashboard_stats))
        .route("/api/metrics/dashboard/v2", get(ui::get_dashboard_stats_v2))
        .route("/api/metrics/chart", get(ui::get_hourly_chart))
        .route("/api/metrics/chart/v2", get(ui::get_chart_data))
        .route("/test", get(test_handler))
        .route("/v1/chat/completions", post(proxy::chat_completions))
        .route("/ws", get(ws::ws_handler))
        .nest_service("/static", ServeDir::new("static"))
        .layer(cors)
        .with_state(state);

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

async fn serve_index() -> impl axum::response::IntoResponse {
    match tokio::fs::read_to_string("static/index.html").await {
        Ok(content) => axum::response::Html(content),
        Err(_) => axum::response::Html("<h1>Error loading dashboard</h1>".to_string()),
    }
}
