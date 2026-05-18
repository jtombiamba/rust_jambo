use crate::messaging::events::GameEvent;
use crate::messaging::RedisClient;
use crate::observability::metrics;
use crate::observability::CorrelationId;
use chrono::Utc;
use futures_util::StreamExt;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::time;
use uuid::Uuid;

/// A single WebSocket connection is represented by a sender that can forward messages.
pub type WsSender = tokio::sync::mpsc::UnboundedSender<String>;

/// Connection identifier for tracking individual WebSocket connections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConnectionId(pub Uuid);

impl ConnectionId {
    /// Generate a new unique connection ID.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Get the underlying UUID.
    pub fn uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for ConnectionId {
    fn default() -> Self {
        Self::new()
    }
}

/// Represents a tracked WebSocket connection.
struct TrackedConnection {
    sender: WsSender,
    id: ConnectionId,
    #[allow(dead_code)]
    correlation_id: CorrelationId,
    last_activity: Instant,
    player_id: Option<Uuid>,
    player_position: Option<i32>,
}

/// Inner shared state for the WebSocket manager.
struct Inner {
    /// Map from game ID to list of active tracked connections.
    connections: HashMap<Uuid, Vec<TrackedConnection>>,
    /// Redis client for publishing/subscribing to game events.
    redis_client: Option<RedisClient>,
}

/// The WebSocket manager that coordinates connections and broadcasts.
#[derive(Clone)]
pub struct WebSocketManager {
    inner: Arc<RwLock<Inner>>,
}

impl WebSocketManager {
    /// Create a new WebSocket manager with an optional Redis client.
    pub fn new(redis_client: Option<RedisClient>) -> Self {
        Self {
            inner: Arc::new(RwLock::new(Inner {
                connections: HashMap::new(),
                redis_client,
            })),
        }
    }

    /// Add a new WebSocket connection for a given game.
    /// Returns a connection ID that can be used to remove the connection later.
    pub async fn add_connection(
        &self,
        game_id: Uuid,
        sender: WsSender,
        correlation_id: CorrelationId,
    ) -> ConnectionId {
        let mut inner = self.inner.write().await;
        let connection_id = ConnectionId::new();
        let cid_display = correlation_id.to_string();
        let conn_uuid = connection_id.uuid();

        let _was_reconnect = inner
            .connections
            .get(&game_id)
            .map(|conns| conns.iter().any(|c| c.player_id.is_some()))
            .unwrap_or(false);

        inner
            .connections
            .entry(game_id)
            .or_default()
            .push(TrackedConnection {
                sender,
                id: connection_id,
                correlation_id,
                last_activity: Instant::now(),
                player_id: None,
                player_position: None,
            });
        tracing::info!(
            "New WebSocket connection {} for game {} (correlation_id={})",
            conn_uuid,
            game_id,
            cid_display
        );
        metrics::WS_CONNECTIONS_ACTIVE.inc();
        connection_id
    }

    /// Associate a player ID and position with a connection.
    /// If a previous connection for this player_id exists, it's a reconnect.
    pub async fn set_player_for_connection(
        &self,
        game_id: Uuid,
        connection_id: ConnectionId,
        player_id: Uuid,
        player_position: i32,
    ) {
        let mut inner = self.inner.write().await;
        let was_previously_connected = if let Some(connections) = inner.connections.get(&game_id) {
            connections
                .iter()
                .any(|c| c.player_id == Some(player_id) && c.id != connection_id)
        } else {
            false
        };

        if let Some(connections) = inner.connections.get_mut(&game_id) {
            for conn in connections.iter_mut() {
                if conn.id == connection_id {
                    conn.player_id = Some(player_id);
                    conn.player_position = Some(player_position);
                    break;
                }
            }
        }

        drop(inner);

        if was_previously_connected {
            let event = GameEvent::PlayerReconnected {
                game_id,
                player_id,
                player_position,
                reconnected_at: Some(Utc::now().to_rfc3339()),
            };
            self.broadcast_to_game(game_id, &event.to_json()).await;
            tracing::info!(
                "Player {} (position {}) reconnected to game {}",
                player_id,
                player_position,
                game_id
            );
        }
    }

    /// Update the last activity time for a connection.
    #[allow(dead_code)]
    pub async fn update_activity(&self, game_id: Uuid, connection_id: ConnectionId) {
        let mut inner = self.inner.write().await;
        if let Some(connections) = inner.connections.get_mut(&game_id) {
            for connection in connections {
                if connection.id == connection_id {
                    connection.last_activity = Instant::now();
                    break;
                }
            }
        }
    }

