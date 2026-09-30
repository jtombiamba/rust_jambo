use std::time::Instant;
use uuid::Uuid;

use super::WebSocketManager;
use crate::observability::metrics;
use crate::observability::CorrelationId;
use crate::websocket::connection::{ConnectionId, TrackedConnection, WsSender};

impl WebSocketManager {
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
                disconnected: false,
                last_pong: Instant::now(),
                spectator: false,
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
            for connection in connections {
                match connection.sender.send(message.to_string()) {
                    Ok(()) => metrics::WS_MESSAGES_SENT_TOTAL.inc(),
                    Err(e) => {
                        metrics::WS_SEND_FAILED_TOTAL.inc();
                        tracing::warn!(
                            "Failed to send message to room connection {}: {}",
                            connection.id.uuid(),
                            e
                        );
                    }
                }
            }
        }
    }
}
