use sea_orm::DatabaseConnection;
use serde_json;
use tracing::{error, warn};
use uuid::Uuid;

use crate::game::service::types::{
    CachedCard, CachedGameState, CachedPlayer, GAME_STATE_CACHE_TTL_SECS,
};
use crate::game::turn_order::next_player;
use jambo_core::error::GameError;
use jambo_core::observability::metrics::GAME_STATE_CACHE_WRITE_ERRORS_TOTAL;
use jambo_db::models::{game, game_card, player, PlayerType};
use jambo_db::repositories::{GameCardRepository, GameRepository, PlayerRepository};

use super::GameService;

fn assemble_cached_game_state(
    game: &game::Model,
    players: &[player::Model],
    cards: &[game_card::Model],
    round_completed: bool,
    next_player_id: Uuid,
) -> CachedGameState {
    let cached_players: Vec<CachedPlayer> = players
        .iter()
        .map(|p| CachedPlayer {
            id: p.id,
            name: p.name.clone(),
            position: p.position,
            player_type: match p.player_type {
                PlayerType::Human => "human".to_string(),
                PlayerType::Bot => "bot".to_string(),
            },
            credits: p.credits,
            user_id: p.user_id,
        })
        .collect();

    let cached_cards: Vec<CachedCard> = cards
        .iter()
        .map(|c| CachedCard {
            player_id: c.player_id,
            card_index: c.card_index,
            played: c.played,
            round: c.round,
        })
        .collect();

    CachedGameState {
        status: format!("{:?}", game.status),
        roll: game.roll,
        rank: game.rank,
        bet: game.bet,
        current_winning_card: game.current_winning_card,
        current_winning_player_position: game.current_winning_player_position,
        players: cached_players,
        cards: cached_cards,
        round_completed,
        next_player_id,
    }
}

/// A round is complete when every non-kicked player has played a card in it.
/// Uses two aggregate queries (via the repositories) instead of a per-player N+1.
async fn is_round_complete(
    db: &DatabaseConnection,
    game_id: Uuid,
    round: i32,
) -> Result<bool, GameError> {
    let active_count = PlayerRepository::new(db.clone())
        .count_active(game_id)
        .await?;
    let played_in_round = GameCardRepository::new(db.clone())
        .count_played_in_round(game_id, round)
        .await?;

    Ok(played_in_round >= active_count)
}

fn next_player_id_from(game: &game::Model, players: &[player::Model]) -> Option<Uuid> {
    let current_rank = game.rank.unwrap_or(0) as usize;
    let active_count = players.iter().filter(|p| !p.kicked).count();
    let next_rank = next_player(current_rank, active_count);
    players
        .iter()
        .filter(|p| !p.kicked)
        .nth(next_rank)
        .map(|p| p.id)
}

impl GameService {
    pub async fn cache_game_state(&self, game_id: Uuid) {
        let mut redis = match self.redis_client.clone() {
            Some(r) => r,
            None => return,
        };
        if let Ok(state) = self.build_cached_game_state(game_id).await {
            if let Ok(data) = serde_json::to_string(&state) {
                match redis
                    .set_ex(
                        &format!("game:state:{game_id}"),
                        &data,
                        GAME_STATE_CACHE_TTL_SECS,
                    )
                    .await
                {
                    Ok(()) => {}
                    Err(e) => {
                        warn!("Failed to cache game state for {}: {}", game_id, e);
                        GAME_STATE_CACHE_WRITE_ERRORS_TOTAL.inc();
                    }
                }
            } else {
                warn!("Failed to serialize game state for cache: {}", game_id);
            }
        } else {
            warn!("Failed to build cached game state for {}", game_id);
        }
    }