    /// Remove a WebSocket connection from a game using its connection ID.
    pub async fn remove_connection(&self, game_id: Uuid, connection_id: ConnectionId) {
        let (removed_player_id, removed_position) = {
            let mut inner = self.inner.write().await;
            if let Some(connections) = inner.connections.get_mut(&game_id) {
                let before = connections.len();
                let player_info = connections
                    .iter()
                    .find(|c| c.id == connection_id)
                    .map(|c| (c.player_id, c.player_position));
                connections.retain(|c| c.id != connection_id);
                let after = connections.len();
                tracing::debug!(
                    "Removed connection {} for game {}, connections before: {}, after: {}",
                    connection_id.uuid(),
                    game_id,
                    before,
                    after
                );
                if connections.is_empty() {
                    inner.connections.remove(&game_id);
                    tracing::info!("No more connections for game {}, removed from map", game_id);
                }
                player_info.unwrap_or((None, None))
            } else {
                tracing::warn!(
                    "Attempted to remove connection {} for game {} but no connections found",
                    connection_id.uuid(),
                    game_id
                );
                (None, None)
            }
        };

        metrics::WS_CONNECTIONS_ACTIVE.dec();
        metrics::WS_DISCONNECTS_TOTAL.inc();

        if let (Some(player_id), Some(position)) = (removed_player_id, removed_position) {
            let event = GameEvent::PlayerDisconnected {
                game_id,
                player_id,
                player_position: position,
                disconnected_at: Some(Utc::now().to_rfc3339()),
            };
            self.broadcast_to_game(game_id, &event.to_json()).await;
            tracing::info!(
                "Player {} (position {}) disconnected from game {}",
                player_id,
                position,
                game_id
            );
        }
    }

    /// Remove all connections for a specific game (e.g., when game ends).
    #[allow(dead_code)]
    pub async fn remove_all_connections(&self, game_id: Uuid) {
        let mut inner = self.inner.write().await;
        if let Some(conns) = inner.connections.remove(&game_id) {
            let count = conns.len();
            metrics::WS_CONNECTIONS_ACTIVE.sub(count as f64);
            metrics::WS_DISCONNECTS_TOTAL.inc_by(count as f64);
            tracing::info!("Removed all {} connections for game {}", count, game_id);
        }
    }

    /// Broadcast a message to all connections of a specific game.
    pub async fn broadcast_to_game(&self, game_id: Uuid, message: &str) {
        let inner = self.inner.read().await;
        if let Some(connections) = inner.connections.get(&game_id) {
            let count = connections.len();
            metrics::WS_MESSAGES_SENT_TOTAL.inc_by(count as f64);
            tracing::debug!("Broadcasting to {} connections for game {}", count, game_id);
            for connection in connections {
                // Ignore errors if the receiver is closed
                if let Err(e) = connection.sender.send(message.to_string()) {
                    tracing::debug!(
                        "Failed to send message to WebSocket connection {}: {}",
                        connection.id.uuid(),
                        e
                    );
                }
            }
        } else {
            tracing::debug!("No connections for game {}, message not broadcast", game_id);
        }
    }

    /// Get the number of active connections for a specific game.
    #[allow(dead_code)]
    pub async fn connection_count(&self, game_id: Uuid) -> usize {
        let inner = self.inner.read().await;
        inner
            .connections
            .get(&game_id)
            .map(|c| c.len())
            .unwrap_or(0)
    }

    /// Check if a specific player has an active connection for a game.
    #[allow(dead_code)]
    pub async fn is_player_connected(&self, game_id: Uuid, player_id: Uuid) -> bool {
        let inner = self.inner.read().await;
        inner
            .connections
            .get(&game_id)
            .map(|conns| conns.iter().any(|c| c.player_id == Some(player_id)))
            .unwrap_or(false)
    }

    /// Set player identity on the most recently added connection for a game.
    /// Used during WS join when player_id/position arrive after initial connection.
    pub async fn set_player_for_latest_connection(
        manager: &WebSocketManager,
        game_id: Uuid,
        player_id: Uuid,
        player_position: i32,
    ) {
        let conn_id = {
            let inner = manager.inner.read().await;
            inner
                .connections
                .get(&game_id)
                .and_then(|conns| conns.last())
                .map(|c| c.id)
        };
        if let Some(cid) = conn_id {
            manager
                .set_player_for_connection(game_id, cid, player_id, player_position)
                .await;
        }
    }

    /// Get total number of active connections across all games.
    #[allow(dead_code)]
    pub async fn total_connection_count(&self) -> usize {
        let inner = self.inner.read().await;
        inner.connections.values().map(|c| c.len()).sum()
    }

