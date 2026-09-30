use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use super::connection::TrackedConnection;
use crate::messaging::RedisClient;

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
}
