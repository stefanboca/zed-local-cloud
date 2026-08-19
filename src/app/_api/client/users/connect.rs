use topcoat::{
    Result,
    context::Cx,
    router::{
        content::websocket::{Message, WebSocket, WebSocketUpgrade},
        response::Response,
        route,
    },
};

use super::super::validate_cached_credentials;

#[route(GET)]
async fn users_connect(cx: &Cx, websocket: WebSocketUpgrade) -> Result<Response> {
    validate_cached_credentials(cx).await?;
    websocket.on_upgrade(keep_cloud_connection_alive)
}

async fn keep_cloud_connection_alive(mut socket: WebSocket) {
    while let Some(message) = socket.recv().await {
        match message {
            Ok(Message::Close(_)) | Err(_) => break,
            Ok(_) => {}
        }
    }
}
