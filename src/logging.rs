use chrono::Utc;
use serde::Serialize;
use serde_json::Value;

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

fn redact_headers(headers: &Value) -> Value {
    match headers {
        Value::Object(map) => {
            let mut redacted = map.clone();
            if let Some(auth) = redacted.get_mut("authorization") {
                if let Some(s) = auth.as_str() {
                    let redacted_val = if let Some(token) = s.strip_prefix("Bearer ") {
                        if token.len() > 8 {
                            format!(
                                "Bearer {}...{}",
                                &token[..4],
                                &token[token.len() - 4..]
                            )
                        } else {
                            "Bearer [REDACTED]".to_string()
                        }
                    } else {
                        "[REDACTED]".to_string()
                    };
                    *auth = Value::String(redacted_val);
                }
            }
            Value::Object(redacted)
        }
        other => other.clone(),
    }
}

pub async fn log_transaction(
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
    let req_headers = redact_headers(&req_headers);
    let resp_headers = redact_headers(&resp_headers);

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
    let filename = format!("log/tx_{filename_ts}_{id}.json");

    match serde_json::to_string_pretty(&transaction) {
        Ok(json_content) => {
            if let Err(e) = tokio::fs::write(&filename, json_content.as_bytes()).await {
                tracing::error!("Failed to write log file {}: {}", filename, e);
            }
            Some(json_content)
        }
        Err(e) => {
            tracing::error!("Failed to serialize transaction log: {}", e);
            None
        }
    }
}
