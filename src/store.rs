use axum::http::StatusCode;
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::fs;
use tracing::warn;
use uuid::Uuid;

/// Parse and validate a transaction id string as a UUID, returning 400 on failure.
/// This prevents header injection and path traversal via unvalidated id strings.
pub fn validate_id(id: &str) -> Result<Uuid, StatusCode> {
    Uuid::parse_str(id).map_err(|_| StatusCode::BAD_REQUEST)
}

/// Load a transaction JSON file by its UUID and deserialize into `Value`.
pub async fn load_tx_value(id: &Uuid) -> Result<Value, StatusCode> {
    load_tx_typed::<Value>(id).await
}

/// Load a transaction JSON file by its UUID and deserialize into any `T`.
pub async fn load_tx_typed<T: DeserializeOwned>(id: &Uuid) -> Result<T, StatusCode> {
    let suffix = format!("_{id}.json");

    let mut dir = fs::read_dir("log").await.map_err(|e| {
        warn!("Failed to read log directory: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    while let Ok(Some(entry)) = dir.next_entry().await {
        let path = entry.path();
        let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if filename.ends_with(&suffix) {
            let contents = fs::read_to_string(&path).await.map_err(|e| {
                warn!("Failed to read transaction file {:?}: {}", path, e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
            return serde_json::from_str(&contents).map_err(|e| {
                warn!("Failed to parse transaction file {:?}: {}", path, e);
                StatusCode::BAD_REQUEST
            });
        }
    }

    warn!("Transaction not found: {}", id);
    Err(StatusCode::NOT_FOUND)
}