    /// Get the Redis client for publishing events.
    pub async fn redis_client(&self) -> Option<RedisClient> {
        let inner = self.inner.read().await;
        inner.redis_client.clone()
    }

    /// Start a background task that subscribes to Redis channels and forwards messages.
    /// Uses N sharded subscriber tasks (N = min(num_cpus, 8)) to distribute load.
    /// This should be called once when the server starts.
    pub async fn start_redis_subscriber(&self) -> anyhow::Result<()> {
        tracing::info!("Starting Redis subscriber");
        let redis_client = match self.redis_client().await {
            Some(client) => client,
            None => {
                tracing::warn!("No Redis client available, skipping Redis subscription");
                return Ok(());
            }
        };

        let shard_count = num_cpus::get().clamp(1, 8);
        tracing::info!(
            "Starting {} sharded Redis subscribers (pattern: game:*)",
            shard_count
        );

        for shard in 0..shard_count {
            let mut redis = redis_client.clone();
            let manager = self.clone();

            tokio::spawn(async move {
                let mut pubsub = match redis.psubscribe(&["game:*"]).await {
                    Ok(ps) => ps,
                    Err(e) => {
                        tracing::error!(
                            "Shard {} failed to subscribe to Redis pattern: {}",
                            shard,
                            e
                        );
                        return;
                    }
                };
                tracing::info!("Redis subscriber shard {}/{} ready", shard, shard_count);

                let mut stream = pubsub.on_message();
                while let Some(msg) = stream.next().await {
                    let channel: String = msg.get_channel().unwrap_or_default();
                    let payload: String = msg.get_payload().unwrap_or_default();
                    tracing::debug!("Redis message on channel {} (shard {})", channel, shard);

                    if let Some(game_id) = Self::extract_game_id_from_channel(&channel) {
                        if Self::shard_for_game(game_id, shard_count) == shard {
                            manager.broadcast_to_game(game_id, &payload).await;
                        }
                    } else {
                        tracing::warn!("Received message on unexpected channel: {}", channel);
                    }
                }
                tracing::info!("Redis subscriber shard {} ended", shard);
            });
        }
        Ok(())
    }

    fn shard_for_game(game_id: Uuid, shard_count: usize) -> usize {
        let bytes = game_id.as_bytes();
        let hash = bytes
            .iter()
            .fold(0u64, |acc, &b| acc.wrapping_mul(31).wrapping_add(b as u64));
        (hash as usize) % shard_count
    }

    /// Extract game ID from a Redis channel name of the form "game:{uuid}".
    fn extract_game_id_from_channel(channel: &str) -> Option<Uuid> {
        const PREFIX: &str = "game:";
        channel.strip_prefix(PREFIX).and_then(|s| s.parse().ok())
    }

    /// Clean up stale connections that haven't had activity for more than `max_idle_duration`.
    /// Returns the number of connections removed.
    pub async fn cleanup_stale_connections(&self, max_idle_duration: Duration) -> usize {
        let mut inner = self.inner.write().await;
        let mut total_removed = 0;
        let now = Instant::now();

        // Iterate over games
        inner.connections.retain(|game_id, connections| {
            // Remove stale connections for this game
            let before = connections.len();
            connections.retain(|conn| {
                let idle_duration = now.duration_since(conn.last_activity);
                idle_duration <= max_idle_duration
            });
            let after = connections.len();
            let removed = before - after;
            total_removed += removed;

            if removed > 0 {
                tracing::info!(
                    "Cleaned up {} stale connections for game {}",
                    removed,
                    game_id
                );
            }

            // Keep the game entry only if there are still connections
            !connections.is_empty()
        });

        if total_removed > 0 {
            tracing::info!("Total stale connections cleaned up: {}", total_removed);
        }
        metrics::WS_CONNECTIONS_ACTIVE.sub(total_removed as f64);
        total_removed
    }

    /// Start a background task that periodically cleans up stale connections.
    /// This should be called once when the server starts.
    pub async fn start_connection_cleanup_task(
        &self,
        cleanup_interval: Duration,
        max_idle_duration: Duration,
    ) {
        let manager = self.clone();
        tokio::spawn(async move {
            let mut interval = time::interval(cleanup_interval);
            loop {
                interval.tick().await;
                let removed = manager.cleanup_stale_connections(max_idle_duration).await;
                if removed > 0 {
                    tracing::debug!("Cleanup task removed {} stale connections", removed);
                }
            }
        });
        tracing::info!(
            "Started connection cleanup task with interval {:?} and max idle {:?}",
            cleanup_interval,
            max_idle_duration
        );
    }
}
