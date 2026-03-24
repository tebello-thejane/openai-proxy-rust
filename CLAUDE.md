# CLAUDE.md — AI Assistant Guide for openai-proxy-rust

## Project Overview

`openai-proxy-rust` is an HTTP proxy server for the OpenAI API, written in Rust. It intercepts requests to OpenAI, logs them to disk and SQLite, and exposes a real-time monitoring dashboard with WebSocket updates.

**Key capabilities:**
- Forwards `POST /v1/chat/completions` to a configurable upstream (default: OpenAI)
- Logs full request/response transactions to JSON files under `log/`
- Stores aggregated metrics in a SQLite database (`metrics.db`)
- Serves a dark-themed web dashboard at `http://localhost:3000/`
- Broadcasts each transaction to WebSocket clients in real time
- Exports conversations/responses as Markdown files

---

## Repository Structure

```
openai-proxy-rust/
├── src/
│   ├── main.rs        # Entry point, CLI args, router, shared state setup
│   ├── proxy.rs       # Core proxy handler (request forwarding, header filtering)
│   ├── metrics.rs     # SQLite-backed metrics collection and dashboard stats
│   ├── logging.rs     # Per-request JSON transaction logging to disk
│   ├── download.rs    # Markdown export endpoints for conversations/responses
│   ├── ui.rs          # REST API endpoints for the dashboard
│   └── ws.rs          # WebSocket upgrade handler for real-time updates
├── static/
│   ├── index.html     # Dashboard HTML
│   ├── app.js         # Dashboard JavaScript (WebSocket, metrics refresh, UI)
│   └── styles.css     # Dark-mode dashboard styles
├── Cargo.toml         # Dependencies and project metadata
├── Cargo.lock         # Locked dependency versions
├── .editorconfig      # Editor formatting rules
├── .gitignore         # Ignores /target, /log, metrics.db*, IDE files
└── CLAUDE.md          # This file
```

**Runtime-generated (gitignored):**
- `log/tx_<ISO-timestamp>_<UUID>.json` — transaction log files
- `metrics.db`, `metrics.db-wal`, `metrics.db-shm` — SQLite database files

---

## Architecture & Request Flow

```
Client Request
     │
     ▼
Axum Router (main.rs)
     │
     ├── POST /v1/chat/completions ──► proxy.rs: chat_completions()
     │                                    │
     │                              Forward to upstream
     │                              (OpenAI or --dest)
     │                                    │
     │                              Read + decompress response
     │                                    │
     │                              logging.rs: log_transaction()
     │                              ├── Write JSON to log/
     │                              └── Return JSON string
     │                                    │
     │                              metrics.rs: record_transaction()
     │                              └── Upsert hourly/daily SQLite rows
     │                                    │
     │                              Broadcast via tokio::broadcast
     │                                    │
     │                              Return response to client
     │
     ├── GET /ws ──────────────────► ws.rs: ws_handler()
     │                              Subscribe to broadcast channel
     │                              Push each transaction to client
     │
     ├── GET /api/transactions ────► ui.rs: list_transactions()
     ├── GET /api/metrics/* ───────► ui.rs: get_dashboard_stats*(), get_chart_data*()
     └── GET /api/transactions/:id/conversation|response ──► download.rs
```

### Shared State (`AppState`)

Defined in `main.rs`, injected into all handlers via Axum state:

| Field | Type | Purpose |
|-------|------|---------|
| `client` | `reqwest::Client` | HTTP client for upstream requests |
| `dest` | `String` | Upstream URL (default: `https://api.openai.com`) |
| `tx` | `broadcast::Sender<String>` | Pub/sub channel for WebSocket broadcast |
| `metrics_db` | `Arc<MetricsDb>` | Thread-safe metrics database handle |

---

## Module Responsibilities

### `src/main.rs`
- CLI parsing via `clap`: `--port` (default 3000), `--dest` (default OpenAI endpoint)
- `dotenv` loading for environment variables
- Tracing/logging initialization
- SQLite database initialization
- CORS layer configuration (permissive — all origins)
- All route definitions

