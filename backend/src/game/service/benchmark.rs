use std::collections::HashSet;

use rand::RngExt;
use sea_orm::{ActiveValue, ColumnTrait, EntityTrait, QueryFilter, Set, TransactionTrait};
use uuid::Uuid;

use crate::database::models::{self, game, player, GameMode, GameStatus, PlayerType};
use crate::error::GameError;
use crate::game::distribution::distribute_cards;
use crate::game::service::types::{
    BenchmarkCleanupCounts, BenchmarkGameOutcome, BenchmarkPlayerOutcome,
};

use super::GameService;

impl GameService {
    #[tracing::instrument(level = "info", skip(self), fields(user_ids = ?user_ids, bet = bet))]
    pub async fn create_benchmark_multiplayer_game(
        &self,
        user_ids: Vec<Uuid>,
        bet: i32,
    ) -> Result<BenchmarkGameOutcome, GameError> {
        tracing::debug!(
            "create_benchmark_multiplayer_game: user_ids={:?}, bet={}",
            user_ids,
            bet
        );

        if user_ids.len() != 4 {
            let msg = format!(
                "Benchmark games require exactly 4 players, got {}",
                user_ids.len()
            );
            tracing::error!("{}", msg);
            return Err(GameError::internal(msg));
        }

        let unique: HashSet<Uuid> = user_ids.iter().copied().collect();
        if unique.len() != 4 {
            let msg = format!(
                "Benchmark game requires 4 unique user IDs, got {} unique from {:?}",
                unique.len(),
                user_ids
            );
            tracing::error!("{}", msg);
            return Err(GameError::internal(msg));
        }

        let txn = self.db.begin().await.map_err(|e| {
            tracing::error!("Failed to begin transaction: {}", e);
            GameError::Database(e)
        })?;

        let users = models::user::Entity::find()
            .filter(models::user::Column::Id.is_in(user_ids.clone()))
            .all(&txn)
            .await
            .map_err(|e| {
                tracing::error!("Failed to query users: {}", e);
                GameError::Database(e)
            })?;

        if users.len() != 4 {
            let found_ids: Vec<Uuid> = users.iter().map(|u| u.id).collect();
            tracing::error!(
                "Expected 4 users but found {} for benchmark game. Requested IDs: {:?}, Found IDs: {:?}",
                users.len(),
                user_ids,
                found_ids
            );
            return Err(GameError::internal(format!(
                "One or more users not found: requested {} users but found {}",
                user_ids.len(),
                users.len()
            )));
        }

        let user_pseudos: Vec<String> = user_ids
            .iter()
            .map(|uid| {
                users
                    .iter()
                    .find(|u| u.id == *uid)
                    .map(|u| u.pseudo.clone())
                    .unwrap_or_default()
            })
            .collect();

        let creator_id = user_ids[0];

        let game_id = Uuid::new_v4();
        let now = chrono::Utc::now();
        let initial_rank = rand::rng().random_range(0..4) as i32;

        let game_active = game::ActiveModel {
            id: Set(game_id),
            status: Set(GameStatus::Active),
            bet: Set(bet),
            created_at: Set(now),
            updated_at: Set(now),
            finished_at: ActiveValue::NotSet,
            rank: Set(Some(initial_rank)),
            roll: Set(1),
            auto: Set(false),
            winner_id: ActiveValue::NotSet,
            player_positions: Set(serde_json::json!({})),
            current_winning_card: ActiveValue::NotSet,
            current_winning_player_position: ActiveValue::NotSet,
            creator_id: Set(Some(creator_id)),
            game_mode: Set(GameMode::Multiplayer),
            max_players: Set(4),
            invite_expires_at: ActiveValue::NotSet,
            stall_warning_sent_at: ActiveValue::NotSet,
            game_run_id: ActiveValue::NotSet,
            step_by_step: Set(false),
            kicked_players: Set(serde_json::json!([])),
        };
        game::Entity::insert(game_active)
            .exec(&txn)
            .await
            .map_err(GameError::Database)?;

        let mut player_ids = Vec::with_capacity(4);
        for (i, (&uid, pseudo)) in user_ids.iter().zip(user_pseudos.iter()).enumerate() {
            let position = i as i32;
            let player_id = Uuid::new_v4();
            let player_active = player::ActiveModel {
                id: Set(player_id),
                game_id: Set(game_id),
                player_type: Set(PlayerType::Human),
                name: Set(pseudo.clone()),
                position: Set(position),
                credits: Set(10),
                created_at: Set(now),
                user_id: Set(Some(uid)),
                kicked: Set(false),
                kicked_at: ActiveValue::NotSet,
            };
            player::Entity::insert(player_active)
                .exec(&txn)
                .await
                .map_err(|e| {
                    tracing::error!("Failed to create player {}: {}", pseudo, e);
                    GameError::Database(e)
                })?;
            player_ids.push(player_id);
        }

        let card_assignments = distribute_cards(&player_ids);

        let card_active_models: Vec<models::game_card::ActiveModel> = card_assignments
            .iter()
            .map(|&(player_id, card_index)| models::game_card::ActiveModel {
                id: Set(Uuid::new_v4()),
                game_id: Set(game_id),
                player_id: Set(Some(player_id)),
                card_index: Set(card_index),
                played: Set(false),
                played_at: ActiveValue::NotSet,
                round: Set(None),
                created_at: Set(now),
            })
            .collect();
        if !card_active_models.is_empty() {
            models::game_card::Entity::insert_many(card_active_models)
                .exec(&txn)
                .await
                .map_err(GameError::Database)?;
        }

        txn.commit().await.map_err(GameError::Database)?;

        let players_outcome: Vec<BenchmarkPlayerOutcome> = player_ids
            .iter()
            .enumerate()
            .map(|(i, &player_id)| {
                let uid = user_ids[i];
                let cards: Vec<i32> = card_assignments
                    .iter()
                    .filter(|(pid, _)| *pid == player_id)
                    .map(|(_, card)| *card)
                    .collect();
                BenchmarkPlayerOutcome {
                    player_id,
                    user_id: uid,
                    name: user_pseudos[i].clone(),
                    position: i as i32,
                    cards,
                }
            })
            .collect();

        Ok(BenchmarkGameOutcome {
            game_id,
            players: players_outcome,
            current_turn: initial_rank,
            bet,
        })
    }

