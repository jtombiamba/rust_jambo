use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::RwLock;
use uuid::Uuid;

use super::connection::{ConnectionId, MessageKind, TrackedConnection, WsMessage, WsSender};
use crate::messaging::RedisClient;
use crate::observability::metrics;

mod cleanup;
mod game;
mod redis;
mod room;
mod spectators;
mod user;

#[cfg(test)]
mod tests;

/// Inner shared state for the WebSocket manager.
struct Inner {
    /// Map from game ID to list of active tracked connections.
    connections: HashMap<Uuid, Vec<TrackedConnection>>,
    /// Map from room ID to list of active tracked connections.
    room_connections: HashMap<Uuid, Vec<TrackedConnection>>,
    /// Map from user ID to list of active tracked connections (user-scoped,
    /// e.g. for real-time invitation push).
    user_connections: HashMap<Uuid, Vec<TrackedConnection>>,
    /// Redis client for publishing/subscribing to game events.
    redis_client: Option<RedisClient>,
    /// Database connection for querying game state snapshots.
    db: Option<sea_orm::DatabaseConnection>,
}

/// The WebSocket manager that coordinates connections and broadcasts.
#[derive(Clone)]
pub struct WebSocketManager {
    inner: Arc<RwLock<Inner>>,
}

impl WebSocketManager {
    /// Create a new WebSocket manager with an optional Redis client and database connection.
    pub fn new(redis_client: Option<RedisClient>, db: Option<sea_orm::DatabaseConnection>) -> Self {
        Self {
            inner: Arc::new(RwLock::new(Inner {
                connections: HashMap::new(),
                room_connections: HashMap::new(),
                user_connections: HashMap::new(),
                redis_client,
                db,
            })),
        }
    }

    /// Get the Redis client for publishing events.
    pub async fn redis_client(&self) -> Option<RedisClient> {
        let inner = self.inner.read().await;
        inner.redis_client.clone()
    }

    /// Deliver a message to a set of already-collected connections using a
    /// non-blocking `try_send`. This is called *outside* the state lock so the
    /// read guard is never held across a send.
    ///
    /// Returns the connection ids of slow consumers that must be disconnected
    /// (a `Control` message hit a full queue). A `Snapshot` message that hits a
    /// full queue is simply dropped; a closed receiver increments the failed
    /// counter and is cleaned up by the forwarding task, not here.
    fn deliver(msg: &WsMessage, targets: &[(ConnectionId, WsSender)]) -> Vec<ConnectionId> {
        let mut slow_consumers = Vec::new();
        for (conn_id, sender) in targets {
            match sender.try_send(msg.clone()) {
                Ok(()) => metrics::WS_MESSAGES_SENT_TOTAL.inc(),
                Err(TrySendError::Full(_)) => match msg.kind {
                    MessageKind::Snapshot => {
                        metrics::WS_MESSAGES_DROPPED_TOTAL.inc();
                    }
                    MessageKind::Control => {
                        metrics::WS_SLOW_CONSUMER_DISCONNECTS_TOTAL.inc();
                        slow_consumers.push(*conn_id);
                    }
                },
                Err(TrySendError::Closed(_)) => {
                    metrics::WS_SEND_FAILED_TOTAL.inc();
                }
            }
        }
        slow_consumers
    }

    /// Disconnect connections whose queue overflowed on a `Control` message.
    ///
    /// `force_disconnect` is boxed here to break the async recursion cycle
    /// (`broadcast_to_game` -> `disconnect_slow_consumers` -> `force_disconnect`
    /// -> `remove_connection` -> `broadcast_to_game`).
    async fn disconnect_slow_consumers(&self, game_id: Uuid, conn_ids: Vec<ConnectionId>) {
        for conn_id in conn_ids {
            Box::pin(self.force_disconnect(game_id, conn_id)).await;
        }
    }

    /// Snapshot the sender handles for a game's connections under the read lock,
    /// then release the lock before the caller delivers outside it.
    async fn snapshot_senders(&self, game_id: Uuid) -> Vec<(ConnectionId, WsSender)> {
        let inner = self.inner.read().await;
        inner
            .connections
            .get(&game_id)
            .map(|conns| conns.iter().map(|c| (c.id, c.sender.clone())).collect())
            .unwrap_or_default()
    }
}
