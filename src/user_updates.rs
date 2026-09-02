use std::{
    sync::{Arc, LazyLock},
    time::Duration,
};

use anyhow::Context as _;
use dashmap::DashMap;
use rustc_hash::FxBuildHasher;
use serde::Serialize;
use sqlx::postgres::{PgListener, PgNotification};
use tokio::sync::broadcast;

const NOTIFICATION_CHANNEL: &str = "zed_local_cloud_user_updated";
const SUBSCRIBER_CAPACITY: usize = 1;

pub static USER_UPDATED_MESSAGE: LazyLock<Vec<u8>> = LazyLock::new(|| {
    let mut message = Vec::new();
    ciborium::into_writer(&MessageToClient::UserUpdated, &mut message)
        .expect("serializing a fixed cloud protocol message cannot fail");
    message
});

#[derive(Clone, Default)]
pub struct UserUpdates(Arc<InnerUserUpdates>);

#[derive(Default)]
struct InnerUserUpdates {
    subscribers: DashMap<i32, broadcast::Sender<()>, FxBuildHasher>,
}

pub struct UserUpdateSubscription {
    user_id: i32,
    updates: UserUpdates,
    receiver: broadcast::Receiver<()>,
}

impl UserUpdates {
    pub async fn connect(database_url: &str) -> anyhow::Result<Self> {
        let mut listener = PgListener::connect(database_url)
            .await
            .context("failed to connect the user-update listener to Postgres")?;
        listener
            .listen(NOTIFICATION_CHANNEL)
            .await
            .context("failed to listen for user updates")?;

        let updates = Self::default();
        let listener_updates = updates.clone();
        tokio::spawn(async move { listener_updates.receive_notifications(listener).await });
        Ok(updates)
    }

    async fn receive_notifications(&self, mut listener: PgListener) {
        loop {
            match listener.recv().await {
                Ok(notification) => self.handle_notification(&notification),
                Err(error) => self.handle_listener_error(&error).await,
            }
        }
    }

    fn handle_notification(&self, notification: &PgNotification) {
        match notification.payload().parse::<i32>() {
            Ok(user_id) => self.publish(user_id),
            Err(error) => tracing::warn!(
                payload = notification.payload(),
                %error,
                "ignored invalid user-update notification"
            ),
        }
    }

    async fn handle_listener_error(&self, error: &sqlx::Error) {
        tracing::error!(%error, "failed to receive a user-update notification");
        // Avoid a tight retry loop when PostgreSQL remains unavailable.
        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    pub fn subscribe(&self, user_id: i32) -> UserUpdateSubscription {
        let receiver = self
            .0
            .subscribers
            .entry(user_id)
            .or_insert_with(|| broadcast::channel(SUBSCRIBER_CAPACITY).0)
            .subscribe();
        UserUpdateSubscription {
            user_id,
            updates: self.clone(),
            receiver,
        }
    }

    fn publish(&self, user_id: i32) {
        if let Some(sender) = self.0.subscribers.get(&user_id) {
            let _ = sender.send(());
        }
    }
}

impl UserUpdateSubscription {
    pub async fn recv(&mut self) -> Result<(), broadcast::error::RecvError> {
        self.receiver.recv().await
    }
}

impl Drop for UserUpdateSubscription {
    fn drop(&mut self) {
        self.updates
            .0
            .subscribers
            .remove_if(&self.user_id, |_, sender| sender.receiver_count() == 1);
    }
}

#[derive(Serialize)]
enum MessageToClient {
    UserUpdated,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn sends_updates_only_to_the_affected_user() {
        let updates = UserUpdates::default();
        let mut alice = updates.subscribe(1);
        let alice_second_connection = updates.subscribe(1);
        let mut bob = updates.subscribe(2);

        updates.publish(1);

        alice.recv().await.unwrap();
        assert!(bob.receiver.try_recv().is_err());

        drop(alice);
        assert!(updates.0.subscribers.contains_key(&1));
        drop(alice_second_connection);
        drop(bob);
        assert!(updates.0.subscribers.is_empty());
    }

    #[test]
    fn serializes_the_zed_cloud_protocol_message() {
        assert_eq!(&*USER_UPDATED_MESSAGE, b"\x6bUserUpdated");
    }
}
