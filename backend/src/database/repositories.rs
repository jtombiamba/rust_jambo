use async_trait::async_trait;
use sea_orm::{DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, ColumnTrait, Set, ActiveValue, ActiveModelTrait, DbErr};
use uuid::Uuid;
use serde_json::json;

use crate::database::models::{game, Game, Player, GameCard, PlayerType, GameStatus};
use crate::database::traits::{GameRepoTrait, PlayerRepoTrait, GameCardRepoTrait};

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
        };
        let insert_result = game::Entity::insert(game_active).exec(&self.connection).await?;
        let inserted_id = insert_result.last_insert_id;
        let game = game::Entity::find_by_id(inserted_id)
            .one(&self.connection)
            .await?
            .ok_or_else(|| DbErr::Custom("Game not found after insertion".to_string()))?;
        Ok(game)
    }

    pub async fn find_by_id(&self, id: Uuid) -> Result<Option<Game>, DbErr> {
        game::Entity::find_by_id(id)
            .one(&self.connection)
            .await
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

    pub async fn update_player_positions(&self, id: Uuid, player_id: Uuid, position: i32) -> Result<Game, DbErr> {
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

    pub async fn list_players(&self, game_id: Uuid) -> Result<Vec<Player>, DbErr> {
        use crate::database::models::player;
        player::Entity::find()
            .filter(player::Column::GameId.eq(game_id))
            .order_by_asc(player::Column::Position)
            .all(&self.connection)
            .await
    }
}

#[async_trait]
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

pub struct PlayerRepository {
    connection: DatabaseConnection,
}

impl PlayerRepository {
    pub fn new(connection: DatabaseConnection) -> Self {
        Self { connection }
    }

    pub async fn create(
        &self,
        game_id: Uuid,
        player_type: PlayerType,
        name: &str,
        position: i32,
    ) -> Result<Player, DbErr> {
        use crate::database::models::player;
        let id = Uuid::new_v4();
        let now = chrono::Utc::now();

        let player_active = player::ActiveModel {
            id: Set(id),
            game_id: Set(game_id),
            player_type: Set(player_type),
            name: Set(name.to_string()),
            position: Set(position),
            credits: Set(500),
            created_at: Set(now),
        };
        let insert_result = player::Entity::insert(player_active).exec(&self.connection).await?;
        let inserted_id = insert_result.last_insert_id;
        let player = player::Entity::find_by_id(inserted_id)
            .one(&self.connection)
            .await?
            .ok_or_else(|| DbErr::Custom("Player not found after insertion".to_string()))?;
        Ok(player)
    }

    pub async fn list_by_game(&self, game_id: Uuid) -> Result<Vec<Player>, DbErr> {
        use crate::database::models::player;
        player::Entity::find()
            .filter(player::Column::GameId.eq(game_id))
            .order_by_asc(player::Column::Position)
            .all(&self.connection)
            .await
    }
}

#[async_trait]
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

    async fn list_by_game(&self, game_id: Uuid) -> Result<Vec<Player>, DbErr> {
        self.list_by_game(game_id).await
    }
}

pub struct GameCardRepository {
    connection: DatabaseConnection,
}

impl GameCardRepository {
    pub fn new(connection: DatabaseConnection) -> Self {
        Self { connection }
    }

    pub async fn create(
        &self,
        game_id: Uuid,
        player_id: Option<Uuid>,
        card_index: i32,
        round: Option<i32>,
    ) -> Result<GameCard, DbErr> {
        use crate::database::models::game_card;
        let id = Uuid::new_v4();
        let now = chrono::Utc::now();

        let card_active = game_card::ActiveModel {
            id: Set(id),
            game_id: Set(game_id),
            player_id: Set(player_id),
            card_index: Set(card_index),
            played: Set(false),
            played_at: ActiveValue::NotSet,
            round: Set(round),
            created_at: Set(now),
        };
        let insert_result = game_card::Entity::insert(card_active).exec(&self.connection).await?;
        let inserted_id = insert_result.last_insert_id;
        let card = game_card::Entity::find_by_id(inserted_id)
            .one(&self.connection)
            .await?
            .ok_or_else(|| DbErr::Custom("GameCard not found after insertion".to_string()))?;
        Ok(card)
    }

    pub async fn bulk_insert(&self, cards: Vec<(Uuid, Option<Uuid>, i32)>) -> Result<(), DbErr> {
        use crate::database::models::game_card;
        let now = chrono::Utc::now();
        let active_models: Vec<game_card::ActiveModel> = cards
            .into_iter()
            .map(|(game_id, player_id, card_index)| game_card::ActiveModel {
                id: Set(Uuid::new_v4()),
                game_id: Set(game_id),
                player_id: Set(player_id),
                card_index: Set(card_index),
                played: Set(false),
                played_at: ActiveValue::NotSet,
                round: Set(None),
                created_at: Set(now),
            })
            .collect();
        if !active_models.is_empty() {
            game_card::Entity::insert_many(active_models)
                .exec(&self.connection)
                .await?;
        }
        Ok(())
    }

    pub async fn list_by_player(&self, player_id: Uuid) -> Result<Vec<GameCard>, DbErr> {
        use crate::database::models::game_card;
        game_card::Entity::find()
            .filter(game_card::Column::PlayerId.eq(player_id))
            .order_by_asc(game_card::Column::CardIndex)
            .all(&self.connection)
            .await
    }

    pub async fn list_by_game_and_round(&self, game_id: Uuid, round: i32) -> Result<Vec<GameCard>, DbErr> {
        use crate::database::models::game_card;
        game_card::Entity::find()
            .filter(game_card::Column::GameId.eq(game_id))
            .filter(game_card::Column::Round.eq(round))
            .filter(game_card::Column::Played.eq(true))
            .order_by_asc(game_card::Column::CardIndex)
            .all(&self.connection)
            .await
    }

    pub async fn list_by_player_and_round(
        &self,
        player_id: Uuid,
        round: i32,
    ) -> Result<Vec<GameCard>, DbErr> {
        use crate::database::models::game_card;
        game_card::Entity::find()
            .filter(game_card::Column::PlayerId.eq(player_id))
            .filter(game_card::Column::Round.eq(round))
            .order_by_asc(game_card::Column::CardIndex)
            .all(&self.connection)
            .await
    }
}

#[async_trait]
impl GameCardRepoTrait for GameCardRepository {
    async fn create(
        &self,
        game_id: Uuid,
        player_id: Option<Uuid>,
        card_index: i32,
        round: Option<i32>,
    ) -> Result<GameCard, DbErr> {
        self.create(game_id, player_id, card_index, round).await
    }

    async fn bulk_insert(&self, cards: Vec<(Uuid, Option<Uuid>, i32)>) -> Result<(), DbErr> {
        self.bulk_insert(cards).await
    }

    async fn list_by_player(&self, player_id: Uuid) -> Result<Vec<GameCard>, DbErr> {
        self.list_by_player(player_id).await
    }

    async fn list_by_game_and_round(&self, game_id: Uuid, round: i32) -> Result<Vec<GameCard>, DbErr> {
        self.list_by_game_and_round(game_id, round).await
    }
}
