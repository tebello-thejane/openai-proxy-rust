use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    extract::State,
    response::IntoResponse,
};
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::info;

use crate::AppState;

pub type TxBroadcast = broadcast::Sender<String>;

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state.tx_broadcast))
}

async fn handle_socket(mut socket: WebSocket, tx: Arc<TxBroadcast>) {
    let mut rx = tx.subscribe();
    info!("WebSocket client connected");

    loop {
        tokio::select! {
            result = rx.recv() => {
                match result {
                    Ok(json_str) => {
                        if let Ok(tx_value) = serde_json::from_str::<serde_json::Value>(&json_str) {
                            let html = crate::fragments::render_new_tx_card(&tx_value).into_string();
                            if socket.send(Message::Text(html)).await.is_err() {
                                break;
                            }
                        }
                        // If JSON parsing fails, skip silently (don't send malformed data to browser)
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        info!("WebSocket client lagged by {} messages", n);
                        continue;
                    }
                    Err(_) => break,
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {} // ignore client messages
                }
            }
        }
    }

    info!("WebSocket client disconnected");
}
