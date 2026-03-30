use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use maud::{html, Markup};
use serde_json::Value;
use tokio::fs;

use crate::metrics::{ChartParams, TimeWindow, WindowStats};
use crate::AppState;

// Lightweight transaction summary (for list rendering)
#[derive(Debug, serde::Deserialize)]
pub struct TransactionSummary {
    pub id: String,
    pub timestamp: String,
    pub method: Option<String>,
    pub status: Option<u16>,
    pub latency_ms: Option<u64>,
}

// Which part of the transaction to show in detail view
#[derive(Debug, Clone, Copy)]
pub enum DetailSection {
    Request,
    Response,
}

// Query params for detail endpoint
#[derive(Debug, serde::Deserialize)]
pub struct DetailParams {
    pub section: Option<String>,
}

/// Format a timestamp like "2026-03-26T12:00:00.123+00:00" → "2026-03-26 12:00:00"
fn format_timestamp(ts: &str) -> String {
    // Replace 'T' with space
    let with_space = ts.replacen('T', " ", 1);
    // Truncate at '.', '+', or 'Z'
    let truncated = with_space
        .split_once('.')
        .map(|(s, _)| s)
        .or_else(|| with_space.split_once('+').map(|(s, _)| s))
        .or_else(|| with_space.split_once('Z').map(|(s, _)| s))
        .unwrap_or(&with_space);
    truncated.to_string()
}

pub fn render_tx_card(summary: &TransactionSummary) -> Markup {
    let id = &summary.id;
    let formatted_ts = format_timestamp(&summary.timestamp);
    let method = summary.method.as_deref().unwrap_or("?");
    let status_str = summary
        .status
        .map(|s| s.to_string())
        .unwrap_or_else(|| "?".to_string());
    let status_class = match summary.status {
        Some(s) if s >= 200 && s <= 299 => "status-ok",
        _ => "status-err",
    };
    let latency_str = summary
        .latency_ms
        .map(|ms| format!("{} ms", ms))
        .unwrap_or_default();

    let response_hx_get = format!("/fragments/tx/{}?section=response", id);
    let request_hx_get = format!("/fragments/tx/{}?section=request", id);

    html! {
        div class="tx-card" "data-tx-id"=(id) {
            div class="tx-header" {
                span class="ts" { (formatted_ts) }
                span class="method" { (method) }
                span class=(status_class) { (status_str) }
                span class="latency" { (latency_str) }
            }
            details class="response-details"
                "hx-get"=(response_hx_get)
                "hx-trigger"="toggle once"
                "hx-target"="find .detail-content"
                "hx-swap"="innerHTML"
            {
                summary { "Response body" }
                div class="detail-content" {
                    p style="color:#64748b" { "Click to load..." }
                }
            }
            details class="request-details"
                "hx-get"=(request_hx_get)
                "hx-trigger"="toggle once"
                "hx-target"="find .detail-content"
                "hx-swap"="innerHTML"
            {
                summary { "Request body" }
                div class="detail-content" {
                    p style="color:#64748b" { "Click to load..." }
                }
            }
        }
    }
}

pub fn render_tx_detail(tx: &Value, section: DetailSection) -> Markup {
    let body = match section {
        DetailSection::Response => &tx["response"]["body"],
        DetailSection::Request => &tx["request"]["body"],
    };

    let id = tx["id"].as_str().unwrap_or("");
    let timestamp = tx["timestamp"].as_str().unwrap_or("unknown");
    // Make timestamp safe for filenames: replace ':' with '-' and '.' with '-'
    let timestamp_safe = timestamp.replace(':', "-").replace('.', "-");

    let (endpoint, filename, button_label) = match section {
        DetailSection::Response => (
            format!("response"),
            format!("response_{}.md", timestamp_safe),
            "⬇ Response.md",
        ),
        DetailSection::Request => (
            format!("conversation"),
            format!("conversation_{}.md", timestamp_safe),
            "⬇ Conversation.md",
        ),
    };

    let download_url = format!("/api/transactions/{}/{}", id, endpoint);

    let download_script = format!(
        "on click\n  set link to document.createElement('a') then\n  set link.href to '{}' then\n  set link.download to '{}' then\n  call document.body.appendChild(link) then\n  call link.click() then\n  call document.body.removeChild(link)",
        download_url, filename
    );

    let copy_script = "on click call navigator.clipboard.writeText(my.textContent)";

    let body_pretty = serde_json::to_string_pretty(body).unwrap_or_default();

    html! {
        button class="download-md" "_"=(download_script) { (button_label) }
        pre style="cursor:pointer" "_"=(copy_script) {
            (body_pretty)
        }
    }
}

