impl WebSocketManager {
    /// Add a new WebSocket connection for a specific user.
    pub async fn add_user_connection(
        &self,
        user_id: Uuid,
        sender: WsSender,
        correlation_id: CorrelationId,
    ) -> ConnectionId {
        let mut inner = self.inner.write().await;
        let connection_id = ConnectionId::new();

        inner
            .user_connections
            .entry(user_id)
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
            "Added user connection {} (correlation_id={}) for user {}",
            connection_id.uuid(),
            correlation_id,
            user_id
        );
        connection_id
    }

    /// Remove a WebSocket connection for a user.
    pub async fn remove_user_connection(&self, user_id: Uuid, connection_id: ConnectionId) {
        let mut inner = self.inner.write().await;
        if let Some(connections) = inner.user_connections.get_mut(&user_id) {
            connections.retain(|c| c.id != connection_id);
            if connections.is_empty() {
                inner.user_connections.remove(&user_id);
            }
            metrics::WS_CONNECTIONS_ACTIVE.dec();
            metrics::WS_DISCONNECTS_TOTAL.inc();
            tracing::info!(
                "Removed user connection {} for user {}",
                connection_id.uuid(),
                user_id
            );
        }
    }

    /// Update the last pong time for a user connection to keep it alive.
    pub async fn update_user_pong(&self, user_id: Uuid, connection_id: ConnectionId) {
        let mut inner = self.inner.write().await;
        if let Some(connections) = inner.user_connections.get_mut(&user_id) {
            for connection in connections.iter_mut() {
                if connection.id == connection_id {
                    connection.last_pong = Instant::now();
                    break;
                }
            }
        }
    }

    /// Broadcast a message to all connections of a specific user.
    pub async fn broadcast_to_user(&self, user_id: Uuid, message: &str) {
        let inner = self.inner.read().await;
        if let Some(connections) = inner.user_connections.get(&user_id) {
            let count = connections.len();
            metrics::WS_MESSAGES_SENT_TOTAL.inc_by(count as f64);
            for connection in connections {
                if let Err(e) = connection.sender.send(message.to_string()) {
                    tracing::debug!(
                        "Failed to send message to user connection {}: {}",
                        connection.id.uuid(),
                        e
                    );
                }
            }
        } else {
            tracing::debug!("No connections for user {}, message not broadcast", user_id);
        }
    }
}
