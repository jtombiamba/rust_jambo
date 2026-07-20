use async_trait::async_trait;
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, DatabaseConnection, DbErr, EntityTrait,
    QueryFilter, QueryOrder, Set,
};
use uuid::Uuid;

use crate::database::models::{player, Player, PlayerType};
use crate::database::traits::PlayerRepoTrait;

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
}
