use async_trait::async_trait;
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, DatabaseConnection, DatabaseTransaction, DbErr,
    EntityTrait, QueryFilter, QueryOrder, Set,
};
use uuid::Uuid;

use crate::database::models::{player, Player, PlayerType};
use crate::database::traits::PlayerRepoTrait;

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
        .exec(txn)
        .await?;
        Ok(())
    }

    #[tracing::instrument(skip(txn), fields(db.statement, db.rows_affected))]
    pub async fn list_by_game_in_txn(
        &self,
        txn: &DatabaseTransaction,
        game_id: Uuid,
    ) -> Result<Vec<Player>, DbErr> {
        player::Entity::find()
            .filter(player::Column::GameId.eq(game_id))
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
    ) -> Result<(), DbErr> {
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
