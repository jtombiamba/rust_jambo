use async_trait::async_trait;
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, DatabaseConnection, DatabaseTransaction, DbErr,
    EntityTrait, QueryFilter, QueryOrder, Set,
};
use uuid::Uuid;

use crate::database::models::{player, Player, PlayerType};
use crate::database::traits::PlayerRepoTrait;

/// A single player row for a quick game, ready to be persisted. Kept separate
/// from `Player` so callers can build a heterogeneous human + bot roster
/// without instantiating the full model.
#[derive(Debug, Clone)]
pub struct QuickGamePlayerRow {
    pub name: String,
    pub position: i32,
    pub player_type: PlayerType,
    pub credits: i32,
    pub user_id: Option<Uuid>,
}

#[derive(Debug, Clone)]
pub struct PlayerRepository {
    connection: DatabaseConnection,
}

impl PlayerRepository {
    pub fn new(connection: DatabaseConnection) -> Self {
        Self { connection }
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn create(
        &self,
        game_id: Uuid,
        player_type: PlayerType,
        name: &str,
        position: i32,
    ) -> Result<Player, DbErr> {
        let id = Uuid::now_v7();
        let now = chrono::Utc::now();

        let player_active = player::ActiveModel {
            id: Set(id),
            game_id: Set(game_id),
            player_type: Set(player_type),
            name: Set(name.to_string()),
            position: Set(position),
            credits: Set(500),
            created_at: Set(now),
            user_id: ActiveValue::NotSet,
            kicked: Set(false),
            kicked_at: ActiveValue::NotSet,
        };
        let insert_result = player::Entity::insert(player_active)
            .exec(&self.connection)
            .await?;
        let inserted_id = insert_result.last_insert_id;
        let player = player::Entity::find_by_id(inserted_id)
            .one(&self.connection)
            .await?
            .ok_or_else(|| DbErr::Custom("Player not found after insertion".to_string()))?;
        Ok(player)
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn list_by_game(&self, game_id: Uuid) -> Result<Vec<Player>, DbErr> {
        player::Entity::find()
            .filter(player::Column::GameId.eq(game_id))
            .order_by_asc(player::Column::Position)
            .all(&self.connection)
            .await
    }

    #[allow(dead_code)]
    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn update_credits(&self, player_id: Uuid, credits: i32) -> Result<Player, DbErr> {
        let model = player::Entity::find_by_id(player_id)
            .one(&self.connection)
            .await?
            .ok_or_else(|| DbErr::Custom("Player not found".to_string()))?;
        let mut active: player::ActiveModel = model.into();
        active.credits = Set(credits);
        active.update(&self.connection).await
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn create_with_user(
        &self,
        game_id: Uuid,
        player_type: PlayerType,
        name: &str,
        position: i32,
        user_id: Uuid,
    ) -> Result<Player, DbErr> {
        let id = Uuid::now_v7();
        let now = chrono::Utc::now();

        let player_active = player::ActiveModel {
            id: Set(id),
            game_id: Set(game_id),
            player_type: Set(player_type),
            name: Set(name.to_string()),
            position: Set(position),
            credits: Set(500),
            created_at: Set(now),
            user_id: Set(Some(user_id)),
            kicked: Set(false),
            kicked_at: ActiveValue::NotSet,
        };
        let insert_result = player::Entity::insert(player_active)
            .exec(&self.connection)
            .await?;
        let inserted_id = insert_result.last_insert_id;
        let player = player::Entity::find_by_id(inserted_id)
            .one(&self.connection)
            .await?
            .ok_or_else(|| DbErr::Custom("Player not found after insertion".to_string()))?;
        Ok(player)
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    #[allow(dead_code)]
    pub async fn find_by_game_and_user(
        &self,
        game_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<Player>, DbErr> {
        player::Entity::find()
            .filter(player::Column::GameId.eq(game_id))
            .filter(player::Column::UserId.eq(user_id))
            .one(&self.connection)
            .await
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn find_active_player_in_game(
        &self,
        game_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<Player>, DbErr> {
        player::Entity::find()
            .filter(player::Column::GameId.eq(game_id))
            .filter(player::Column::UserId.eq(user_id))
            .filter(player::Column::Kicked.eq(false))
            .one(&self.connection)
            .await
    }

    #[tracing::instrument(skip(txn), fields(db.statement, db.rows_affected))]
    #[allow(clippy::too_many_arguments)]
    pub async fn create_player_for_run_in_txn(
        &self,
        txn: &DatabaseTransaction,
        player_id: Uuid,
        game_id: Uuid,
        user_id: Uuid,
        name: &str,
        position: i32,
        credits: i32,
    ) -> Result<Player, DbErr> {
        let now = chrono::Utc::now();
        player::Entity::insert(player::ActiveModel {
            id: Set(player_id),
            game_id: Set(game_id),
            player_type: Set(PlayerType::Human),
            name: Set(name.to_string()),
            position: Set(position),
            credits: Set(credits),
            created_at: Set(now),
            user_id: Set(Some(user_id)),
            kicked: Set(false),
            kicked_at: ActiveValue::NotSet,
        })
        .exec_without_returning(txn)
        .await?;

        Ok(Player {
            id: player_id,
            game_id,
            player_type: PlayerType::Human,
            name: name.to_string(),
            position,
            credits,
            created_at: now,
            user_id: Some(user_id),
            kicked: false,
            kicked_at: None,
        })
    }

    #[tracing::instrument(skip(txn), fields(db.statement, db.rows_affected))]
    pub async fn list_by_game_in_txn(
        &self,
        txn: &DatabaseTransaction,
        game_id: Uuid,
    ) -> Result<Vec<Player>, DbErr> {
        player::Entity::find()
            .filter(player::Column::GameId.eq(game_id))
            .order_by_asc(player::Column::Position)
            .all(txn)
            .await
    }

    #[tracing::instrument(skip(txn), fields(db.statement, db.rows_affected))]
    #[allow(clippy::too_many_arguments)]
    pub async fn create_with_user_in_txn(
        &self,
        txn: &DatabaseTransaction,
        player_id: Uuid,
        game_id: Uuid,
        user_id: Uuid,
        name: &str,
        position: i32,
        credits: i32,
    ) -> Result<(), DbErr> {
        let now = chrono::Utc::now();
        player::Entity::insert(player::ActiveModel {
            id: Set(player_id),
            game_id: Set(game_id),
            player_type: Set(PlayerType::Human),
            name: Set(name.to_string()),
            position: Set(position),
            credits: Set(credits),
            created_at: Set(now),
            user_id: Set(Some(user_id)),
            kicked: Set(false),
            kicked_at: ActiveValue::NotSet,
        })
        .exec_without_returning(txn)
        .await?;
        Ok(())
    }

    #[tracing::instrument(skip(txn), fields(db.statement, db.rows_affected))]
    pub async fn create_quick_game_players_in_txn(
        &self,
        txn: &DatabaseTransaction,
        game_id: Uuid,
        rows: Vec<QuickGamePlayerRow>,
    ) -> Result<(), DbErr> {
        if rows.is_empty() {
            return Ok(());
        }
        let now = chrono::Utc::now();
        let models: Vec<player::ActiveModel> = rows
            .into_iter()
            .map(|row| player::ActiveModel {
                id: Set(Uuid::now_v7()),
                game_id: Set(game_id),
                player_type: Set(row.player_type),
                name: Set(row.name),
                position: Set(row.position),
                credits: Set(row.credits),
                created_at: Set(now),
                user_id: Set(row.user_id),
                kicked: Set(false),
                kicked_at: ActiveValue::NotSet,
            })
            .collect();
        player::Entity::insert_many(models)
            .exec_without_returning(txn)
            .await?;
        Ok(())
    }
}

#[async_trait]
#[allow(dead_code)]
impl PlayerRepoTrait for PlayerRepository {
    async fn create(
        &self,
        game_id: Uuid,
        player_type: PlayerType,
        name: &str,
        position: i32,
    ) -> Result<Player, DbErr> {
        self.create(game_id, player_type, name, position).await
    }

    async fn create_with_user(
        &self,
        game_id: Uuid,
        player_type: PlayerType,
        name: &str,
        position: i32,
        user_id: Uuid,
    ) -> Result<Player, DbErr> {
        self.create_with_user(game_id, player_type, name, position, user_id)
            .await
    }

    async fn list_by_game(&self, game_id: Uuid) -> Result<Vec<Player>, DbErr> {
        self.list_by_game(game_id).await
    }

    async fn find_by_game_and_user(
        &self,
        game_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<Player>, DbErr> {
        self.find_by_game_and_user(game_id, user_id).await
    }

    async fn create_player_for_run_in_txn(
        &self,
        txn: &DatabaseTransaction,
        player_id: Uuid,
        game_id: Uuid,
        user_id: Uuid,
        name: &str,
        position: i32,
        credits: i32,
    ) -> Result<Player, DbErr> {
        self.create_player_for_run_in_txn(txn, player_id, game_id, user_id, name, position, credits)
            .await
    }

    async fn list_by_game_in_txn(
        &self,
        txn: &DatabaseTransaction,
        game_id: Uuid,
    ) -> Result<Vec<Player>, DbErr> {
        self.list_by_game_in_txn(txn, game_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm::{DatabaseBackend, MockDatabase, MockExecResult, TransactionTrait};

    fn row(name: &str, position: i32, player_type: PlayerType) -> QuickGamePlayerRow {
        QuickGamePlayerRow {
            name: name.to_string(),
            position,
            player_type,
            credits: 0,
            user_id: None,
        }
    }

    #[tokio::test]
    async fn create_quick_game_players_inserts_human_and_bots() {
        let game_id = Uuid::now_v7();
        let rows = vec![
            row("You", 0, PlayerType::Human),
            row("Bot East", 1, PlayerType::Bot),
            row("Bot North", 2, PlayerType::Bot),
            row("Bot West", 3, PlayerType::Bot),
        ];

        let db = MockDatabase::new(DatabaseBackend::Postgres)
            .append_exec_results(vec![MockExecResult {
                last_insert_id: 0,
                rows_affected: 4,
            }])
            .into_connection();
        let repo = PlayerRepository::new(db.clone());
        let txn = db.begin().await.unwrap();

        let result = repo
            .create_quick_game_players_in_txn(&txn, game_id, rows)
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn create_quick_game_players_no_ops_on_empty_rows() {
        let db = MockDatabase::new(DatabaseBackend::Postgres).into_connection();
        let repo = PlayerRepository::new(db.clone());
        let txn = db.begin().await.unwrap();

        let result = repo
            .create_quick_game_players_in_txn(&txn, Uuid::now_v7(), vec![])
            .await;

        assert!(result.is_ok());
    }
}
