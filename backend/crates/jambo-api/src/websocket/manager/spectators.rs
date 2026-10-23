use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

use super::WebSocketManager;
use crate::websocket::connection::{ConnectionId, MessageKind, WsMessage};
use jambo_core::observability::metrics;

impl WebSocketManager {
    /// Mark a specific connection for a game as a read-only spectator.
    ///
    /// Setting the spectator flag also clears any player identity on the same
    /// connection: a spectator never has a hand, so a connection that is marked
    /// spectator must not keep receiving personalized (per-player) events.
    ///
    /// The per-game spectator gauge is refreshed lazily by the periodic resync
    /// task (and immediately on disconnect) rather than on every mark, so a
    /// burst of spectator joins does not serialize on a gauge write per socket.
    pub async fn mark_spectator_for_connection(&self, game_id: Uuid, connection_id: ConnectionId) {
        let mut shard = self.shard(game_id).write().await;
        if let Some(connections) = shard.connections.get_mut(&game_id) {
            for conn in connections.iter_mut() {
                if conn.id == connection_id {
                    conn.spectator = true;
                    conn.player_id = None;
                    conn.player_position = None;
                    break;
                }
            }
        }
    }

    /// Send a message to all spectator connections of a game (player_id == None
    /// and marked as spectator).
    pub async fn send_to_spectators(&self, game_id: Uuid, message: &str, kind: MessageKind) {
        let targets = {
            let shard = self.shard(game_id).read().await;
            shard
                .connections
                .get(&game_id)
                .map(|conns| {
                    conns
                        .iter()
                        .filter(|c| c.spectator)
                        .map(|c| (c.id, c.sender.clone()))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        };
        if targets.is_empty() {
            return;
        }

        let msg = WsMessage {
            payload: Arc::from(message),
            kind,
        };
        let slow = Self::deliver(&msg, &targets);
        self.disconnect_slow_consumers(game_id, slow).await;
    }

    /// Recompute and publish the per-game spectator gauge for a single game,
    /// removing the series when the game has no spectators (or no connections at
    /// all) to bound label cardinality.
    ///
    /// Takes the read lock internally, so callers must not hold the write lock.
    pub(super) async fn refresh_spectator_gauge(&self, game_id: Uuid) {
        let count = {
            let shard = self.shard(game_id).read().await;
            shard
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

    /// Return the ids of every game that currently has at least one spectator
    /// connection. Used by the periodic resync task to bound the number of games
    /// it refreshes.
    pub(crate) async fn games_with_spectators(&self) -> Vec<Uuid> {
        let mut games = Vec::new();
        for shard in self.shards.iter() {
            let shard = shard.read().await;
            games.extend(
                shard
                    .connections
                    .iter()
                    .filter(|(_, conns)| conns.iter().any(|c| c.spectator))
                    .map(|(game_id, _)| *game_id),
            );
        }
        games
    }

    /// Start a background task that periodically sends a public-only spectator
    /// snapshot to every game with active spectators.
    ///
    /// Redis pub/sub is fire-and-forget: events published during a brief
    /// disconnect are lost. Because a snapshot is the authoritative full state,
    /// a low-rate periodic resync self-heals any gap — the next snapshot
    /// supersedes missed intermediate events.
    pub async fn start_spectator_resync_task(&self, interval: Duration) {
        let manager = self.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                ticker.tick().await;
                let games = manager.games_with_spectators().await;
                if games.is_empty() {
                    continue;
                }
                let Some(db) = manager.db.clone() else {
                    continue;
                };
                for game_id in games {
                    // Refresh the gauge lazily here; disconnect paths refresh it
                    // immediately so the series is removed as soon as the last
                    // spectator leaves.
                    manager.refresh_spectator_gauge(game_id).await;
                    crate::websocket::game_state::send_spectator_snapshot(&manager, &db, game_id)
                        .await;
                }
            }
        });
        tracing::info!(
            "Started spectator snapshot resync task with interval {:?}",
            interval
        );
    }
}