### `src/proxy.rs`
- `chat_completions()` — the core proxy handler
- Generates a UUID per request for tracking
- Enforces 32 MB request body size limit
- Filters hop-by-hop headers before forwarding (`connection`, `upgrade`, `transfer-encoding`, `keep-alive`, `proxy-authenticate`, `proxy-authorization`, `te`, `trailers`)
- Also removes `host` and `content-length` before forwarding
- Decompresses gzip responses using `flate2` before logging
- **Known limitation:** Responses are fully buffered before forwarding. This breaks SSE/streaming from OpenAI. A TODO exists in the code.

### `src/metrics.rs`
- `MetricsDb` wraps a SQLite connection pool (max 5 connections, WAL mode)
- Schema: `metrics_hourly` (per model, per hour) and `daily_totals` (per model, per day)
- `record_transaction()` — upserts metrics using `INSERT OR REPLACE`
- `get_dashboard_stats()` — all-time, last-hour, today aggregates (v1 API)
- `get_stats_for_window()` — flexible `TimeWindow` enum (1m/5m/15m/1h/6h/12h/24h)
- `get_chart_data()` — time-series data for charting with configurable granularity
- Cost table (hardcoded, per 1K tokens):

| Model | Prompt | Completion |
|-------|--------|------------|
| gpt-4o | $0.005 | $0.015 |
| gpt-4o-mini | $0.00015 | $0.0006 |
| gpt-4 | $0.030 | $0.060 |
| gpt-4-turbo | $0.010 | $0.030 |
| gpt-3.5-turbo | $0.0005 | $0.0015 |

### `src/logging.rs`
- `Transaction` / `RequestLog` / `ResponseLog` structures
- Writes `log/tx_<timestamp>_<uuid>.json` for every proxied request
- `redact_headers()` — redacts `Authorization` bearer tokens (keeps first 4 + last 4 chars)
- Returns JSON string for WebSocket broadcast

### `src/download.rs`
- `download_conversation()` — serves request messages as Markdown (`=== Role ===` format)
- `download_response()` — serves response choices as Markdown
- `load_transaction()` — scans `log/` directory for matching UUID filename pattern
- Sanitizes escaped `\n` sequences to actual newlines

### `src/ui.rs`
- `list_transactions()` — reads all files from `log/`, returns sorted newest-first
- `get_dashboard_stats()` / `get_dashboard_stats_v2()` — delegate to `MetricsDb`
- `get_hourly_chart()` / `get_chart_data()` — delegate to `MetricsDb`
- Returns zero/empty stats on database errors (non-fatal)

### `src/ws.rs`
- Axum WebSocket upgrade handler
- Each connected client subscribes to the broadcast channel
- Forwards each transaction JSON string to the client
- Detects and logs broadcast lag (slow consumers)
- Handles client close frames gracefully

---

## API Endpoints

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| `POST` | `/v1/chat/completions` | `proxy.rs` | Main proxy endpoint |
| `GET` | `/` | static | Dashboard HTML |
| `GET` | `/static/*` | static | Static assets |
| `GET` | `/ws` | `ws.rs` | WebSocket upgrade |
| `GET` | `/test` | inline | Health check → `"ok"` |
| `GET` | `/api/transactions` | `ui.rs` | List all logged transactions |
| `GET` | `/api/transactions/:id/conversation` | `download.rs` | Download conversation as Markdown |
| `GET` | `/api/transactions/:id/response` | `download.rs` | Download response as Markdown |
| `GET` | `/api/metrics/dashboard` | `ui.rs` | Dashboard stats (v1) |
| `GET` | `/api/metrics/dashboard/v2` | `ui.rs` | Dashboard stats with time window |
| `GET` | `/api/metrics/chart` | `ui.rs` | Hourly chart data (v1) |
| `GET` | `/api/metrics/chart/v2` | `ui.rs` | Chart data with time window |

---

## Development Workflow

### Build & Run

```bash
# Debug build
cargo build

# Release build (optimized)
cargo build --release

# Run with defaults (port 3000, forwarding to OpenAI)
cargo run

# Run with custom port and destination
cargo run -- --port 8080 --dest https://api.openai.com

# Or after release build
./target/release/openai-proxy-rust --port 3000
```

### Environment Variables

The server loads `.env` via `dotenv`. Relevant variable:
- `RUST_LOG` — controls log level (e.g., `RUST_LOG=debug cargo run`)

There is no built-in API key management; the proxy forwards the `Authorization` header from the client as-is.

