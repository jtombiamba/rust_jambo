use crate::game::service::compute_display_position;
use crate::messaging::events::{GameEvent, GameStartedPlayer, RoomEvent};
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
    /// Map from room ID to list of active tracked connections.
    room_connections: HashMap<Uuid, Vec<TrackedConnection>>,
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
                redis_client,
                db,
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

    /// Remove all connections for a specific player from a game without sending
    /// a PlayerDisconnected event (the player was kicked, not disconnected).
    /// This prevents the kicked player from receiving further game events.
    pub async fn remove_player_connections(&self, game_id: Uuid, player_id: Uuid) {
        let mut inner = self.inner.write().await;
        if let Some(connections) = inner.connections.get_mut(&game_id) {
            let before = connections.len();
            connections.retain(|c| c.player_id != Some(player_id));
            let removed = before - connections.len();
            if connections.is_empty() {
                inner.connections.remove(&game_id);
                tracing::info!(
                    "No more connections for game {} after removing player {}",
                    game_id,
                    player_id
                );
            }
            if removed > 0 {
                metrics::WS_CONNECTIONS_ACTIVE.sub(removed as f64);
                tracing::info!(
                    "Removed {} connection(s) for kicked player {} from game {}",
                    removed,
                    player_id,
                    game_id
                );
            }
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

    /// Send a structured error message to all connections of a specific game.
    pub async fn send_error(&self, game_id: Uuid, message: &str, source: &str) {
        let error_msg =
            serde_json::to_string(&crate::websocket::messages::OutgoingMessage::Error {
                message: message.to_string(),
                source: source.to_string(),
            })
            .unwrap_or_else(|_| {
                serde_json::json!({"type":"error","message":message,"source":source}).to_string()
            });
        self.broadcast_to_game(game_id, &error_msg).await;
    }

    /// Send a structured error message to a specific player within a game.
    #[allow(dead_code)]
    pub async fn send_error_to_player(
        &self,
        game_id: Uuid,
        player_id: Uuid,
        message: &str,
        source: &str,
    ) {
        let error_msg =
            serde_json::to_string(&crate::websocket::messages::OutgoingMessage::Error {
                message: message.to_string(),
                source: source.to_string(),
            })
            .unwrap_or_else(|_| {
                serde_json::json!({"type":"error","message":message,"source":source}).to_string()
            });
        self.send_to_player(game_id, player_id, &error_msg).await;
    }

    /// Send a message to only the connections belonging to a specific player within a game.
    pub async fn send_to_player(&self, game_id: Uuid, player_id: Uuid, message: &str) {
        let inner = self.inner.read().await;
        if let Some(connections) = inner.connections.get(&game_id) {
            metrics::WS_MESSAGES_SENT_TOTAL.inc();
            for connection in connections {
                if connection.player_id == Some(player_id) {
                    if let Err(e) = connection.sender.send(message.to_string()) {
                        tracing::debug!(
                            "Failed to send message to player {} (conn {}): {}",
                            player_id,
                            connection.id.uuid(),
                            e
                        );
                    }
                }
            }
        } else {
            tracing::debug!(
                "No connections for game {}, message not sent to player {}",
                game_id,
                player_id
            );
        }
    }

    /// Get the list of (player_id, player_position) for all connected players in a game.
    /// Returns only connections that have both a known player_id and player_position.
    pub async fn get_connected_player_info(&self, game_id: Uuid) -> Vec<(Uuid, i32)> {
        let inner = self.inner.read().await;
        if let Some(connections) = inner.connections.get(&game_id) {
            connections
                .iter()
                .filter_map(|c| c.player_id.zip(c.player_position))
                .collect()
        } else {
            Vec::new()
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

    /// Add a new WebSocket connection for a given room.
    pub async fn add_room_connection(
        &self,
        room_id: Uuid,
        sender: WsSender,
        correlation_id: CorrelationId,
    ) -> ConnectionId {
        let mut inner = self.inner.write().await;
        let connection_id = ConnectionId::new();

        inner
            .room_connections
            .entry(room_id)
            .or_default()
            .push(TrackedConnection {
                sender,
                id: connection_id,
                correlation_id,
                last_activity: Instant::now(),
                player_id: None,
                player_position: None,
            });

        metrics::WS_CONNECTIONS_ACTIVE.inc();
        tracing::info!(
            "Added room connection {} (correlation_id={}) for room {}",
            connection_id.uuid(),
            correlation_id,
            room_id
        );
        connection_id
    }

    /// Remove a WebSocket connection for a room.
    pub async fn remove_room_connection(&self, room_id: Uuid, connection_id: ConnectionId) {
        let mut inner = self.inner.write().await;
        if let Some(connections) = inner.room_connections.get_mut(&room_id) {
            connections.retain(|c| c.id != connection_id);
            if connections.is_empty() {
                inner.room_connections.remove(&room_id);
            }
            metrics::WS_CONNECTIONS_ACTIVE.dec();
            metrics::WS_DISCONNECTS_TOTAL.inc();
            tracing::info!(
                "Removed room connection {} for room {}",
                connection_id.uuid(),
                room_id
            );
        }
    }

    /// Broadcast a message to all connections of a specific room.
    pub async fn broadcast_to_room(&self, room_id: Uuid, message: &str) {
        let inner = self.inner.read().await;
        if let Some(connections) = inner.room_connections.get(&room_id) {
            let count = connections.len();
            metrics::WS_MESSAGES_SENT_TOTAL.inc_by(count as f64);
            for connection in connections {
                if let Err(e) = connection.sender.send(message.to_string()) {
                    tracing::debug!(
                        "Failed to send message to room connection {}: {}",
                        connection.id.uuid(),
                        e
                    );
                }
            }
        }
    }
}

include!("manager_redis.rs");

include!("manager_cleanup.rs");

#[cfg(test)]
#[path = "manager_tests.rs"]
mod tests;
