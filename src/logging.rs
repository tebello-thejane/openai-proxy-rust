use serde::Serialize;
use serde_json::Value;
use std::fs::File;
use std::io::Write;
use chrono::Utc;
use std::path::Path;

#[derive(Serialize)]
pub struct Transaction {
    pub id: String,
    pub timestamp: String,
    pub request: RequestLog,
    pub response: ResponseLog,
}

#[derive(Serialize)]
pub struct RequestLog {
    pub method: String,
    pub url: String,
    pub downstream_url: String,
    pub headers: Value,
    pub body: Value,
}

#[derive(Serialize)]
pub struct ResponseLog {
    pub status: u16,
    pub headers: Value,
    pub body: Value,
    pub latency_ms: u128,
}

pub fn log_transaction(
    id: &str,
    req_method: &str,
    req_url: &str,
    downstream_url: &str,
    req_headers: Value,
    req_body: Value,
    resp_status: u16,
    resp_headers: Value,
    resp_body: Value,
    latency_ms: u128,
) -> Option<String> {
    let transaction = Transaction {
        id: id.to_string(),
        timestamp: Utc::now().to_rfc3339(),
        request: RequestLog {
            method: req_method.to_string(),
            url: req_url.to_string(),
            downstream_url: downstream_url.to_string(),
            headers: req_headers,
            body: req_body,
        },
        response: ResponseLog {
            status: resp_status,
            headers: resp_headers,
            body: resp_body,
            latency_ms,
        },
    };

    // Filename: tx_<ISO-timestamp>_<UUID>.json
    // Sanitize timestamp for filename safety
    let filename_ts = transaction.timestamp.replace(":", "-");
    let filename = format!("log/tx_{}_{}.json", filename_ts, id);

    match serde_json::to_string_pretty(&transaction) {
        Ok(json_content) => {
            if let Ok(mut file) = File::create(Path::new(&filename)) {
                if let Err(e) = file.write_all(json_content.as_bytes()) {
                    tracing::error!("Failed to write log file: {}", e);
                }
            } else {
                tracing::error!("Failed to create log file: {}", filename);
            }
            Some(json_content)
        }
        Err(e) => {
            tracing::error!("Failed to serialize transaction log: {}", e);
            None
        }
    }
}