pub fn render_stats(stats: &WindowStats, window: &str) -> Markup {
    let window_label = match window {
        "1m" => "1 minute",
        "5m" => "5 minutes",
        "15m" => "15 minutes",
        "1h" => "1 hour",
        "6h" => "6 hours",
        "12h" => "12 hours",
        "24h" => "24 hours",
        other => other,
    };

    let hx_get = format!("/fragments/stats?window={}", window);
    let avg_latency = format!("{} ms", stats.avg_latency_ms.round() as i64);
    let error_rate = format!("{:.1}%", stats.error_rate);
    let cost = format!("${:.4}", stats.cost);
    let requests_label = format!("Requests ({})", window_label);
    let cost_label = format!("Cost ({})", window_label);

    html! {
        div id="stats-container"
            "hx-get"=(hx_get)
            "hx-trigger"="every 10s, refresh"
            "hx-swap"="outerHTML"
        {
            div class="stat-card" {
                span class="stat-value" id="stat-requests" { (stats.requests) }
                span class="stat-label" id="label-requests" { (requests_label) }
            }
            div class="stat-card" {
                span class="stat-value" id="stat-latency" { (avg_latency) }
                span class="stat-label" { "Avg Latency" }
            }
            div class="stat-card" {
                span class="stat-value" id="stat-errors" { (error_rate) }
                span class="stat-label" { "Error Rate" }
            }
            div class="stat-card" {
                span class="stat-value" id="stat-cost" { (cost) }
                span class="stat-label" id="label-cost" { (cost_label) }
            }
        }
    }
}

pub fn render_tx_list(summaries: &[TransactionSummary]) -> Markup {
    html! {
        div id="transactions" {
            @if summaries.is_empty() {
                p style="color:#475569" { "No transactions yet." }
            } @else {
                @for summary in summaries {
                    (render_tx_card(summary))
                }
            }
        }
    }
}

pub fn render_new_tx_card(tx: &Value) -> Markup {
    let id = tx["id"].as_str().unwrap_or("").to_string();
    let timestamp = tx["timestamp"].as_str().unwrap_or("").to_string();
    let method = tx["request"]["method"].as_str().map(|s| s.to_string());
    let status = tx["response"]["status"].as_u64().map(|n| n as u16);
    let latency_ms = tx["response"]["latency_ms"].as_f64().map(|n| n as u64);

    let summary = TransactionSummary {
        id,
        timestamp,
        method,
        status,
        latency_ms,
    };

    html! {
        div "hx-swap-oob"="afterbegin:#transactions" {
            (render_tx_card(&summary))
        }
    }
}

pub async fn fragment_transactions() -> Markup {
    let mut summaries: Vec<TransactionSummary> = Vec::new();

    if let Ok(mut dir) = fs::read_dir("log").await {
        while let Ok(Some(entry)) = dir.next_entry().await {
            let path = entry.path();
            let is_json = path.extension().and_then(|e| e.to_str()) == Some("json");
            if !is_json {
                continue;
            }
            if let Ok(contents) = fs::read_to_string(&path).await {
                if let Ok(val) = serde_json::from_str::<Value>(&contents) {
                    // Build a TransactionSummary from the full Value
                    let id = val["id"].as_str().unwrap_or("").to_string();
                    let timestamp = val["timestamp"].as_str().unwrap_or("").to_string();
                    let method = val["request"]["method"].as_str().map(|s| s.to_string());
                    let status = val["response"]["status"].as_u64().map(|n| n as u16);
                    let latency_ms = val["response"]["latency_ms"].as_f64().map(|n| n as u64);

                    summaries.push(TransactionSummary {
                        id,
                        timestamp,
                        method,
                        status,
                        latency_ms,
                    });
                }
            }
        }
    }

    // Sort newest-first by timestamp (ISO 8601 sorts lexicographically)
    summaries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    render_tx_list(&summaries)
}

pub async fn fragment_tx_detail(
    Path(id): Path<String>,
    Query(params): Query<DetailParams>,
) -> Result<Markup, StatusCode> {
    if let Ok(mut dir) = fs::read_dir("log").await {
        while let Ok(Some(entry)) = dir.next_entry().await {
            let path = entry.path();
            let filename = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");
            if filename.ends_with(&format!("_{}.json", id)) {
                let contents = match fs::read_to_string(&path).await {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::warn!("Failed to read transaction file {:?}: {}", path, e);
                        continue;
                    }
                };
                let tx = match serde_json::from_str::<Value>(&contents) {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::warn!("Failed to parse transaction file {:?}: {}", path, e);
                        continue;
                    }
                };
                let section = match params.section.as_deref() {
                    Some("request") => DetailSection::Request,
                    _ => DetailSection::Response,
                };
                return Ok(render_tx_detail(&tx, section));
            }
        }
    }

    Err(StatusCode::NOT_FOUND)
}

pub async fn fragment_stats(
    State(state): State<AppState>,
    Query(params): Query<ChartParams>,
) -> Markup {
    let window = params
        .window
        .as_deref()
        .and_then(TimeWindow::from_str)
        .unwrap_or(TimeWindow::Hours1);

    let window_str = params.window.as_deref().unwrap_or("1h");

    let stats = state
        .metrics
        .get_stats_for_window(window)
        .await
        .unwrap_or(WindowStats {
            requests: 0,
            avg_latency_ms: 0.0,
            error_rate: 0.0,
            cost: 0.0,
            total_tokens: 0,
        });

    render_stats(&stats, window_str)
}
