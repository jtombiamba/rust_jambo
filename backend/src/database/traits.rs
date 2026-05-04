use async_trait::async_trait;
use sea_orm::DbErr;
use uuid::Uuid;

use crate::database::models::{Game, Player, GameCard, PlayerType, GameStatus};

/// Repository trait for Game entity operations.
#[async_trait]
pub trait GameRepoTrait: Send + Sync {
    async fn create(&self, bet: i32, auto: bool) -> Result<Game, DbErr>;
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Game>, DbErr>;
    async fn update_rank(&self, id: Uuid, rank: Option<i32>) -> Result<Game, DbErr>;
    async fn update_status(&self, id: Uuid, status: GameStatus) -> Result<Game, DbErr>;
    async fn update_winner(&self, id: Uuid, winner_id: Option<Uuid>) -> Result<Game, DbErr>;
    async fn list_players(&self, game_id: Uuid) -> Result<Vec<Player>, DbErr>;
}

/// Repository trait for Player entity operations.
#[async_trait]
pub trait PlayerRepoTrait: Send + Sync {
    async fn create(
        &self,
        game_id: Uuid,
        player_type: PlayerType,
        name: &str,
        position: i32,
    ) -> Result<Player, DbErr>;
    async fn list_by_game(&self, game_id: Uuid) -> Result<Vec<Player>, DbErr>;
}

/// Repository trait for GameCard entity operations.
#[async_trait]
pub trait GameCardRepoTrait: Send + Sync {
    async fn create(
        &self,
        game_id: Uuid,
        player_id: Option<Uuid>,
        card_index: i32,
        round: Option<i32>,
    ) -> Result<GameCard, DbErr>;
    async fn bulk_insert(&self, cards: Vec<(Uuid, Option<Uuid>, i32)>) -> Result<(), DbErr>;
    async fn list_by_player(&self, player_id: Uuid) -> Result<Vec<GameCard>, DbErr>;
    async fn list_by_game_and_round(&self, game_id: Uuid, round: i32) -> Result<Vec<GameCard>, DbErr>;
}
