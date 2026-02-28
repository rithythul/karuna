use axum::{
    extract::{Path, State, WebSocketUpgrade, ws::{Message, WebSocket}},
    response::Response,
};
use uuid::Uuid;

use crate::AppState;

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    Path(task_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, task_id, state))
}

async fn handle_socket(mut socket: WebSocket, task_id: Uuid, state: AppState) {
    // Subscribe to Redis Pub/Sub for this task's events
    let mut rx = match state.redis.subscribe_task_events(&task_id.to_string()).await {
        Ok(rx) => rx,
        Err(e) => {
            tracing::error!("Failed to subscribe to task {task_id} events: {e}");
            let _ = socket.send(Message::Text(
                serde_json::json!({"error": "Failed to subscribe to events"}).to_string().into()
            )).await;
            return;
        }
    };

    // Forward events from Redis to WebSocket
    while let Some(event) = rx.recv().await {
        let is_terminal = event.event_type == "task_completed"
            || event.event_type == "task_failed";

        let payload = serde_json::to_string(&event).unwrap_or_default();
        if socket.send(Message::Text(payload.into())).await.is_err() {
            break; // Client disconnected
        }

        if is_terminal {
            break; // Task is done
        }
    }
}
