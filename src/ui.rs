use axum::extract::{Query, State};
use axum::Json;
use serde_json::Value;
use tokio::fs;

use crate::metrics::{
    ChartDataPoint, ChartParams, DashboardStats, DashboardStatsV2, HourlyData, TimeWindow,
    TimeWindowStats,
};
use crate::AppState;

pub async fn list_transactions() -> Json<Vec<Value>> {
    let mut entries = Vec::new();

    if let Ok(mut dir) = fs::read_dir("log").await {
        while let Ok(Some(entry)) = dir.next_entry().await {
            let path = entry.path();
            let is_json = path.extension().and_then(|e| e.to_str()) == Some("json");
            if !is_json {
                continue;
            }
            if let Ok(contents) = fs::read_to_string(&path).await {
                if let Ok(val) = serde_json::from_str::<Value>(&contents) {
                    entries.push(val);
                }
            }
        }
    }

    // Sort newest-first by the "timestamp" string field (ISO 8601 — lexicographic sort works)
    entries.sort_by(|a, b| {
        let ta = a.get("timestamp").and_then(|v| v.as_str()).unwrap_or("");
        let tb = b.get("timestamp").and_then(|v| v.as_str()).unwrap_or("");
        tb.cmp(ta)
    });

    Json(entries)
}

pub async fn get_dashboard_stats(
    State(state): State<AppState>,
) -> Json<DashboardStats> {
    match state.metrics.get_dashboard_stats().await {
        Ok(stats) => Json(stats),
        Err(e) => {
            tracing::error!("Failed to get dashboard stats: {}", e);
            Json(DashboardStats {
                last_hour: crate::metrics::WindowStats {
                    requests: 0,
                    avg_latency_ms: 0.0,
                    error_rate: 0.0,
                    estimated_cost: 0.0,
                    total_tokens: 0,
                },
                today: crate::metrics::WindowStats {
                    requests: 0,
                    avg_latency_ms: 0.0,
                    error_rate: 0.0,
                    estimated_cost: 0.0,
                    total_tokens: 0,
                },
                all_time: crate::metrics::WindowStats {
                    requests: 0,
                    avg_latency_ms: 0.0,
                    error_rate: 0.0,
                    estimated_cost: 0.0,
                    total_tokens: 0,
                },
                per_model: vec![],
            })
        }
    }
}

pub async fn get_hourly_chart(
    State(state): State<AppState>,
    Query(params): Query<ChartParams>,
) -> Json<Vec<HourlyData>> {
    let hours = params.hours.unwrap_or(24);
    match state.metrics.get_hourly_chart(hours).await {
        Ok(data) => Json(data),
        Err(e) => {
            tracing::error!("Failed to get hourly chart: {}", e);
            Json(vec![])
        }
    }
}

/// New endpoint that supports flexible time windows
pub async fn get_dashboard_stats_v2(
    State(state): State<AppState>,
    Query(params): Query<ChartParams>,
) -> Json<DashboardStatsV2> {
    let window = params
        .window
        .as_deref()
        .and_then(TimeWindow::from_str)
        .unwrap_or(TimeWindow::Hours1);

    match state.metrics.get_stats_for_window(window).await {
        Ok(stats) => {
            let per_model = state
                .metrics
                .get_per_model_stats(window)
                .await
                .unwrap_or_default();

            Json(DashboardStatsV2 {
                current: TimeWindowStats {
                    window: window.as_label().to_string(),
                    stats,
                },
                per_model,
            })
        }
        Err(e) => {
            tracing::error!("Failed to get dashboard stats v2: {}", e);
            Json(DashboardStatsV2 {
                current: TimeWindowStats {
                    window: window.as_label().to_string(),
                    stats: crate::metrics::WindowStats {
                        requests: 0,
                        avg_latency_ms: 0.0,
                        error_rate: 0.0,
                        estimated_cost: 0.0,
                        total_tokens: 0,
                    },
                },
                per_model: vec![],
            })
        }
    }
}

/// New chart endpoint that supports flexible time windows
pub async fn get_chart_data(
    State(state): State<AppState>,
    Query(params): Query<ChartParams>,
) -> Json<Vec<ChartDataPoint>> {
    let window = params
        .window
        .as_deref()
        .and_then(TimeWindow::from_str)
        .unwrap_or(TimeWindow::Hours1);

    match state.metrics.get_chart_data(window).await {
        Ok(data) => Json(data),
        Err(e) => {
            tracing::error!("Failed to get chart data: {}", e);
            Json(vec![])
        }
    }
}
