use topcoat::Result;
use topcoat::context::{Cx, app_context};
use topcoat::router::content::websocket::{Message, WebSocket, WebSocketUpgrade};
use topcoat::router::response::Response;
use topcoat::router::route;

use super::super::validate_cached_credentials;
use crate::user_updates::{USER_UPDATED_MESSAGE, UserUpdateSubscription, UserUpdates};

#[route(GET)]
async fn users_connect(cx: &Cx, websocket: WebSocketUpgrade) -> Result<Response> {
    let credentials = validate_cached_credentials(cx).await?;
    let subscription = app_context::<UserUpdates>(cx).subscribe(credentials.user_id);
    websocket.on_upgrade(move |socket| CloudConnection::new(socket, subscription).run())
}

struct CloudConnection {
    socket: WebSocket,
    subscription: UserUpdateSubscription,
}

impl CloudConnection {
    fn new(socket: WebSocket, subscription: UserUpdateSubscription) -> Self {
        Self {
            socket,
            subscription,
        }
    }

    async fn run(mut self) {
        loop {
            tokio::select! {
                message = self.socket.recv() => if self.handle_message(message) { break },
                notification = self.subscription.recv() => if self.handle_notification(notification).await { break },
            }
        }
    }

    fn handle_message(&self, message: Option<Result<Message>>) -> bool {
        matches!(message, Some(Ok(Message::Close(_)) | Err(_)) | None)
    }

    async fn handle_notification(
        &mut self,
        notification: Result<(), tokio::sync::broadcast::error::RecvError>,
    ) -> bool {
        match notification {
            Ok(()) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => self
                .socket
                .send(Message::binary(USER_UPDATED_MESSAGE.as_slice()))
                .await
                .is_err(),
            Err(tokio::sync::broadcast::error::RecvError::Closed) => true,
        }
    }
}
