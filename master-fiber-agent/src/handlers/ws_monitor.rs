use crate::auth::check_websocket_handshake_origin;
use crate::state::AppState;
use axum::extract::ws::{Message as AxumMessage, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::sync::broadcast;

pub async fn ui_monitor_ws_handler(
    ws: WebSocketUpgrade,
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    if let Err(status) = check_websocket_handshake_origin(&headers, &state.ws_allowed_origins) {
        return status.into_response();
    }

    let ui_broadcast = state.ui_broadcast.clone();
    ws.on_upgrade(move |socket| handle_ui_monitor_socket(socket, ui_broadcast))
}

async fn handle_ui_monitor_socket(socket: WebSocket, broadcast_sender: broadcast::Sender<String>) {
    let (mut ws_tx, _) = socket.split();
    let mut rx = broadcast_sender.subscribe();

    loop {
        tokio::select! {
            result = rx.recv() => {
                match result {
                    Ok(msg) => {
                        if ws_tx.send(AxumMessage::Text(msg)).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped_count)) => {
                        eprintln!(
                            "⚠️ [MONITOR BACKPRESSURE] Dashboard stream lagging behind events. Skipped {skipped_count} messages."
                        );

                        let lag_alert = format!(
                            "{{\"event\":\"SYS_LAG\",\"skipped\":{skipped_count}}}"
                        );

                        if ws_tx.send(AxumMessage::Text(lag_alert)).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        eprintln!("🛑 [MONITOR] Global broadcast channel closed.");
                        break;
                    }
                }
            }
        }
    }
}
