use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder};
use serde_json;
use tracing::{error, warn};
use uuid::Uuid;

use crate::database::models::{game, game_card, player, PlayerType};
use crate::error::GameError;
use crate::game::service::types::{
    CachedCard, CachedGameState, CachedPlayer, GAME_STATE_CACHE_TTL_SECS,
};
use crate::game::turn_order::next_player;
use crate::observability::metrics::GAME_STATE_CACHE_WRITE_ERRORS_TOTAL;

use super::GameService;

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
            if let Err(e) = redis.del(&profile_key).await {
                error!("Failed to invalidate profile cache for {}: {}", user_id, e);
                GAME_STATE_CACHE_WRITE_ERRORS_TOTAL.inc();
            }

            let games_pattern = format!("dashboard:games:{user_id}:*");
            if let Err(e) = redis.del_pattern(&games_pattern).await {
                error!("Failed to invalidate games cache for {}: {}", user_id, e);
                GAME_STATE_CACHE_WRITE_ERRORS_TOTAL.inc();
            }
        }
    }

    pub(crate) async fn build_cached_game_state(
        &self,
        game_id: Uuid,
    ) -> Result<CachedGameState, GameError> {
        let game = game::Entity::find_by_id(game_id)
            .one(&self.db)
            .await?
            .ok_or(GameError::GameNotFound)?;

        let players = player::Entity::find()
            .filter(player::Column::GameId.eq(game_id))
            .order_by_asc(player::Column::Position)
            .all(&self.db)
            .await?;

        let cards = game_card::Entity::find()
            .filter(game_card::Column::GameId.eq(game_id))
            .all(&self.db)
            .await?;

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

        let round_complete = self
            .is_round_complete_txn_inner(&self.db, game_id, game.roll)
            .await?;

        let next_player_id = self.order_next_player(game_id, &game, &players).await?;

        Ok(CachedGameState {
            status: format!("{:?}", game.status),
            roll: game.roll,
            rank: game.rank,
            bet: game.bet,
            current_winning_card: game.current_winning_card,
            current_winning_player_position: game.current_winning_player_position,
            players: cached_players,
            cards: cached_cards,
            round_completed: round_complete,
            next_player_id,
        })
    }

    pub(crate) async fn is_round_complete_txn_inner(
        &self,
        db: &DatabaseConnection,
        game_id: Uuid,
        round: i32,
    ) -> Result<bool, GameError> {
        let players = player::Entity::find()
            .filter(player::Column::GameId.eq(game_id))
            .all(db)
            .await?;

        // Skip kicked players — they should not block round completion
        for player_model in players {
            if player_model.kicked {
                continue;
            }
            let cards = game_card::Entity::find()
                .filter(game_card::Column::PlayerId.eq(player_model.id))
                .all(db)
                .await?;
            let played_in_round = cards.iter().any(|c| c.played && c.round == Some(round));
            if !played_in_round {
                return Ok(false);
            }
        }
        Ok(true)
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
