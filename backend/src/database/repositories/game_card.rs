use async_trait::async_trait;
use sea_orm::{
    ActiveValue, ColumnTrait, DatabaseConnection, DatabaseTransaction, DbErr, EntityTrait,
    QueryFilter, QueryOrder, Set,
};
use uuid::Uuid;

use crate::database::models::{game_card, GameCard};
use crate::database::traits::GameCardRepoTrait;

#[derive(Debug, Clone)]
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

    #[tracing::instrument(skip(txn), fields(db.statement, db.rows_affected))]
    pub async fn list_by_player_in_txn(
        &self,
        txn: &DatabaseTransaction,
        player_id: Uuid,
    ) -> Result<Vec<GameCard>, DbErr> {
        game_card::Entity::find()
            .filter(game_card::Column::PlayerId.eq(player_id))
            .order_by_asc(game_card::Column::CardIndex)
            .all(txn)
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

    #[tracing::instrument(skip(txn), fields(db.statement, db.rows_affected))]
    pub async fn list_played_by_game_and_round_in_txn(
        &self,
        txn: &DatabaseTransaction,
        game_id: Uuid,
        round: i32,
    ) -> Result<Vec<GameCard>, DbErr> {
        game_card::Entity::find()
            .filter(game_card::Column::GameId.eq(game_id))
            .filter(game_card::Column::Round.eq(round))
            .filter(game_card::Column::Played.eq(true))
            .order_by_asc(game_card::Column::PlayedAt)
            .all(txn)
            .await
    }

    #[tracing::instrument(skip(txn), fields(db.statement, db.rows_affected))]
    pub async fn find_played_card_in_round_in_txn(
        &self,
        txn: &DatabaseTransaction,
        player_id: Uuid,
        round: i32,
    ) -> Result<Option<GameCard>, DbErr> {
        game_card::Entity::find()
            .filter(game_card::Column::PlayerId.eq(player_id))
            .filter(game_card::Column::Round.eq(round))
            .filter(game_card::Column::Played.eq(true))
            .one(txn)
            .await
    }

    #[tracing::instrument(skip(txn), fields(db.statement, db.rows_affected))]
    pub async fn bulk_insert_in_txn(
        &self,
        txn: &DatabaseTransaction,
        cards: Vec<crate::database::models::game_card::ActiveModel>,
    ) -> Result<(), DbErr> {
        if !cards.is_empty() {
            crate::database::models::game_card::Entity::insert_many(cards)
                .exec(txn)
                .await?;
        }
        Ok(())
    }

    #[tracing::instrument(skip(txn), fields(db.statement, db.rows_affected))]
    pub async fn create_quick_game_cards_in_txn(
        &self,
        txn: &DatabaseTransaction,
        game_id: Uuid,
        cards: Vec<(Uuid, i32)>,
    ) -> Result<(), DbErr> {
        if cards.is_empty() {
            return Ok(());
        }
        let now = chrono::Utc::now();
        let models: Vec<game_card::ActiveModel> = cards
            .into_iter()
            .map(|(player_id, card_index)| game_card::ActiveModel {
                id: Set(Uuid::now_v7()),
                game_id: Set(game_id),
                player_id: Set(Some(player_id)),
                card_index: Set(card_index),
                played: Set(false),
                played_at: ActiveValue::NotSet,
                round: Set(None),
                created_at: Set(now),
            })
            .collect();
        game_card::Entity::insert_many(models)
            .exec_without_returning(txn)
            .await?;
        Ok(())
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

    async fn bulk_insert_in_txn(
        &self,
        txn: &DatabaseTransaction,
        cards: Vec<crate::database::models::game_card::ActiveModel>,
    ) -> Result<(), DbErr> {
        self.bulk_insert_in_txn(txn, cards).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm::{DatabaseBackend, MockDatabase, MockExecResult, TransactionTrait};

    #[tokio::test]
    async fn create_quick_game_cards_inserts_assigned_cards() {
        let game_id = Uuid::now_v7();
        let cards = vec![
            (Uuid::now_v7(), 0),
            (Uuid::now_v7(), 5),
            (Uuid::now_v7(), 12),
        ];

        let db = MockDatabase::new(DatabaseBackend::Postgres)
            .append_exec_results(vec![MockExecResult {
                last_insert_id: 0,
                rows_affected: 3,
            }])
            .into_connection();
        let repo = GameCardRepository::new(db.clone());
        let txn = db.begin().await.unwrap();

        let result = repo
            .create_quick_game_cards_in_txn(&txn, game_id, cards)
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn create_quick_game_cards_no_ops_on_empty() {
        let db = MockDatabase::new(DatabaseBackend::Postgres).into_connection();
        let repo = GameCardRepository::new(db.clone());
        let txn = db.begin().await.unwrap();

        let result = repo
            .create_quick_game_cards_in_txn(&txn, Uuid::now_v7(), vec![])
            .await;

        assert!(result.is_ok());
    }
}
