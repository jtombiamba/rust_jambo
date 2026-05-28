impl WebSocketManager {
    /// Clean up stale connections that haven't had activity for more than `max_idle_duration`.
    /// Returns the number of connections removed.
    pub async fn cleanup_stale_connections(&self, max_idle_duration: Duration) -> usize {
        let mut inner = self.inner.write().await;
        let mut total_removed = 0;
        let now = Instant::now();

        // Iterate over game connections
        inner.connections.retain(|game_id, connections| {
            let before = connections.len();
            connections.retain(|conn| {
                let idle_duration = now.duration_since(conn.last_activity);
                idle_duration <= max_idle_duration
            });
            let removed = before - connections.len();
            total_removed += removed;

            if removed > 0 {
                tracing::info!(
                    "Cleaned up {} stale connections for game {}",
                    removed,
                    game_id
                );
            }

            !connections.is_empty()
        });

        // Iterate over room connections
        inner.room_connections.retain(|room_id, connections| {
            let before = connections.len();
            connections.retain(|conn| {
                let idle_duration = now.duration_since(conn.last_activity);
                idle_duration <= max_idle_duration
            });
            let removed = before - connections.len();
            total_removed += removed;

            if removed > 0 {
                tracing::info!(
                    "Cleaned up {} stale connections for room {}",
                    removed,
                    room_id
                );
            }

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
