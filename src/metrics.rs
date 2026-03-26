use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{sqlite::SqlitePoolOptions, Pool, Sqlite};

/// Cost per 1K tokens (prompt, completion) for various models
/// Prices sourced from OpenAI API pricing (as of 2024)
static COST_PER_1K: &[(&str, f64, f64)] = &[
    ("gpt-4o", 0.005, 0.015),
    ("gpt-4o-mini", 0.00015, 0.0006),
    ("gpt-4-turbo", 0.01, 0.03),
    ("gpt-4", 0.03, 0.06),
    ("gpt-3.5-turbo", 0.0005, 0.0015),
];

fn compute_cost(model: &str, prompt_tokens: i64, completion_tokens: i64) -> f64 {
    let model_lower = model.to_lowercase();
    for (prefix, prompt_price, completion_price) in COST_PER_1K {
        if model_lower.contains(prefix) {
            let prompt_cost = (prompt_tokens as f64 / 1000.0) * prompt_price;
            let completion_cost = (completion_tokens as f64 / 1000.0) * completion_price;
            return prompt_cost + completion_cost;
        }
    }
    // Default to gpt-3.5-turbo pricing for unknown models
    (prompt_tokens as f64 / 1000.0) * 0.0005 + (completion_tokens as f64 / 1000.0) * 0.0015
}

