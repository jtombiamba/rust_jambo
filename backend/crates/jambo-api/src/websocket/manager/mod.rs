use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::RwLock;
use uuid::Uuid;

use super::connection::{ConnectionId, MessageKind, TrackedConnection, WsMessage, WsSender};
use super::routing::shard_for_id;
use jambo_core::observability::metrics;
use jambo_domain::messaging::RedisClient;

mod cleanup;
mod game;
mod redis;
mod room;
mod run_spectators;
mod spectators;
mod user;

use run_spectators::RunSpectator;

#[cfg(test)]
mod tests;

/// Registry state for a single shard. Connection maps are partitioned by the
/// same `shard_for_id` hash used to route Redis subscriber messages, so a game's
/// connections and its events always land on the same shard and never contend
/// with unrelated games.
struct Shard {
    /// Map from game ID to list of active tracked connections.
    connections: HashMap<Uuid, Vec<TrackedConnection>>,
    /// Map from room ID to list of active tracked connections.
    room_connections: HashMap<Uuid, Vec<TrackedConnection>>,
    /// Map from user ID to list of active tracked connections (user-scoped,
    /// e.g. for real-time invitation push).
    user_connections: HashMap<Uuid, Vec<TrackedConnection>>,
    /// Map from run ID to read-only run-spectator connections that follow the
    /// run across games (re-pointed to each new game_id on `GameStarted`).
    run_spectators: HashMap<Uuid, Vec<RunSpectator>>,
}

impl Shard {
    fn empty() -> Self {
        Self {
            connections: HashMap::new(),
            room_connections: HashMap::new(),
            user_connections: HashMap::new(),
            run_spectators: HashMap::new(),
        }
    }
}

/// The WebSocket manager that coordinates connections and broadcasts.
///
/// Connection state is sharded across `N = min(num_cpus, 8)` [`RwLock`]s instead
/// of one global lock, so a handshake storm (many concurrent `add_connection`
/// / `mark_spectator` / `update_pong` writes) no longer serializes all games
/// behind a single writer. The immutable collaborators (`redis_client`, `db`)
/// are held outside the locks.
#[derive(Clone)]
pub struct WebSocketManager {
    shards: Arc<Vec<RwLock<Shard>>>,
    shard_count: usize,
    redis_client: Option<RedisClient>,
    db: Option<sea_orm::DatabaseConnection>,
}

impl WebSocketManager {
    /// Create a new WebSocket manager with an optional Redis client and database connection.
    pub fn new(redis_client: Option<RedisClient>, db: Option<sea_orm::DatabaseConnection>) -> Self {
        let shard_count = num_cpus::get().clamp(1, 8);
        let shards = (0..shard_count)
            .map(|_| RwLock::new(Shard::empty()))
            .collect();
        Self {
            shards: Arc::new(shards),
            shard_count,
            redis_client,
            db,
        }
    }

    /// Resolve the shard that owns a given entity id (game, room, or user).
    fn shard(&self, id: Uuid) -> &RwLock<Shard> {
        &self.shards[shard_for_id(id, self.shard_count)]
    }

    /// Get the Redis client for publishing events.
    pub async fn redis_client(&self) -> Option<RedisClient> {
        self.redis_client.clone()
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
        let shard = self.shard(game_id).read().await;
        shard
            .connections
            .get(&game_id)
            .map(|conns| conns.iter().map(|c| (c.id, c.sender.clone())).collect())
            .unwrap_or_default()
    }
}
