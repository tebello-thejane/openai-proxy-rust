use crate::logging::log_transaction;
use crate::metrics::extract_metrics_from_transaction;
use crate::AppState;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::{
    extract::Request,
    response::{IntoResponse, Response},
};
use flate2::read::GzDecoder;
use serde_json::Value;
use std::io::Read;
use std::time::Instant;
use tracing::{error, info};
use uuid::Uuid;

const MAX_BODY_SIZE: usize = 32 * 1024 * 1024; // 32 MB

const HOP_BY_HOP_HEADERS: &[&str] = &[
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "proxy-connection",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
];

fn is_hop_by_hop(name: &str) -> bool {
    HOP_BY_HOP_HEADERS
        .iter()
        .any(|h| h.eq_ignore_ascii_case(name))
}

pub async fn chat_completions(
    State(state): State<AppState>,
    req: Request,
) -> Response {
    let unique_id = Uuid::new_v4().to_string();

    // 1. Deconstruct Request
    let (parts, body) = req.into_parts();
    let method = parts.method.clone();
    let uri = parts.uri.clone();
    let headers = parts.headers.clone();

    // 2. Read Request Body (bounded to prevent OOM DoS)
    let body_bytes = match axum::body::to_bytes(body, MAX_BODY_SIZE).await {
        Ok(b) => b,
        Err(e) => {
            error!("Failed to read request body: {}", e);
            return (StatusCode::PAYLOAD_TOO_LARGE, "Request body too large").into_response();
        }
    };

    // Try parsing request body as JSON for pretty logging, else string
    let req_body_json: Value = serde_json::from_slice(&body_bytes)
        .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&body_bytes).to_string()));

    // Serialize headers for logging
    let req_headers_json = headers_to_json(&headers);

    info!("[{}] Received Request: {} {}", unique_id, method, uri);

    // 3. Prepare Upstream Request
    let target_url = state.dest_url.clone();

    // TODO: Streaming responses (stream: true) are fully buffered before forwarding.
    // This breaks real-time SSE streaming for LLM APIs. A future fix should tee
    // the response stream for simultaneous forwarding and logging.
    let mut upstream_req = state.client.post(&target_url).body(body_bytes.clone());

    // Forward headers, filtering out hop-by-hop and connection-specific headers
    for (name, value) in &headers {
        if name != "host" && name != "content-length" && !is_hop_by_hop(name.as_str()) {
            upstream_req = upstream_req.header(name, value);
        }
    }

    // 4. Send Request
    let upstream_start = Instant::now();
    match upstream_req.send().await {
        Ok(resp) => {
            let latency = upstream_start.elapsed().as_millis();
            let status = resp.status();
            let resp_headers = resp.headers().clone();

            // Read Response Body
            let resp_bytes = match resp.bytes().await {
                Ok(b) => b,
                Err(e) => {
                    error!(
                        "[{}] Failed to read upstream response body: {}",
                        unique_id, e
                    );
                    return (StatusCode::BAD_GATEWAY, "Upstream error").into_response();
                }
            };

            // Decompress for logging if gzip
            let log_resp_bytes = if resp_headers
                .get("content-encoding")
                .and_then(|v| v.to_str().ok())
                .is_some_and(|ce| ce.contains("gzip"))
            {
                let mut decompressed = Vec::new();
                let mut decoder = GzDecoder::new(std::io::Cursor::new(resp_bytes.clone()));
                if decoder.read_to_end(&mut decompressed).is_ok() {
                    decompressed
                } else {
                    resp_bytes.to_vec()
                }
            } else {
                resp_bytes.to_vec()
            };

            let resp_body_json: Value =
                serde_json::from_slice(&log_resp_bytes).unwrap_or_else(|_| {
                    Value::String(String::from_utf8_lossy(&log_resp_bytes).to_string())
                });

            let resp_headers_json = headers_to_json(&resp_headers);

            // 5. Log Transaction
            let tx_json = log_transaction(
                &unique_id,
                method.as_str(),
                uri.to_string().as_str(),
                &target_url,
                req_headers_json,
                req_body_json,
                status.as_u16(),
                resp_headers_json,
                resp_body_json,
                latency,
            )
            .await;

            // Broadcast to WebSocket clients
            if let Some(json_str) = tx_json {
                let _ = state.tx_broadcast.send(json_str.clone());

                // Extract and record metrics (don't fail request on metrics error)
                if let Ok(tx_value) = serde_json::from_str::<Value>(&json_str) {
                    if let Some(metrics) = extract_metrics_from_transaction(&tx_value) {
                        let _ = state.metrics.record_transaction(&metrics).await;
                    }
                }
            }

            info!(
                "[{}] Upstream Response: {} ({}ms)",
                unique_id, status, latency
            );

            // 6. Return Response to Client
            let mut response = axum::body::Body::from(resp_bytes).into_response();
            *response.status_mut() = status;

            // Copy headers back, filtering hop-by-hop headers
            for (name, value) in &resp_headers {
                if name != "content-length" && !is_hop_by_hop(name.as_str()) {
                    response.headers_mut().insert(name.clone(), value.clone());
                }
            }

            response
        }
        Err(e) => {
            error!("[{}] Upstream Request Failed: {}", unique_id, e);
            (StatusCode::BAD_GATEWAY, format!("Upstream failed: {e}")).into_response()
        }
    }
}

fn headers_to_json(headers: &HeaderMap) -> Value {
    let mut map = serde_json::Map::new();
    for (k, v) in headers {
        let val_str = v.to_str().unwrap_or("<binary>").to_string();
        map.insert(k.to_string(), Value::String(val_str));
    }
    Value::Object(map)
}