    #[tracing::instrument(level = "info", skip(self))]
    pub async fn cleanup_benchmark_data(&self) -> Result<BenchmarkCleanupCounts, GameError> {
        let benchmark_users: Vec<Uuid> = models::user::Entity::find()
            .filter(models::user::Column::Email.contains("@benchmark.local"))
            .all(&self.db)
            .await
            .map_err(GameError::Database)?
            .into_iter()
            .map(|u| u.id)
            .collect();

        let benchmark_games: Vec<Uuid> = game::Entity::find()
            .filter(game::Column::GameMode.eq(GameMode::Multiplayer))
            .filter(game::Column::CreatorId.is_in(benchmark_users.clone()))
            .all(&self.db)
            .await
            .map_err(GameError::Database)?
            .into_iter()
            .map(|g| g.id)
            .collect();

        let player_profiles_deleted = models::player_profile::Entity::delete_many()
            .filter(models::player_profile::Column::UserId.is_in(benchmark_users.clone()))
            .exec(&self.db)
            .await
            .map_err(GameError::Database)?
            .rows_affected;

        let game_invites_deleted = models::game_invite::Entity::delete_many()
            .filter(models::game_invite::Column::InvitedUserId.is_in(benchmark_users.clone()))
            .exec(&self.db)
            .await
            .map_err(GameError::Database)?
            .rows_affected;

        let game_cards_deleted = if !benchmark_games.is_empty() {
            models::game_card::Entity::delete_many()
                .filter(models::game_card::Column::GameId.is_in(benchmark_games.clone()))
                .exec(&self.db)
                .await
                .map_err(GameError::Database)?
                .rows_affected
        } else {
            0
        };

        let players_deleted = if !benchmark_games.is_empty() {
            player::Entity::delete_many()
                .filter(player::Column::GameId.is_in(benchmark_games.clone()))
                .exec(&self.db)
                .await
                .map_err(GameError::Database)?
                .rows_affected
        } else {
            0
        };

        let games_deleted = if !benchmark_games.is_empty() {
            game::Entity::delete_many()
                .filter(game::Column::Id.is_in(benchmark_games.clone()))
                .exec(&self.db)
                .await
                .map_err(GameError::Database)?
                .rows_affected
        } else {
            0
        };

        let users_deleted = if !benchmark_users.is_empty() {
            models::user::Entity::delete_many()
                .filter(models::user::Column::Id.is_in(benchmark_users.clone()))
                .exec(&self.db)
                .await
                .map_err(GameError::Database)?
                .rows_affected
        } else {
            0
        };

        tracing::info!(
            users = users_deleted,
            games = games_deleted,
            cards = game_cards_deleted,
            players = players_deleted,
            profiles = player_profiles_deleted,
            invites = game_invites_deleted,
            "Benchmark data cleanup complete"
        );

        Ok(BenchmarkCleanupCounts {
            users_deleted,
            games_deleted,
            game_cards_deleted,
            players_deleted,
            player_profiles_deleted,
            game_invites_deleted,
        })
    }
}