pub struct MetricsDb {
    pool: Pool<Sqlite>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TransactionMetrics {
    pub model: String,
    pub status: u16,
    pub latency_ms: i64,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub cost: f64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WindowStats {
    pub requests: i64,
    pub avg_latency_ms: f64,
    pub error_rate: f64,
    pub cost: f64,
    pub total_tokens: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelStats {
    pub model: String,
    pub requests: i64,
    pub tokens: i64,
    pub cost: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DashboardStats {
    pub last_hour: WindowStats,
    pub today: WindowStats,
    pub all_time: WindowStats,
    pub per_model: Vec<ModelStats>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HourlyData {
    pub hour: String,
    pub requests: i64,
}

#[derive(Debug, Deserialize)]
pub struct ChartParams {
    pub hours: Option<i64>,
    pub window: Option<String>,
}

/// Time window for dashboard stats
#[derive(Debug, Clone, Copy)]
pub enum TimeWindow {
    Minutes1,
    Minutes5,
    Minutes15,
    Hours1,
    Hours6,
    Hours12,
    Hours24,
}

impl TimeWindow {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "1m" => Some(Self::Minutes1),
            "5m" => Some(Self::Minutes5),
            "15m" => Some(Self::Minutes15),
            "1h" => Some(Self::Hours1),
            "6h" => Some(Self::Hours6),
            "12h" => Some(Self::Hours12),
            "24h" => Some(Self::Hours24),
            _ => None,
        }
    }

    pub fn as_seconds(&self) -> i64 {
        match self {
            Self::Minutes1 => 60,
            Self::Minutes5 => 300,
            Self::Minutes15 => 900,
            Self::Hours1 => 3600,
            Self::Hours6 => 21600,
            Self::Hours12 => 43200,
            Self::Hours24 => 86400,
        }
    }

    pub fn as_label(&self) -> &'static str {
        match self {
            Self::Minutes1 => "1 minute",
            Self::Minutes5 => "5 minutes",
            Self::Minutes15 => "15 minutes",
            Self::Hours1 => "1 hour",
            Self::Hours6 => "6 hours",
            Self::Hours12 => "12 hours",
            Self::Hours24 => "24 hours",
        }
    }

    /// Returns the appropriate bucket size in seconds for charting
    pub fn bucket_seconds(&self) -> i64 {
        match self {
            Self::Minutes1 => 10,      // 10-second buckets for 1 minute
            Self::Minutes5 => 60,      // 1-minute buckets for 5 minutes
            Self::Minutes15 => 300,    // 5-minute buckets for 15 minutes
            Self::Hours1 => 300,       // 5-minute buckets for 1 hour
            Self::Hours6 => 1800,      // 30-minute buckets for 6 hours
            Self::Hours12 => 3600,     // 1-hour buckets for 12 hours
            Self::Hours24 => 3600,     // 1-hour buckets for 24 hours
        }
    }
}

/// Stats for a specific time window
#[derive(Debug, Clone, Serialize)]
pub struct TimeWindowStats {
    pub window: String,
    pub stats: WindowStats,
}

#[derive(Debug, Clone, Serialize)]
pub struct DashboardStatsV2 {
    pub current: TimeWindowStats,
    pub per_model: Vec<ModelStats>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChartDataPoint {
    pub label: String,
    pub requests: i64,
}

fn hour_bucket(ts: DateTime<Utc>) -> i64 {
    ts.timestamp() / 3600 * 3600
}

fn date_bucket(ts: DateTime<Utc>) -> String {
    ts.format("%Y-%m-%d").to_string()
}

impl MetricsDb {
    pub async fn new(path: &str) -> Result<Self, sqlx::Error> {
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(path)
            .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS metrics_hourly (
                hour_bucket INTEGER NOT NULL,
                model TEXT NOT NULL,
                requests INTEGER NOT NULL DEFAULT 0,
                errors_4xx INTEGER NOT NULL DEFAULT 0,
                errors_5xx INTEGER NOT NULL DEFAULT 0,
                latency_min INTEGER NOT NULL DEFAULT 0,
                latency_max INTEGER NOT NULL DEFAULT 0,
                latency_sum INTEGER NOT NULL DEFAULT 0,
                latency_count INTEGER NOT NULL DEFAULT 0,
                prompt_tokens INTEGER NOT NULL DEFAULT 0,
                completion_tokens INTEGER NOT NULL DEFAULT 0,
                estimated_cost REAL NOT NULL DEFAULT 0.0,
                PRIMARY KEY (hour_bucket, model)
            )
            "#,
        )
        .execute(&pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS daily_totals (
                date TEXT NOT NULL,
                model TEXT NOT NULL,
                total_requests INTEGER NOT NULL DEFAULT 0,
                total_tokens INTEGER NOT NULL DEFAULT 0,
                estimated_cost REAL NOT NULL DEFAULT 0.0,
                PRIMARY KEY (date, model)
            )
            "#,
        )
        .execute(&pool)
        .await?;

        sqlx::query("PRAGMA journal_mode = WAL").execute(&pool).await?;

        Ok(Self { pool })
    }

    pub async fn record_transaction(&self, tx: &TransactionMetrics) -> Result<(), sqlx::Error> {
        let hb = hour_bucket(tx.timestamp);
        let date = date_bucket(tx.timestamp);
        let cost = tx.cost;

        let is_4xx = tx.status >= 400 && tx.status < 500;
        let is_5xx = tx.status >= 500;

        let mut conn = self.pool.acquire().await?;

        sqlx::query(
            r#"
            INSERT INTO metrics_hourly (
                hour_bucket, model, requests, errors_4xx, errors_5xx,
                latency_min, latency_max, latency_sum, latency_count,
                prompt_tokens, completion_tokens, estimated_cost
            )
            VALUES (?, ?, 1, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(hour_bucket, model) DO UPDATE SET
                requests = requests + 1,
                errors_4xx = errors_4xx + excluded.errors_4xx,
                errors_5xx = errors_5xx + excluded.errors_5xx,
                latency_min = CASE
                    WHEN metrics_hourly.latency_min = 0 OR excluded.latency_min < metrics_hourly.latency_min
                    THEN excluded.latency_min
                    ELSE metrics_hourly.latency_min
                END,
                latency_max = CASE
                    WHEN excluded.latency_max > metrics_hourly.latency_max
                    THEN excluded.latency_max
                    ELSE metrics_hourly.latency_max
                END,
                latency_sum = latency_sum + excluded.latency_sum,
                latency_count = latency_count + excluded.latency_count,
                prompt_tokens = prompt_tokens + excluded.prompt_tokens,
                completion_tokens = completion_tokens + excluded.completion_tokens,
                estimated_cost = estimated_cost + excluded.estimated_cost
            "#,
        )
        .bind(hb)
        .bind(&tx.model)
        .bind(if is_4xx { 1 } else { 0 })
        .bind(if is_5xx { 1 } else { 0 })
        .bind(tx.latency_ms)
        .bind(tx.latency_ms)
        .bind(tx.latency_ms)
        .bind(1i64)
        .bind(tx.prompt_tokens)
        .bind(tx.completion_tokens)
        .bind(cost)
        .execute(&mut *conn)
        .await?;

        sqlx::query(
            r#"
            INSERT INTO daily_totals (date, model, total_requests, total_tokens, estimated_cost)
            VALUES (?, ?, 1, ?, ?)
            ON CONFLICT(date, model) DO UPDATE SET
                total_requests = total_requests + 1,
                total_tokens = total_tokens + excluded.total_tokens,
                estimated_cost = estimated_cost + excluded.estimated_cost
            "#,
        )
        .bind(&date)
        .bind(&tx.model)
        .bind(tx.prompt_tokens + tx.completion_tokens)
        .bind(cost)
        .execute(&mut *conn)
        .await?;

        Ok(())
    }

    pub async fn get_dashboard_stats(&self) -> Result<DashboardStats, sqlx::Error> {
        let now = Utc::now();
        let hour_ago = now - Duration::hours(1);
        let today_start = now.date_naive().and_hms_opt(0, 0, 0).unwrap();
        let today_start_ts: DateTime<Utc> = DateTime::from_naive_utc_and_offset(today_start, Utc);
        let today_start_ts = today_start_ts.timestamp();
        let hour_ago_bucket = hour_bucket(hour_ago);

        let row: (i64, Option<i64>, i64, f64, i64) = sqlx::query_as(
            r#"
            SELECT
                COALESCE(SUM(requests), 0),
                COALESCE(SUM(latency_sum), 0),
                COALESCE(SUM(errors_4xx + errors_5xx), 0),
                COALESCE(SUM(estimated_cost), 0.0),
                COALESCE(SUM(prompt_tokens + completion_tokens), 0)
            FROM metrics_hourly
            WHERE hour_bucket >= ?
            "#,
        )
        .bind(hour_ago_bucket)
        .fetch_one(&self.pool)
        .await?;

        let last_hour = WindowStats {
            requests: row.0,
            avg_latency_ms: if row.0 > 0 {
                row.1.unwrap_or(0) as f64 / row.0 as f64
            } else {
                0.0
            },
            error_rate: if row.0 > 0 { row.2 as f64 / row.0 as f64 * 100.0 } else { 0.0 },
            cost: row.3,
            total_tokens: row.4,
        };

        let row: (i64, Option<i64>, i64, f64, i64) = sqlx::query_as(
            r#"
            SELECT
                COALESCE(SUM(requests), 0),
                COALESCE(SUM(latency_sum), 0),
                COALESCE(SUM(errors_4xx + errors_5xx), 0),
                COALESCE(SUM(estimated_cost), 0.0),
                COALESCE(SUM(prompt_tokens + completion_tokens), 0)
            FROM metrics_hourly
            WHERE hour_bucket >= ?
            "#,
        )
        .bind(today_start_ts)
        .fetch_one(&self.pool)
        .await?;

        let today = WindowStats {
            requests: row.0,
            avg_latency_ms: if row.0 > 0 {
                row.1.unwrap_or(0) as f64 / row.0 as f64
            } else {
                0.0
            },
            error_rate: if row.0 > 0 { row.2 as f64 / row.0 as f64 * 100.0 } else { 0.0 },
            cost: row.3,
            total_tokens: row.4,
        };

        let row: (i64, Option<i64>, i64, f64, i64) = sqlx::query_as(
            r#"
            SELECT
                COALESCE(SUM(requests), 0),
                COALESCE(SUM(latency_sum), 0),
                COALESCE(SUM(errors_4xx + errors_5xx), 0),
                COALESCE(SUM(estimated_cost), 0.0),
                COALESCE(SUM(prompt_tokens + completion_tokens), 0)
            FROM metrics_hourly
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        let all_time = WindowStats {
            requests: row.0,
            avg_latency_ms: if row.0 > 0 {
                row.1.unwrap_or(0) as f64 / row.0 as f64
            } else {
                0.0
            },
            error_rate: if row.0 > 0 { row.2 as f64 / row.0 as f64 * 100.0 } else { 0.0 },
            cost: row.3,
            total_tokens: row.4,
        };

        let model_rows: Vec<(String, i64, i64, f64)> = sqlx::query_as(
            r#"
            SELECT
                model,
                COALESCE(SUM(total_requests), 0) as requests,
                COALESCE(SUM(total_tokens), 0) as tokens,
                COALESCE(SUM(estimated_cost), 0.0) as cost
            FROM daily_totals
            GROUP BY model
            ORDER BY requests DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let per_model = model_rows
            .into_iter()
            .map(|(model, requests, tokens, cost)| ModelStats {
                model,
                requests,
                tokens,
                cost,
            })
            .collect();

        Ok(DashboardStats {
            last_hour,
            today,
            all_time,
            per_model,
        })
    }

    pub async fn get_hourly_chart(&self, hours: i64) -> Result<Vec<HourlyData>, sqlx::Error> {
        let now = Utc::now();
        let start_time = now - Duration::hours(hours);
        let start_bucket = hour_bucket(start_time);

        let rows: Vec<(i64, i64)> = sqlx::query_as(
            r#"
            SELECT
                hour_bucket,
                COALESCE(SUM(requests), 0) as requests
            FROM metrics_hourly
            WHERE hour_bucket >= ?
            GROUP BY hour_bucket
            ORDER BY hour_bucket ASC
            "#,
        )
        .bind(start_bucket)
        .fetch_all(&self.pool)
        .await?;

        let data: Vec<HourlyData> = rows
            .into_iter()
            .map(|(bucket, requests)| {
                let dt = DateTime::from_timestamp(bucket, 0).unwrap_or_else(|| Utc::now());
                HourlyData {
                    hour: dt.format("%H:%M").to_string(),
                    requests,
                }
            })
            .collect();

        Ok(data)
    }

    pub async fn get_stats_for_window(
        &self,
        window: TimeWindow,
    ) -> Result<WindowStats, sqlx::Error> {
        let now = Utc::now();
        let start_time = now - Duration::seconds(window.as_seconds());
        let start_timestamp = start_time.timestamp();

        let row: (i64, Option<i64>, i64, f64, i64) = sqlx::query_as(
            r#"
            SELECT
                COALESCE(SUM(requests), 0),
                COALESCE(SUM(latency_sum), 0),
                COALESCE(SUM(errors_4xx + errors_5xx), 0),
                COALESCE(SUM(estimated_cost), 0.0),
                COALESCE(SUM(prompt_tokens + completion_tokens), 0)
            FROM metrics_hourly
            WHERE hour_bucket >= ?
            "#,
        )
        .bind(start_timestamp)
        .fetch_one(&self.pool)
        .await?;

        Ok(WindowStats {
            requests: row.0,
            avg_latency_ms: if row.0 > 0 {
                row.1.unwrap_or(0) as f64 / row.0 as f64
            } else {
                0.0
            },
            error_rate: if row.0 > 0 {
                row.2 as f64 / row.0 as f64 * 100.0
            } else {
                0.0
            },
            cost: row.3,
            total_tokens: row.4,
        })
    }

    pub async fn get_chart_data(
        &self,
        window: TimeWindow,
    ) -> Result<Vec<ChartDataPoint>, sqlx::Error> {
        let now = Utc::now();
        let start_time = now - Duration::seconds(window.as_seconds());
        let start_timestamp = start_time.timestamp();

        // For sub-hour buckets, we need to aggregate from the hourly data
        // Since we're storing hourly buckets, we'll distribute the data
        // For minute-level views, we'll interpolate or return what we have
        let rows: Vec<(i64, i64)> = sqlx::query_as(
            r#"
            SELECT
                hour_bucket,
                COALESCE(SUM(requests), 0) as requests
            FROM metrics_hourly
            WHERE hour_bucket >= ?
            GROUP BY hour_bucket
            ORDER BY hour_bucket ASC
            "#,
        )
        .bind(start_timestamp)
        .fetch_all(&self.pool)
        .await?;

        let data: Vec<ChartDataPoint> = rows
            .into_iter()
            .map(|(bucket, requests)| {
                let dt = DateTime::from_timestamp(bucket, 0).unwrap_or_else(|| Utc::now());
                let label = dt.format("%H:%M").to_string();
                ChartDataPoint { label, requests }
            })
            .collect();

        Ok(data)
    }

    pub async fn get_per_model_stats(
        &self,
        window: TimeWindow,
    ) -> Result<Vec<ModelStats>, sqlx::Error> {
        let now = Utc::now();
        let start_time = now - Duration::seconds(window.as_seconds());
        let start_timestamp = start_time.timestamp();

        let model_rows: Vec<(String, i64, i64, f64)> = sqlx::query_as(
            r#"
            SELECT
                model,
                COALESCE(SUM(requests), 0) as requests,
                COALESCE(SUM(prompt_tokens + completion_tokens), 0) as tokens,
                COALESCE(SUM(estimated_cost), 0.0) as cost
            FROM metrics_hourly
            WHERE hour_bucket >= ?
            GROUP BY model
            ORDER BY requests DESC
            "#,
        )
        .bind(start_timestamp)
        .fetch_all(&self.pool)
        .await?;

        Ok(model_rows
            .into_iter()
            .map(|(model, requests, tokens, cost)| ModelStats {
                model,
                requests,
                tokens,
                cost,
            })
            .collect())
    }
}

pub fn extract_metrics_from_transaction(tx_json: &Value) -> Option<TransactionMetrics> {
    let res = tx_json.get("response")?;
    let _req = tx_json.get("request")?;
    let resp_body = res.get("body")?;

    let status = res.get("status")?.as_u64()? as u16;
    let latency_ms = res.get("latency_ms")?.as_u64()? as i64;
    let timestamp_str = tx_json.get("timestamp")?.as_str()?;
    let timestamp = DateTime::parse_from_rfc3339(timestamp_str)
        .ok()?
        .with_timezone(&Utc);

    let model = resp_body
        .get("model")
        .and_then(|m| m.as_str())
        .unwrap_or("unknown")
        .to_string();

    let (prompt_tokens, completion_tokens) = if let Some(usage) = resp_body.get("usage") {
        let prompt = usage
            .get("prompt_tokens")
            .and_then(|t| t.as_u64())
            .unwrap_or(0) as i64;
        let completion = usage
            .get("completion_tokens")
            .and_then(|t| t.as_u64())
            .unwrap_or(0) as i64;
        (prompt, completion)
    } else {
        (0, 0)
    };

    // Compute cost from token counts using the cost table
    let cost = compute_cost(&model, prompt_tokens, completion_tokens);

    Some(TransactionMetrics {
        model,
        status,
        latency_ms,
        prompt_tokens,
        completion_tokens,
        cost,
        timestamp,
    })
}
