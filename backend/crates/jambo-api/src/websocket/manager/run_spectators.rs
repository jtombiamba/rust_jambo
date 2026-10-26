use std::time::Instant;
use uuid::Uuid;

use super::WebSocketManager;
use crate::websocket::connection::{ConnectionId, TrackedConnection, WsSender};
use jambo_core::observability::metrics;
use jambo_core::observability::CorrelationId;

/// A read-only spectator connection that follows a whole run (not a single
/// game). It is registered under the run's current `game_id` so it receives the
/// same public game events as a regular spectator, and is re-pointed to each new
/// `game_id` when the run advances.
pub(super) struct RunSpectator {
    pub(super) connection_id: ConnectionId,
    pub(super) sender: WsSender,
    pub(super) correlation_id: CorrelationId,
    pub(super) current_game_id: Option<Uuid>,
}

impl WebSocketManager {
    /// Track a connection as a run spectator. When `game_id` is `Some`, the
    /// connection is assumed to already be registered in that game's connection
    /// map (via `add_connection` + `mark_spectator_for_connection`). When
    /// `None`, the run has no current game yet and the connection is only kept
    /// in the run registry until the first `GameStarted`.
    #[allow(clippy::too_many_arguments)]
    pub async fn register_run_spectator(
        &self,
        run_id: Uuid,
        connection_id: ConnectionId,
        sender: WsSender,
        correlation_id: CorrelationId,
        game_id: Option<Uuid>,
    ) {
        let mut shard = self.shard(run_id).write().await;
        shard
            .run_spectators
            .entry(run_id)
            .or_default()
            .push(RunSpectator {
                connection_id,
                sender,
                correlation_id,
                current_game_id: game_id,
            });
        tracing::debug!(
            "Registered run spectator {} for run {} (game {:?})",
            connection_id.uuid(),
            run_id,
            game_id
        );
    }

    /// Remove a run spectator from both the run registry and the game
    /// connection map (using its currently-tracked game id, if any).
    pub async fn remove_run_spectator(&self, run_id: Uuid, connection_id: ConnectionId) {
        let current_game_id = {
            let mut shard = self.shard(run_id).write().await;
            let mut game_id = None;
            if let Some(spectators) = shard.run_spectators.get_mut(&run_id) {
                spectators.retain(|s| {
                    if s.connection_id == connection_id {
                        game_id = s.current_game_id;
                        false
                    } else {
                        true
                    }
                });
                if spectators.is_empty() {
                    shard.run_spectators.remove(&run_id);
                }
            }
            game_id
        };

        if let Some(game_id) = current_game_id {
            self.remove_connection(game_id, connection_id).await;
        }
        tracing::debug!(
            "Removed run spectator {} for run {}",
            connection_id.uuid(),
            run_id
        );
    }

    /// Re-point every run spectator of `run_id` to `new_game_id`, then send them
    /// a fresh public snapshot of the new game.
    pub async fn re_point_run_spectators(&self, run_id: Uuid, new_game_id: Uuid) {
        let entries: Vec<(ConnectionId, Option<Uuid>, WsSender, CorrelationId)> = {
            let shard = self.shard(run_id).read().await;
            shard
                .run_spectators
                .get(&run_id)
                .map(|spectators| {
                    spectators
                        .iter()
                        .filter(|s| s.current_game_id != Some(new_game_id))
                        .map(|s| {
                            (
                                s.connection_id,
                                s.current_game_id,
                                s.sender.clone(),
                                s.correlation_id,
                            )
                        })
                        .collect()
                })
                .unwrap_or_default()
        };

        let mut moved = false;
        for (connection_id, old_game_id, sender, correlation_id) in entries {
            match old_game_id {
                Some(old) => {
                    self.move_connection(old, new_game_id, connection_id).await;
                }
                None => {
                    self.insert_spectator_connection(
                        new_game_id,
                        connection_id,
                        sender,
                        correlation_id,
                    )
                    .await;
                }
            }
            moved = true;
        }

        if moved {
            let mut shard = self.shard(run_id).write().await;
            if let Some(spectators) = shard.run_spectators.get_mut(&run_id) {
                for spectator in spectators.iter_mut() {
                    spectator.current_game_id = Some(new_game_id);
                }
            }

            tracing::debug!(
                "Re-pointed run spectators for run {} to game {}",
                run_id,
                new_game_id
            );
            if let Some(db) = self.db.clone() {
                crate::websocket::game_state::send_spectator_snapshot(self, &db, new_game_id).await;
            }
        }
    }

    /// Update the heartbeat timestamps for a run spectator in whatever game it
    /// is currently registered under.
    pub async fn touch_run_spectator(&self, run_id: Uuid, connection_id: ConnectionId) {
        let game_id = {
            let shard = self.shard(run_id).read().await;
            shard.run_spectators.get(&run_id).and_then(|spectators| {
                spectators
                    .iter()
                    .find(|s| s.connection_id == connection_id)
                    .and_then(|s| s.current_game_id)
            })
        };
        if let Some(game_id) = game_id {
            self.update_activity(game_id, connection_id).await;
            self.update_pong(game_id, connection_id).await;
        }
    }

    /// Insert a spectator `TrackedConnection` into a game's connection map.
    async fn insert_spectator_connection(
        &self,
        game_id: Uuid,
        connection_id: ConnectionId,
        sender: WsSender,
        correlation_id: CorrelationId,
    ) {
        let mut shard = self.shard(game_id).write().await;
        shard
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
                spectator: true,
                user_id: None,
            });
        metrics::WS_CONNECTIONS_ACTIVE.inc();
    }

    /// Move a single tracked connection between two game connection maps,
    /// preserving its identity, sender, and spectator flag.
    async fn move_connection(
        &self,
        old_game_id: Uuid,
        new_game_id: Uuid,
        connection_id: ConnectionId,
    ) {
        let connection = {
            let mut shard = self.shard(old_game_id).write().await;
            let mut removed = None;
            if let Some(connections) = shard.connections.get_mut(&old_game_id) {
                if let Some(idx) = connections.iter().position(|c| c.id == connection_id) {
                    removed = Some(connections.remove(idx));
                }
                if connections.is_empty() {
                    shard.connections.remove(&old_game_id);
                }
            }
            removed
        };

        if let Some(connection) = connection {
            let mut shard = self.shard(new_game_id).write().await;
            shard
                .connections
                .entry(new_game_id)
                .or_default()
                .push(connection);
        }

        self.refresh_spectator_gauge(old_game_id).await;
        self.refresh_spectator_gauge(new_game_id).await;
    }
}
