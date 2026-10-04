use chrono::Utc;
use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;

use super::WebSocketManager;
use crate::messaging::events::GameEvent;
use crate::observability::metrics;
use crate::observability::CorrelationId;
use crate::websocket::connection::{
    ConnectionId, MessageKind, TrackedConnection, WsMessage, WsSender,
};

impl WebSocketManager {
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
                disconnected: false,
                last_pong: Instant::now(),
                spectator: false,
            });
        tracing::info!(
            "New WebSocket connection {} for game {} (correlation_id={})",
            conn_uuid,
            game_id,
            cid_display
        );
        // Confirm the game is now present in the map and how many
        // connections it holds, so we can correlate registration timing with
        // any subsequent "No connections" broadcast warnings.
        let known_games: Vec<String> = inner.connections.keys().map(|g| g.to_string()).collect();
        tracing::info!(
            "[WS-DIAG] add_connection: game {} now has {} connection(s); known_games={:?} (count={})",
            game_id,
            inner.connections.get(&game_id).map(|c| c.len()).unwrap_or(0),
            known_games,
            known_games.len()
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
            self.broadcast_to_game(game_id, &event.to_json(), MessageKind::Control)
                .await;
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

    pub async fn update_pong(&self, game_id: Uuid, connection_id: ConnectionId) {
        let mut inner = self.inner.write().await;
        if let Some(connections) = inner.connections.get_mut(&game_id) {
            for connection in connections.iter_mut() {
                if connection.id == connection_id {
                    connection.last_pong = Instant::now();
                    break;
                }
            }
        }
    }

    /// Force-disconnect a connection, always publishing PlayerDisconnected if player
    /// identity was set. Safe to call from the forwarding task even after the stream
    /// handler has already cleaned up.
    /// Delegates to `remove_connection` for actual removal and metric handling.
    pub async fn force_disconnect(&self, game_id: Uuid, connection_id: ConnectionId) {
        let (player_id, position, was_disconnected) = {
            let mut inner = self.inner.write().await;
            let result = if let Some(connections) = inner.connections.get_mut(&game_id) {
                let idx = connections.iter().position(|c| c.id == connection_id);
                match idx {
                    Some(i) => {
                        let conn = &connections[i];
                        if conn.disconnected {
                            (conn.player_id, conn.player_position, true)
                        } else {
                            let (pid, pos) = (conn.player_id, conn.player_position);
                            connections[i].disconnected = true;
                            (pid, pos, false)
                        }
                    }
                    None => (None, None, true),
                }
            } else {
                (None, None, true)
            };
            result
        };

        if was_disconnected {
            return;
        }

        // Delegate removal and metric decrement to remove_connection to avoid double-decrement
        self.remove_connection(game_id, connection_id).await;

        if let (Some(pid), Some(pos)) = (player_id, position) {
            let event = GameEvent::PlayerDisconnected {
                game_id,
                player_id: pid,
                player_position: pos,
                disconnected_at: Some(Utc::now().to_rfc3339()),
            };
            self.broadcast_to_game(game_id, &event.to_json(), MessageKind::Control)
                .await;
        }
    }
    pub async fn remove_connection(&self, game_id: Uuid, connection_id: ConnectionId) {
        let (removed_player_id, removed_position, already_disconnected) = {
            let mut inner = self.inner.write().await;
            if let Some(connections) = inner.connections.get_mut(&game_id) {
                let before = connections.len();
                let conn_info = connections
                    .iter()
                    .find(|c| c.id == connection_id)
                    .map(|c| (c.player_id, c.player_position, c.disconnected));
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
                // Record the post-removal map state so we can tell
                // whether a later "No connections" broadcast raced with this
                // removal (i.e. the game was present before, gone after).
                let known_games: Vec<String> =
                    inner.connections.keys().map(|g| g.to_string()).collect();
                tracing::info!(
                    "[WS-DIAG] remove_connection: game {} now has {} connection(s); known_games={:?} (count={})",
                    game_id,
                    inner.connections.get(&game_id).map(|c| c.len()).unwrap_or(0),
                    known_games,
                    known_games.len()
                );
                conn_info.unwrap_or((None, None, true))
            } else {
                tracing::warn!(
                    "Attempted to remove connection {} for game {} but no connections found",
                    connection_id.uuid(),
                    game_id
                );
                (None, None, true)
            }
        };

        metrics::WS_CONNECTIONS_ACTIVE.dec();
        metrics::WS_DISCONNECTS_TOTAL.inc();
        self.refresh_spectator_gauge(game_id).await;

        if already_disconnected {
            return;
        }

        if let (Some(player_id), Some(position)) = (removed_player_id, removed_position) {
            let event = GameEvent::PlayerDisconnected {
                game_id,
                player_id,
                player_position: position,
                disconnected_at: Some(Utc::now().to_rfc3339()),
            };
            self.broadcast_to_game(game_id, &event.to_json(), MessageKind::Control)
                .await;
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
        let removed = {
            let mut inner = self.inner.write().await;
            inner.connections.remove(&game_id).map(|conns| {
                let count = conns.len();
                metrics::WS_CONNECTIONS_ACTIVE.sub(count as f64);
                metrics::WS_DISCONNECTS_TOTAL.inc_by(count as f64);
                tracing::info!("Removed all {} connections for game {}", count, game_id);
            })
        };
        if removed.is_some() {
            self.refresh_spectator_gauge(game_id).await;
        }
    }

    /// Remove all connections for a specific player from a game without sending
    /// a PlayerDisconnected event (the player was kicked, not disconnected).
    /// This prevents the kicked player from receiving further game events.
    pub async fn remove_player_connections(&self, game_id: Uuid, player_id: Uuid) {
        let mut changed = false;
        {
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
                    changed = true;
                    tracing::info!(
                        "Removed {} connection(s) for kicked player {} from game {}",
                        removed,
                        player_id,
                        game_id
                    );
                }
            }
        }
        if changed {
            self.refresh_spectator_gauge(game_id).await;
        }
    }

    /// Broadcast a message to all connections of a specific game.
    pub async fn broadcast_to_game(&self, game_id: Uuid, message: &str, kind: MessageKind) {
        let targets = self.snapshot_senders(game_id).await;
        if targets.is_empty() {
            // Record the set of game IDs currently known to this manager so we
            // can tell whether the game was never registered or was already
            // removed when the broadcast arrived.
            let inner = self.inner.read().await;
            let known_games: Vec<String> =
                inner.connections.keys().map(|g| g.to_string()).collect();
            tracing::warn!(
                "[WS-DIAG] No connections for game {}, message not broadcast. \
                 known_games={:?} (count={}), message_len={}",
                game_id,
                known_games,
                known_games.len(),
                message.len()
            );
            return;
        }

        let msg = WsMessage {
            payload: Arc::from(message),
            kind,
        };
        let slow = Self::deliver(&msg, &targets);
        self.disconnect_slow_consumers(game_id, slow).await;
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
        self.broadcast_to_game(game_id, &error_msg, MessageKind::Control)
            .await;
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
        self.send_to_player(game_id, player_id, &error_msg, MessageKind::Control)
            .await;
    }

    /// Send a message to only the connections belonging to a specific player within a game.
    pub async fn send_to_player(
        &self,
        game_id: Uuid,
        player_id: Uuid,
        message: &str,
        kind: MessageKind,
    ) {
        let targets = {
            let inner = self.inner.read().await;
            inner
                .connections
                .get(&game_id)
                .map(|conns| {
                    conns
                        .iter()
                        .filter(|c| c.player_id == Some(player_id))
                        .map(|c| (c.id, c.sender.clone()))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        };
        if targets.is_empty() {
            tracing::debug!(
                "No connections for game {}, message not sent to player {}",
                game_id,
                player_id
            );
            return;
        }

        let msg = WsMessage {
            payload: Arc::from(message),
            kind,
        };
        let slow = Self::deliver(&msg, &targets);
        self.disconnect_slow_consumers(game_id, slow).await;
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
}
