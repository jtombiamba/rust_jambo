use async_trait::async_trait;
use sea_orm::DbErr;
use uuid::Uuid;

use crate::database::models::game_invite;
use crate::database::models::{
    Game, GameCard, GameStatus, Player, PlayerProfile, PlayerType, User,
};

use crate::api::dto::dashboard::GameFilter;

#[async_trait]
pub trait UserRepoTrait: Send + Sync {
    async fn find_by_email(&self, email: &str) -> Result<Option<User>, DbErr>;
    async fn find_by_id(&self, id: Uuid) -> Result<Option<User>, DbErr>;
    async fn find_by_pseudo(&self, pseudo: &str) -> Result<Option<User>, DbErr>;
    #[allow(dead_code)]
    async fn find_by_pseudo_prefix(&self, prefix: &str, limit: u64) -> Result<Vec<User>, DbErr>;
    async fn create_user_with_profile(
        &self,
        pseudo: &str,
        email: &str,
        password_hash: &str,
        ip_hash: Option<&str>,
    ) -> Result<(User, PlayerProfile), DbErr>;
    async fn update_password_hash(&self, id: Uuid, hash: &str) -> Result<User, DbErr>;
    async fn update_last_ip_hash(&self, id: Uuid, hash: &str) -> Result<User, DbErr>;
    #[allow(dead_code)]
    async fn update_language(&self, id: Uuid, language: &str) -> Result<User, DbErr>;
}

#[async_trait]
#[allow(dead_code)]
pub trait PlayerProfileRepoTrait: Send + Sync {
    async fn find_by_user_id(&self, user_id: Uuid) -> Result<Option<PlayerProfile>, DbErr>;
    async fn update_stats(
        &self,
        user_id: Uuid,
        wins_delta: i32,
        kora_wins_delta: i32,
    ) -> Result<PlayerProfile, DbErr>;
}

/// Repository trait for Game entity operations.
#[async_trait]
#[allow(dead_code)]
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
#[allow(dead_code)]
pub trait PlayerRepoTrait: Send + Sync {
    async fn create(
        &self,
        game_id: Uuid,
        player_type: PlayerType,
        name: &str,
        position: i32,
    ) -> Result<Player, DbErr>;
    async fn create_with_user(
        &self,
        game_id: Uuid,
        player_type: PlayerType,
        name: &str,
        position: i32,
        user_id: Uuid,
    ) -> Result<Player, DbErr>;
    async fn list_by_game(&self, game_id: Uuid) -> Result<Vec<Player>, DbErr>;
    async fn find_by_game_and_user(
        &self,
        game_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<Player>, DbErr>;
}

/// Repository trait for GameCard entity operations.
#[async_trait]
#[allow(dead_code)]
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
    async fn list_by_game_and_round(
        &self,
        game_id: Uuid,
        round: i32,
    ) -> Result<Vec<GameCard>, DbErr>;
}

#[async_trait]
pub trait DashboardRepoTrait: Send + Sync {
    async fn find_profile_by_user_id(&self, user_id: Uuid) -> Result<Option<PlayerProfile>, DbErr>;
    async fn list_players_for_user(&self, user_id: Uuid) -> Result<Vec<Player>, DbErr>;
    async fn find_player_by_game_and_user(
        &self,
        game_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<Player>, DbErr>;
    async fn find_game_by_id(&self, game_id: Uuid) -> Result<Option<Game>, DbErr>;
    async fn list_players_by_game_ordered(&self, game_id: Uuid) -> Result<Vec<Player>, DbErr>;
    async fn find_cards_for_player(
        &self,
        player_id: Uuid,
        unplayed_only: bool,
    ) -> Result<Vec<GameCard>, DbErr>;
    async fn find_all_cards_for_game(&self, game_id: Uuid) -> Result<Vec<GameCard>, DbErr>;
    async fn list_players_for_user_filtered(
        &self,
        user_id: Uuid,
        filter: GameFilter,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<(Player, Game)>, u64), DbErr>;
    async fn find_user_by_id(&self, id: Uuid) -> Result<Option<User>, DbErr>;
    async fn find_user_by_pseudo(&self, pseudo: &str) -> Result<Option<User>, DbErr>;
    async fn find_users_by_pseudo_prefix(
        &self,
        prefix: &str,
        limit: u64,
    ) -> Result<Vec<User>, DbErr>;
    async fn list_pending_invites_for_user(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<(game_invite::Model, Game)>, DbErr>;
}

#[allow(dead_code)]
#[async_trait]
pub trait GameInviteRepoTrait: Send + Sync {
    async fn create_invite(
        &self,
        game_id: Uuid,
        invited_user_id: Uuid,
    ) -> Result<game_invite::Model, DbErr>;
    async fn find_invite(
        &self,
        game_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<game_invite::Model>, DbErr>;
    async fn update_invite_status(
        &self,
        invite_id: Uuid,
        status: crate::database::models::InviteStatus,
    ) -> Result<game_invite::Model, DbErr>;
    async fn list_pending_invites_for_user(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<(game_invite::Model, Game)>, DbErr>;
}