### Testing

**There are currently no automated tests** (`#[cfg(test)]` modules, `tests/` directory, or integration tests).

Manual testing:
```bash
# Health check
curl http://localhost:3000/test

# Proxy a chat completion (requires valid OpenAI key)
curl http://localhost:3000/v1/chat/completions \
  -H "Authorization: Bearer sk-..." \
  -H "Content-Type: application/json" \
  -d '{"model":"gpt-4o-mini","messages":[{"role":"user","content":"Hello"}]}'
```

When adding tests, use `#[cfg(test)]` inside the relevant module or create `tests/` for integration tests. The async runtime for tests is `tokio::test`.

### Code Style & Conventions

- **Edition:** Rust 2021
- **Formatting:** Standard `rustfmt` (4-space indentation for Rust, enforced by `.editorconfig`)
- **Error handling:** Use `?` for propagation; API handlers return `(StatusCode, Json<Value>)` or similar Axum response types
- **Async:** All handlers are `async fn`; use `tokio::spawn` for background tasks
- **Logging:** Use `tracing::{info, warn, error, debug}` macros — never `println!` in production code
- **State sharing:** Wrap shared mutable state in `Arc<Mutex<T>>` or `Arc<T>` with interior mutability; prefer `Arc<T>` with SQLx pools for DB access
- **JSON files (config/data):** 2-space indentation (per `.editorconfig`)

### Adding a New Endpoint

1. Define the handler function as `async fn` in the appropriate module (or create a new module)
2. Add the route in `main.rs` inside the `Router::new()` chain
3. If the handler needs state, add `.with_state(state.clone())`
4. If the handler needs new state fields, update `AppState` in `main.rs`

### Adding New Metrics

1. Add fields to the relevant structs in `metrics.rs` (`TransactionMetrics`, `WindowStats`, etc.)
2. Update `extract_metrics_from_transaction()` to parse new fields
3. Update `record_transaction()` SQL to persist new data
4. Update `get_dashboard_stats()` / `get_stats_for_window()` SQL queries accordingly
5. Expose via `ui.rs` endpoints and update `app.js` to display

---

## Key Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| `axum` | 0.7 | Web framework with routing, extractors, WebSocket |
| `tokio` | 1.0 (full) | Async runtime |
| `reqwest` | 0.12 | Upstream HTTP client (rustls TLS, streaming) |
| `serde` / `serde_json` | 1.0 | JSON serialization/deserialization |
| `sqlx` | 0.7 | Async SQLite with compile-time query checking |
| `tracing` / `tracing-subscriber` | 0.1/0.3 | Structured logging |
| `tower-http` | 0.5 | CORS, tracing, static file serving middleware |
| `clap` | 4.0 (derive) | CLI argument parsing |
| `chrono` | 0.4 | Timestamps and date arithmetic |
| `uuid` | 1.0 (v4) | Transaction ID generation |
| `flate2` | 1.0 | Gzip decompression for response bodies |
| `dotenv` | 0.15 | `.env` file loading |

---

## Known Limitations & TODOs

1. **Streaming (SSE) not supported:** `proxy.rs` buffers the full response before returning it. The `stream` feature of `reqwest` is a dependency but unused for the proxy path. This breaks `stream: true` requests to OpenAI.

2. **No authentication:** The proxy itself has no access control. Anyone who can reach port 3000 can use it and view all transaction logs.

3. **No tests:** The codebase has zero automated test coverage.

4. **Hardcoded cost table:** Model pricing in `metrics.rs` is hardcoded and will drift as OpenAI updates pricing.

5. **Filesystem log scanning:** `list_transactions()` and `load_transaction()` scan the entire `log/` directory on each request, which will degrade with large numbers of transactions.

6. **WebSocket broadcasts all data:** All connected WebSocket clients receive all transaction data, including (redacted) auth headers.

---

## Security Notes

- **Authorization headers** are redacted in logs (first 4 + last 4 chars of bearer token preserved for debugging)
- **Request body size** is capped at 32 MB to prevent DoS
- **Hop-by-hop headers** are filtered before forwarding to prevent protocol issues
- The `log/` directory and `metrics.db` should be protected with filesystem permissions in production since they contain full request/response bodies
- CORS is configured to allow all origins — restrict this for production deployments
