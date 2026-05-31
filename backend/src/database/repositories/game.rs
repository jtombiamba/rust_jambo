use async_trait::async_trait;
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, DatabaseConnection, DbErr, EntityTrait,
    QueryFilter, QueryOrder, Set,
};
use serde_json::json;
use uuid::Uuid;

use crate::database::models::{game, player, Game, GameMode, GameStatus, Player};
use crate::database::traits::GameRepoTrait;

pub struct GameRepository {
    connection: DatabaseConnection,
}

impl GameRepository {
    pub fn new(connection: DatabaseConnection) -> Self {
        Self { connection }
    }

    pub async fn create(&self, bet: i32, auto: bool) -> Result<Game, DbErr> {
        let id = Uuid::new_v4();
        let now = chrono::Utc::now();

        let game_active = game::ActiveModel {
            id: Set(id),
            status: Set(GameStatus::Pending),
            bet: Set(bet),
            created_at: Set(now),
            updated_at: Set(now),
            finished_at: ActiveValue::NotSet,
            rank: ActiveValue::NotSet,
            roll: Set(1),
            auto: Set(auto),
            winner_id: ActiveValue::NotSet,
            player_positions: Set(json!({})),
            current_winning_card: ActiveValue::NotSet,
            current_winning_player_position: ActiveValue::NotSet,
            creator_id: ActiveValue::NotSet,
            game_mode: Set(GameMode::Solo),
            max_players: Set(4),
            invite_expires_at: ActiveValue::NotSet,
            stall_warning_sent_at: ActiveValue::NotSet,
            game_run_id: ActiveValue::NotSet,
            kicked_players: Set(json!([])),
        };
        let insert_result = game::Entity::insert(game_active)
            .exec(&self.connection)
            .await?;
        let inserted_id = insert_result.last_insert_id;
        let game = game::Entity::find_by_id(inserted_id)
            .one(&self.connection)
            .await?
            .ok_or_else(|| DbErr::Custom("Game not found after insertion".to_string()))?;
        Ok(game)
    }

    #[allow(dead_code)]
    pub async fn create_with_mode(
        &self,
        bet: i32,
        game_mode: GameMode,
        initial_status: GameStatus,
    ) -> Result<Game, DbErr> {
        let id = Uuid::new_v4();
        let now = chrono::Utc::now();

        let game_active = game::ActiveModel {
            id: Set(id),
            status: Set(initial_status),
            bet: Set(bet),
            created_at: Set(now),
            updated_at: Set(now),
            finished_at: ActiveValue::NotSet,
            rank: ActiveValue::NotSet,
            roll: Set(1),
            auto: Set(false),
            winner_id: ActiveValue::NotSet,
            player_positions: Set(json!({})),
            current_winning_card: ActiveValue::NotSet,
            current_winning_player_position: ActiveValue::NotSet,
            creator_id: ActiveValue::NotSet,
            game_mode: Set(game_mode),
            max_players: Set(4),
            invite_expires_at: ActiveValue::NotSet,
            stall_warning_sent_at: ActiveValue::NotSet,
            game_run_id: ActiveValue::NotSet,
            kicked_players: Set(json!([])),
        };
        let insert_result = game::Entity::insert(game_active)
            .exec(&self.connection)
            .await?;
        let inserted_id = insert_result.last_insert_id;
        let game = game::Entity::find_by_id(inserted_id)
            .one(&self.connection)
            .await?
            .ok_or_else(|| DbErr::Custom("Game not found after insertion".to_string()))?;
        Ok(game)
    }

    #[allow(dead_code)]
    pub async fn create_with_mode_and_creator(
        &self,
        bet: i32,
        game_mode: GameMode,
        initial_status: GameStatus,
        creator_id: Option<Uuid>,
    ) -> Result<Game, DbErr> {
        let id = Uuid::new_v4();
        let now = chrono::Utc::now();

        let game_active = game::ActiveModel {
            id: Set(id),
            status: Set(initial_status),
            bet: Set(bet),
            created_at: Set(now),
            updated_at: Set(now),
            finished_at: ActiveValue::NotSet,
            rank: ActiveValue::NotSet,
            roll: Set(1),
            auto: Set(false),
            winner_id: ActiveValue::NotSet,
            player_positions: Set(json!({})),
            current_winning_card: ActiveValue::NotSet,
            current_winning_player_position: ActiveValue::NotSet,
            creator_id: Set(creator_id),
            game_mode: Set(game_mode),
            max_players: Set(4),
            invite_expires_at: ActiveValue::NotSet,
            stall_warning_sent_at: ActiveValue::NotSet,
            game_run_id: ActiveValue::NotSet,
            kicked_players: Set(json!([])),
        };
        let insert_result = game::Entity::insert(game_active)
            .exec(&self.connection)
            .await?;
        let inserted_id = insert_result.last_insert_id;
        let game = game::Entity::find_by_id(inserted_id)
            .one(&self.connection)
            .await?
            .ok_or_else(|| DbErr::Custom("Game not found after insertion".to_string()))?;
        Ok(game)
    }

