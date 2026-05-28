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
                room_connections: HashMap::new(),
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
                let mut pubsub = match redis.psubscribe(&["game:*", "room:*"]).await {
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
                            match serde_json::from_str::<GameEvent>(&payload) {
                                Ok(event) => {
                                    manager.route_event(game_id, event).await;
                                }
                                Err(e) => {
                                    tracing::error!(
                                        "Failed to parse game event for game {}: {}",
                                        game_id,
                                        e
                                    );
                                    manager.broadcast_to_game(game_id, &payload).await;
                                }
                            }
                        }
                    } else if let Some(room_id) = Self::extract_room_id_from_channel(&channel) {
                        if Self::shard_for_game(room_id, shard_count) == shard {
                            match serde_json::from_str::<RoomEvent>(&payload) {
                                Ok(event) => {
                                    manager.route_room_event(room_id, event).await;
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        "Failed to parse room event for room {}: {}",
                                        room_id,
                                        e
                                    );
                                    manager.broadcast_to_room(room_id, &payload).await;
                                }
                            }
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

    /// Route a parsed game event to the appropriate delivery method.
    async fn route_event(&self, game_id: Uuid, event: GameEvent) {
        match &event {
            GameEvent::CardsDealt { player_id, .. } => {
                self.send_to_player(game_id, *player_id, &event.to_json())
                    .await;
            }
            GameEvent::GameStarted { .. } => {
                self.send_game_started_per_player(game_id, &event).await;
            }
            _ => {
                self.broadcast_to_game(game_id, &event.to_json()).await;
            }
        }
    }

    /// Send a personalized GameStarted event to each player with `display_position`
    /// rotated so that the receiving player is always at position 0 (south).
    async fn send_game_started_per_player(&self, game_id: Uuid, event: &GameEvent) {
        let (players, current_turn, correlation_id) = match event {
            GameEvent::GameStarted {
                players,
                current_turn,
                correlation_id,
                ..
            } => (players, current_turn, correlation_id),
            _ => return,
        };

        let num_players = players.len();

        for player in players {
            let my_position = player.position as usize;

            let rotated_players: Vec<GameStartedPlayer> = players
                .iter()
                .map(|p| {
                    let display_pos =
                        compute_display_position(p.position as usize, num_players, my_position);
                    GameStartedPlayer {
                        id: p.id,
                        name: p.name.clone(),
                        position: p.position,
                        display_position: display_pos as i32,
                        cards_count: p.cards_count,
                        player_type: p.player_type.clone(),
                    }
                })
                .collect();

            let personalized = GameEvent::GameStarted {
                game_id,
                players: rotated_players,
                current_turn: *current_turn,
                correlation_id: *correlation_id,
            };

            self.send_to_player(game_id, player.id, &personalized.to_json())
                .await;
        }
    }

    fn shard_for_game(game_id: Uuid, shard_count: usize) -> usize {
        let bytes = game_id.as_bytes();
        let hash = bytes
            .iter()
            .fold(0u64, |acc, &b| acc.wrapping_mul(31).wrapping_add(b as u64));
        (hash as usize) % shard_count
    }

    /// Route a parsed room event to broadcast to room connections.
    async fn route_room_event(&self, room_id: Uuid, event: RoomEvent) {
        self.broadcast_to_room(room_id, &event.to_json()).await;
    }

    /// Extract game ID from a Redis channel name of the form "game:{uuid}".
    fn extract_game_id_from_channel(channel: &str) -> Option<Uuid> {
        const PREFIX: &str = "game:";
        channel.strip_prefix(PREFIX).and_then(|s| s.parse().ok())
    }

    /// Extract room ID from a Redis channel name of the form "room:{uuid}".
    fn extract_room_id_from_channel(channel: &str) -> Option<Uuid> {
        const PREFIX: &str = "room:";
        channel.strip_prefix(PREFIX).and_then(|s| s.parse().ok())
    }
}

include!("manager_cleanup.rs");

#[cfg(test)]
#[path = "manager_tests.rs"]
mod tests;
