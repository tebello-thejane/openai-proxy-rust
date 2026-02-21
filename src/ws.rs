use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    extract::State,
    response::IntoResponse,
};
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::info;

pub type TxBroadcast = broadcast::Sender<String>;

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(tx): State<Arc<TxBroadcast>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, tx))
}

async fn handle_socket(mut socket: WebSocket, tx: Arc<TxBroadcast>) {
    let mut rx = tx.subscribe();
    info!("WebSocket client connected");

    loop {
        tokio::select! {
            result = rx.recv() => {
                match result {
                    Ok(msg) => {
                        if socket.send(Message::Text(msg.into())).await.is_err() {
                            break;
                        }
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
