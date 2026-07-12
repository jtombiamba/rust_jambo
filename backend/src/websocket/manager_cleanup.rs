impl WebSocketManager {
    /// Clean up stale connections that haven't had activity for more than `max_idle_duration`
    /// or haven't responded to pings within `heartbeat_timeout`.
    /// Returns the number of connections removed.
    pub async fn cleanup_stale_connections(
        &self,
        max_idle_duration: Duration,
        heartbeat_timeout: Duration,
    ) -> usize {
        let mut inner = self.inner.write().await;
        let mut total_removed = 0;
        let now = Instant::now();
        let mut expired_players: Vec<(Uuid, Uuid, i32)> = Vec::new();

        inner.connections.retain(|game_id, connections| {
            let before = connections.len();
            let expired: Vec<(Uuid, Uuid, i32)> = connections
                .iter()
                .filter(|conn| {
                    let idle = now.duration_since(conn.last_activity);
                    let pong_age = now.duration_since(conn.last_pong);
                    idle > max_idle_duration || pong_age > heartbeat_timeout
                })
                .filter_map(|conn| {
                    conn.player_id
                        .zip(conn.player_position)
                        .map(|(pid, pos)| (*game_id, pid, pos))
                })
                .collect();
            expired_players.extend(expired);
            connections.retain(|conn| {
                let idle = now.duration_since(conn.last_activity);
                let pong_age = now.duration_since(conn.last_pong);
                idle <= max_idle_duration && pong_age <= heartbeat_timeout
            });
            let removed = before - connections.len();
            total_removed += removed;

            if removed > 0 {
                tracing::info!(
                    "Cleaned up {} stale/heartbeat connections for game {}",
                    removed,
                    game_id
                );
            }

            !connections.is_empty()
        });

        inner.room_connections.retain(|room_id, connections| {
            let before = connections.len();
            connections.retain(|conn| {
                let idle = now.duration_since(conn.last_activity);
                let pong_age = now.duration_since(conn.last_pong);
                idle <= max_idle_duration && pong_age <= heartbeat_timeout
            });
            let removed = before - connections.len();
            total_removed += removed;

            if removed > 0 {
                tracing::info!(
                    "Cleaned up {} stale/heartbeat connections for room {}",
                    removed,
                    room_id
                );
            }

            !connections.is_empty()
        });

        if total_removed > 0 {
            tracing::info!("Total stale connections cleaned up: {}", total_removed);
            metrics::WS_HEARTBEAT_TIMEOUTS_TOTAL.inc_by(total_removed as f64);
        }
        metrics::WS_CONNECTIONS_ACTIVE.sub(total_removed as f64);

        // Publish PlayerDisconnected events for expired game connections
        for (game_id, player_id, position) in expired_players {
            drop(inner);
            let event = crate::messaging::events::GameEvent::PlayerDisconnected {
                game_id,
                player_id,
                player_position: position,
                disconnected_at: Some(chrono::Utc::now().to_rfc3339()),
            };
            self.broadcast_to_game(game_id, &event.to_json()).await;
            inner = self.inner.write().await;
        }

        total_removed
    }

    /// Start a background task that periodically cleans up stale connections.
    pub async fn start_connection_cleanup_task(
        &self,
        cleanup_interval: Duration,
        max_idle_duration: Duration,
        heartbeat_timeout: Duration,
    ) {
        let manager = self.clone();
        tokio::spawn(async move {
            let mut interval = time::interval(cleanup_interval);
            loop {
                interval.tick().await;
                let removed = manager
                    .cleanup_stale_connections(max_idle_duration, heartbeat_timeout)
                    .await;
                if removed > 0 {
                    tracing::debug!("Cleanup task removed {} stale connections", removed);
                }
            }
        });
        tracing::info!(
            "Started connection cleanup task with interval {:?}, max idle {:?}, heartbeat timeout {:?}",
            cleanup_interval,
            max_idle_duration,
            heartbeat_timeout
        );
    }
}
