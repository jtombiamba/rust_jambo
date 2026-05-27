use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use uuid::Uuid;

use super::{BenchmarkCleanupCounts, GameOrchestrator};
use crate::database::models::{game, game_card, game_invite, player, player_profile, user};
use crate::error::GameError;

impl GameOrchestrator {
    pub async fn cleanup_benchmark_data(&self) -> Result<BenchmarkCleanupCounts, GameError> {
        let span = tracing::info_span!("cleanup_benchmark_data");
        let _guard = span.enter();

        let benchmark_users: Vec<Uuid> = user::Entity::find()
            .filter(user::Column::Email.contains("@benchmark.local"))
            .all(&self.db)
            .await
            .map_err(GameError::Database)?
            .into_iter()
            .map(|u| u.id)
            .collect();

        let benchmark_games: Vec<Uuid> = game::Entity::find()
            .filter(game::Column::GameMode.eq(crate::database::models::GameMode::Multiplayer))
            .filter(game::Column::CreatorId.is_in(benchmark_users.clone()))
            .all(&self.db)
            .await
            .map_err(GameError::Database)?
            .into_iter()
            .map(|g| g.id)
            .collect();

        let player_profiles_deleted = player_profile::Entity::delete_many()
            .filter(player_profile::Column::UserId.is_in(benchmark_users.clone()))
            .exec(&self.db)
            .await
            .map_err(GameError::Database)?
            .rows_affected;

        let game_invites_deleted = game_invite::Entity::delete_many()
            .filter(game_invite::Column::InvitedUserId.is_in(benchmark_users.clone()))
            .exec(&self.db)
            .await
            .map_err(GameError::Database)?
            .rows_affected;

        let game_cards_deleted = if !benchmark_games.is_empty() {
            game_card::Entity::delete_many()
                .filter(game_card::Column::GameId.is_in(benchmark_games.clone()))
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
            user::Entity::delete_many()
                .filter(user::Column::Id.is_in(benchmark_users.clone()))
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
