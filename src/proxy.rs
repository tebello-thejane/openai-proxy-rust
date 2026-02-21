use axum::{
    body::Bytes,
    extract::Request,
    response::{IntoResponse, Response},
    Json,
};
use reqwest::Client;
use serde_json::Value;
use std::env;
use std::time::Instant;
use uuid::Uuid;
use crate::logging::log_transaction;
use tracing::{info, error};
use axum::http::{HeaderMap, StatusCode, Method};

pub async fn chat_completions(req: Request) -> Response {
    let start_time = Instant::now();
    let unique_id = Uuid::new_v4().to_string();

    // 1. Deconstruct Request
    let (parts, body) = req.into_parts();
    let method = parts.method.clone();
    let uri = parts.uri.clone();
    let headers = parts.headers.clone();

    // 2. Read Request Body
    let body_bytes = match axum::body::to_bytes(body, usize::MAX).await {
        Ok(b) => b,
        Err(e) => {
            error!("Failed to read request body: {}", e);
            return (StatusCode::BAD_REQUEST, "Failed to read body").into_response();
        }
    };

    // Try parsing request body as JSON for pretty logging, else string
    let req_body_json: Value = serde_json::from_slice(&body_bytes)
        .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&body_bytes).to_string()));

    // Serialize headers for logging
    let req_headers_json = headers_to_json(&headers);

    info!("[{}] Received Request: {} {}", unique_id, method, uri);

    // 3. Prepare Upstream Request
    let client = Client::new();
    let upstream_base = env::var("OPENAI_API_BASE").unwrap_or_else(|_| "https://api.openai.com/v1".to_string());
    let target_url = format!("{}/chat/completions", upstream_base.trim_end_matches('/'));

    let mut upstream_req = client
        .post(&target_url)
        .body(body_bytes.clone()); // Cloning bytes is cheap enough for this use case

    // Forward Headers
    for (name, value) in &headers {
        // filter out host to avoid confusion, and length which is recalculated
        if name != "host" && name != "content-length" {
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
                     error!("[{}] Failed to read upstream response body: {}", unique_id, e);
                     return (StatusCode::BAD_GATEWAY, "Upstream error").into_response();
                }
            };

            let resp_body_json: Value = serde_json::from_slice(&resp_bytes)
                .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&resp_bytes).to_string()));
            
            let resp_headers_json = headers_to_json(&resp_headers);

            // 5. Log Transaction
            log_transaction(
                &unique_id,
                method.as_str(),
                uri.to_string().as_str(),
                req_headers_json,
                req_body_json,
                status.as_u16(),
                resp_headers_json,
                resp_body_json,
                latency,
            );

            info!("[{}] Upstream Response: {} ({}ms)", unique_id, status, latency);

            // 6. Return Response to Client
            let mut response = axum::body::Body::from(resp_bytes).into_response();
            *response.status_mut() = status;
            
            // Copy headers back
            for (name, value) in &resp_headers {
                if name != "transfer-encoding" && name != "content-length" {
                      response.headers_mut().insert(name.clone(), value.clone());
                }
            }
            
            response
        }
        Err(e) => {
            error!("[{}] Upstream Request Failed: {}", unique_id, e);
            (StatusCode::BAD_GATEWAY, format!("Upstream failed: {}", e)).into_response()
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