    /// Offload the read-through game-state cache write to a background task,
    /// reusing the player list already loaded by the card-play transaction so
    /// the write stays off the request critical path.
    pub(crate) fn spawn_cache_game_state(&self, game_id: Uuid, players: Vec<player::Model>) {
        let db = self.db.clone();
        let Some(mut redis) = self.redis_client.clone() else {
            return;
        };
        tokio::spawn(async move {
            let game = match GameRepository::new(db.clone()).find_by_id(game_id).await {
                Ok(Some(g)) => g,
                _ => return,
            };
            let cards = match GameCardRepository::new(db.clone())
                .list_by_game(game_id)
                .await
            {
                Ok(c) => c,
                Err(_) => return,
            };
            let round_completed = match is_round_complete(&db, game_id, game.roll).await {
                Ok(v) => v,
                Err(_) => return,
            };
            let Some(next_player_id) = next_player_id_from(&game, &players) else {
                return;
            };

            let state = assemble_cached_game_state(
                &game,
                &players,
                &cards,
                round_completed,
                next_player_id,
            );
            let Ok(data) = serde_json::to_string(&state) else {
                warn!("Failed to serialize game state for cache: {}", game_id);
                return;
            };
            if let Err(e) = redis
                .set_ex(
                    &format!("game:state:{game_id}"),
                    &data,
                    GAME_STATE_CACHE_TTL_SECS,
                )
                .await
            {
                warn!("Failed to cache game state for {}: {}", game_id, e);
                GAME_STATE_CACHE_WRITE_ERRORS_TOTAL.inc();
            }
        });
    }

    pub async fn invalidate_game_state_cache(&self, game_id: Uuid) {
        let mut redis = match self.redis_client.clone() {
            Some(r) => r,
            None => return,
        };
        match redis.del(&format!("game:state:{game_id}")).await {
            Ok(()) => {}
            Err(e) => {
                warn!(
                    "Failed to invalidate game state cache for {}: {}",
                    game_id, e
                );
                GAME_STATE_CACHE_WRITE_ERRORS_TOTAL.inc();
            }
        }
    }

    pub(crate) async fn invalidate_dashboard_caches(&self, user_ids: &[Uuid]) {
        let mut redis = match self.redis_client.clone() {
            Some(r) => r,
            None => return,
        };

        for &user_id in user_ids {
            let profile_key = format!("dashboard:profile:{user_id}");
            match redis.del(&profile_key).await {
                Ok(()) => {
                    tracing::info!(
                        "Successfully invalidated profile cache for user_id: {}",
                        user_id
                    );
                }
                Err(e) => {
                    error!("Failed to invalidate profile cache for {}: {}", user_id, e);
                    GAME_STATE_CACHE_WRITE_ERRORS_TOTAL.inc();
                }
            };

            let games_pattern = format!("dashboard:games:{user_id}:*");
            match redis.del_pattern(&games_pattern).await {
                Ok(_deleted) => {
                    tracing::info!(
                        "Successfully invalidated games cache for user_id: {}",
                        user_id
                    );
                }
                Err(e) => {
                    error!("Failed to invalidate games cache for {}: {}", user_id, e);
                    GAME_STATE_CACHE_WRITE_ERRORS_TOTAL.inc();
                }
            };
        }
    }

    pub(crate) async fn build_cached_game_state(
        &self,
        game_id: Uuid,
    ) -> Result<CachedGameState, GameError> {
        let game = GameRepository::new(self.db.clone())
            .find_by_id(game_id)
            .await?
            .ok_or(GameError::GameNotFound)?;

        let players = PlayerRepository::new(self.db.clone())
            .list_by_game(game_id)
            .await?;

        let cards = GameCardRepository::new(self.db.clone())
            .list_by_game(game_id)
            .await?;

        let round_complete = is_round_complete(&self.db, game_id, game.roll).await?;

        let next_player_id = self.order_next_player(game_id, &game, &players).await?;

        Ok(assemble_cached_game_state(
            &game,
            &players,
            &cards,
            round_complete,
            next_player_id,
        ))
    }

    pub(crate) async fn order_next_player(
        &self,
        _game_id: Uuid,
        game_model: &game::Model,
        players: &[player::Model],
    ) -> Result<Uuid, GameError> {
        let current_rank = game_model.rank.unwrap_or(0) as usize;
        // Use active player count (exclude kicked) for correct modular arithmetic
        let active_count = players.iter().filter(|p| !p.kicked).count();
        let next_rank = next_player(current_rank, active_count);
        players
            .iter()
            .filter(|p| !p.kicked)
            .nth(next_rank)
            .map(|p| p.id)
            .ok_or_else(|| GameError::internal("No player at computed rank"))
    }
}
