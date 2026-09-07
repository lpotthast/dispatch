use super::controller::EventController;
use crate::backend::app_state::AppState;
use axum::{
    Extension, Router,
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::{IntoResponse, Response},
    routing::get,
};
use std::sync::Arc;
pub(crate) fn routes<S: Clone + Send + Sync + 'static>() -> Router<S> {
    Router::new()
        .route("/api/events/ws", get(ui_events_ws))
        .layer(axum::middleware::from_fn(controller_context))
}
async fn ui_events_ws(
    Extension(controller): Extension<Arc<EventController>>,
    ws: WebSocketUpgrade,
) -> Response {
    controller.subscribe(ws).await
}
impl EventController {
    async fn subscribe(self: Arc<Self>, ws: WebSocketUpgrade) -> Response {
        ws.on_upgrade(move |socket| handle_ui_events_socket(self.events.clone(), socket))
            .into_response()
    }
}
async fn controller_context(
    Extension(state): Extension<AppState>,
    mut request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    request
        .extensions_mut()
        .insert(state.event_controller.clone());
    next.run(request).await
}
async fn handle_ui_events_socket(
    event_bus: crate::backend::events::UiEventBus,
    mut socket: WebSocket,
) {
    let mut receiver = event_bus.subscribe();
    loop {
        match receiver.recv().await {
            Ok(event) => match serde_json::to_string(&event) {
                Ok(body) => {
                    if socket.send(Message::Text(body.into())).await.is_err() {
                        break;
                    }
                }
                Err(err) => {
                    tracing::warn!("failed to serialize UI event: {err}");
                }
            },
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                continue;
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
        }
    }
}
