use axum::Json;
use serde_json::Value;
use tokio::fs;

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