    pub async fn find_by_id(&self, id: Uuid) -> Result<Option<Game>, DbErr> {
        game::Entity::find_by_id(id).one(&self.connection).await
    }

    pub async fn update_rank(&self, id: Uuid, rank: Option<i32>) -> Result<Game, DbErr> {
        let mut active: game::ActiveModel = game::Entity::find_by_id(id)
            .one(&self.connection)
            .await?
            .ok_or_else(|| DbErr::Custom("Game not found".to_string()))?
            .into();
        active.rank = Set(rank);
        active.updated_at = Set(chrono::Utc::now());
        active.update(&self.connection).await
    }

    pub async fn update_status(&self, id: Uuid, status: GameStatus) -> Result<Game, DbErr> {
        let mut active: game::ActiveModel = game::Entity::find_by_id(id)
            .one(&self.connection)
            .await?
            .ok_or_else(|| DbErr::Custom("Game not found".to_string()))?
            .into();
        active.status = Set(status);
        active.updated_at = Set(chrono::Utc::now());
        active.update(&self.connection).await
    }

    #[allow(dead_code)]
    pub async fn update_winner(&self, id: Uuid, winner_id: Option<Uuid>) -> Result<Game, DbErr> {
        let mut active: game::ActiveModel = game::Entity::find_by_id(id)
            .one(&self.connection)
            .await?
            .ok_or_else(|| DbErr::Custom("Game not found".to_string()))?
            .into();
        active.winner_id = Set(winner_id);
        active.updated_at = Set(chrono::Utc::now());
        active.update(&self.connection).await
    }

    #[allow(dead_code)]
    pub async fn update_player_positions(
        &self,
        id: Uuid,
        player_id: Uuid,
        position: i32,
    ) -> Result<Game, DbErr> {
        let game_model = game::Entity::find_by_id(id)
            .one(&self.connection)
            .await?
            .ok_or_else(|| DbErr::Custom("Game not found".to_string()))?;

        let current_positions = game_model.player_positions.clone();
        let mut positions_map = current_positions.as_object().cloned().unwrap_or_default();
        positions_map.insert(player_id.to_string(), json!(position));

        let mut active: game::ActiveModel = game_model.into();
        active.player_positions = Set(json!(positions_map));
        active.updated_at = Set(chrono::Utc::now());
        active.update(&self.connection).await
    }

    #[allow(dead_code)]
    pub async fn list_players(&self, game_id: Uuid) -> Result<Vec<Player>, DbErr> {
        player::Entity::find()
            .filter(player::Column::GameId.eq(game_id))
            .order_by_asc(player::Column::Position)
            .all(&self.connection)
            .await
    }
}

#[async_trait]
#[allow(dead_code)]
impl GameRepoTrait for GameRepository {
    async fn create(&self, bet: i32, auto: bool) -> Result<Game, DbErr> {
        self.create(bet, auto).await
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<Game>, DbErr> {
        self.find_by_id(id).await
    }

    async fn update_rank(&self, id: Uuid, rank: Option<i32>) -> Result<Game, DbErr> {
        self.update_rank(id, rank).await
    }

    async fn update_status(&self, id: Uuid, status: GameStatus) -> Result<Game, DbErr> {
        self.update_status(id, status).await
    }

    async fn update_winner(&self, id: Uuid, winner_id: Option<Uuid>) -> Result<Game, DbErr> {
        self.update_winner(id, winner_id).await
    }

    async fn list_players(&self, game_id: Uuid) -> Result<Vec<Player>, DbErr> {
        self.list_players(game_id).await
    }
}
