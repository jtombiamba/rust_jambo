use async_trait::async_trait;
use sea_orm::{
    ActiveValue, ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter, QueryOrder, Set,
};
use uuid::Uuid;

use crate::database::models::{game_card, GameCard};
use crate::database::traits::GameCardRepoTrait;

pub struct GameCardRepository {
    connection: DatabaseConnection,
}

impl GameCardRepository {
    pub fn new(connection: DatabaseConnection) -> Self {
        Self { connection }
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn create(
        &self,
        game_id: Uuid,
        player_id: Option<Uuid>,
        card_index: i32,
        round: Option<i32>,
    ) -> Result<GameCard, DbErr> {
        let id = Uuid::now_v7();
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
        let insert_result = game_card::Entity::insert(card_active)
            .exec(&self.connection)
            .await?;
        let inserted_id = insert_result.last_insert_id;
        let card = game_card::Entity::find_by_id(inserted_id)
            .one(&self.connection)
            .await?
            .ok_or_else(|| DbErr::Custom("GameCard not found after insertion".to_string()))?;
        Ok(card)
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    #[allow(dead_code)]
    pub async fn bulk_insert(&self, cards: Vec<(Uuid, Option<Uuid>, i32)>) -> Result<(), DbErr> {
        let now = chrono::Utc::now();
        let active_models: Vec<game_card::ActiveModel> = cards
            .into_iter()
            .map(|(game_id, player_id, card_index)| game_card::ActiveModel {
                id: Set(Uuid::now_v7()),
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

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn list_by_player(&self, player_id: Uuid) -> Result<Vec<GameCard>, DbErr> {
        game_card::Entity::find()
            .filter(game_card::Column::PlayerId.eq(player_id))
            .order_by_asc(game_card::Column::CardIndex)
            .all(&self.connection)
            .await
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn list_by_game_and_round(
        &self,
        game_id: Uuid,
        round: i32,
    ) -> Result<Vec<GameCard>, DbErr> {
        game_card::Entity::find()
            .filter(game_card::Column::GameId.eq(game_id))
            .filter(game_card::Column::Round.eq(round))
            .filter(game_card::Column::Played.eq(true))
            .order_by_asc(game_card::Column::CardIndex)
            .all(&self.connection)
            .await
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn list_by_game(&self, game_id: Uuid) -> Result<Vec<GameCard>, DbErr> {
        game_card::Entity::find()
            .filter(game_card::Column::GameId.eq(game_id))
            .all(&self.connection)
            .await
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn list_played_by_game(&self, game_id: Uuid) -> Result<Vec<GameCard>, DbErr> {
        game_card::Entity::find()
            .filter(game_card::Column::GameId.eq(game_id))
            .filter(game_card::Column::Played.eq(true))
            .all(&self.connection)
            .await
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    #[allow(dead_code)]
    pub async fn list_by_player_and_round(
        &self,
        player_id: Uuid,
        round: i32,
    ) -> Result<Vec<GameCard>, DbErr> {
        game_card::Entity::find()
            .filter(game_card::Column::PlayerId.eq(player_id))
            .filter(game_card::Column::Round.eq(round))
            .order_by_asc(game_card::Column::CardIndex)
            .all(&self.connection)
            .await
    }
}

#[async_trait]
#[allow(dead_code)]
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

    async fn list_by_game_and_round(
        &self,
        game_id: Uuid,
        round: i32,
    ) -> Result<Vec<GameCard>, DbErr> {
        self.list_by_game_and_round(game_id, round).await
    }

    async fn list_by_game(&self, game_id: Uuid) -> Result<Vec<GameCard>, DbErr> {
        self.list_by_game(game_id).await
    }
}
