impl WebSocketManager {
    /// Mark the most recently added connection for a game as a spectator, then
    /// refresh the per-game spectator gauge.
    pub async fn mark_spectator_for_latest_connection(manager: &WebSocketManager, game_id: Uuid) {
        let conn_id = {
            let inner = manager.inner.read().await;
            inner
                .connections
                .get(&game_id)
                .and_then(|conns| conns.last())
                .map(|c| c.id)
        };
        if let Some(cid) = conn_id {
            {
                let mut inner = manager.inner.write().await;
                if let Some(connections) = inner.connections.get_mut(&game_id) {
                    for conn in connections.iter_mut() {
                        if conn.id == cid {
                            conn.spectator = true;
                            break;
                        }
                    }
                }
            }
            manager.refresh_spectator_gauge(game_id).await;
        }
    }

    /// Send a message to all spectator connections of a game (player_id == None
    /// and marked as spectator).
    pub async fn send_to_spectators(&self, game_id: Uuid, message: &str) {
        let inner = self.inner.read().await;
        if let Some(connections) = inner.connections.get(&game_id) {
            for connection in connections {
                if connection.spectator {
                    match connection.sender.send(message.to_string()) {
                        Ok(()) => metrics::WS_MESSAGES_SENT_TOTAL.inc(),
                        Err(e) => {
                            metrics::WS_SEND_FAILED_TOTAL.inc();
                            tracing::warn!(
                                "Failed to send message to spectator connection {}: {}",
                                connection.id.uuid(),
                                e
                            );
                        }
                    }
                }
            }
        }
    }

    /// Recompute and publish the per-game spectator gauge for a single game,
    /// removing the series when the game has no spectators (or no connections at
    /// all) to bound label cardinality.
    ///
    /// Takes the read lock internally, so callers must not hold the write lock.
    async fn refresh_spectator_gauge(&self, game_id: Uuid) {
        let count = {
            let inner = self.inner.read().await;
            inner
                .connections
                .get(&game_id)
                .map(|conns| conns.iter().filter(|c| c.spectator).count())
                .unwrap_or(0)
        };
        if count == 0 {
            let label = game_id.to_string();
            if let Err(e) = metrics::WS_SPECTATORS_PER_GAME.remove_label_values(&[&label]) {
                tracing::warn!(
                    "Failed to remove spectator gauge series for game {}: {}",
                    game_id,
                    e
                );
            }
        } else {
            metrics::WS_SPECTATORS_PER_GAME
                .with_label_values(&[&game_id.to_string()])
                .set(count as f64);
        }
    }
}
