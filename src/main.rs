use axum::{
    http::StatusCode,
    middleware::{self, Next},
    extract::Request,
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use reqwest::Client;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tower_http::services::ServeDir;
use tracing::info;
use tracing_subscriber::EnvFilter;
use clap::Parser;

mod download;
mod fragments;
mod logging;
mod metrics;
mod proxy;
mod store;
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
    /// Set when `DASHBOARD_TOKEN` env var is configured.
    pub dashboard_token: Option<String>,
}

async fn bearer_auth(
    axum::extract::State(state): axum::extract::State<AppState>,
    req: Request,
    next: Next,
) -> impl IntoResponse {
    let Some(ref expected) = state.dashboard_token else {
        return next.run(req).await.into_response();
    };

    let authorized = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .is_some_and(|token| token == expected);

    if authorized {
        next.run(req).await.into_response()
    } else {
        // RFC 7235 §4.1 requires WWW-Authenticate on every 401.
        (
            StatusCode::UNAUTHORIZED,
            [(axum::http::header::WWW_AUTHENTICATE, "Bearer realm=\"dashboard\"")],
        )
            .into_response()
    }
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

    // 2. Read optional dashboard bearer token
    let dashboard_token = std::env::var("DASHBOARD_TOKEN").ok();
    if dashboard_token.is_some() {
        info!("Dashboard authentication enabled (DASHBOARD_TOKEN is set)");
    }

    // 3. Create shared state
    let (tx, _rx) = broadcast::channel::<String>(100);
    let client = Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .expect("Failed to build HTTP client");
    let state = AppState {
        client,
        dest_url: cli.dest.clone(),
        tx_broadcast: Arc::new(tx),
        metrics: metrics_db,
        dashboard_token,
    };

    // 4. Build Router — protected routes require bearer token when DASHBOARD_TOKEN is set
    let protected = Router::new()
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
        .route("/fragments/transactions", get(fragments::fragment_transactions))
        .route("/fragments/tx/:id", get(fragments::fragment_tx_detail))
        .route("/fragments/stats", get(fragments::fragment_stats))
        .route("/ws", get(ws::ws_handler))
        .nest_service("/static", ServeDir::new("static"))
        .route_layer(middleware::from_fn_with_state(state.clone(), bearer_auth));

    let app = Router::new()
        .merge(protected)
        .route("/test", get(test_handler))
        .route("/v1/chat/completions", post(proxy::chat_completions))
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
