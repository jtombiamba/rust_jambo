use async_trait::async_trait;
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api::dto::responses::{
    MultiplayerGameResponse, PlayCardResponse, PlayerInfoDto, QuickGameResponse,
};
use crate::error::GameError;
use crate::observability::CorrelationId;

/// Outcome of a play_card operation — contains everything the API handler
/// needs to build the HTTP response without accessing repositories.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayCardOutcome {
    pub card_id: Uuid,
    pub next_turn: Option<Uuid>,
    pub game_ended: bool,
    pub round_completed: bool,
    pub current_round: i32,
}

/// Outcome of a create_quick_game operation.
#[derive(Debug, Clone)]
pub struct QuickGameOutcome {
    pub game_id: Uuid,
    pub players: Vec<PlayerInfoDto>,
    pub status: String,
    pub current_turn: i32,
    pub bet: i32,
    pub max_players: i32,
    pub invite_expires_at: Option<String>,
    pub deck_slots: Option<Vec<Option<i32>>>,
}

/// Outcome of a create_multiplayer_game operation.
#[derive(Debug, Clone)]
pub struct MultiplayerCreationOutcome {
    pub game_id: Uuid,
    pub status: String,
    pub bet: i32,
    pub max_players: i16,
    pub invite_expires_at: String,
}

/// Outcome of accepting an invite.
#[derive(Debug, Clone)]
pub struct AcceptInviteOutcome {
    pub player_id: Uuid,
    pub position: i32,
    pub player_count: i32,
    pub max_players: i32,
    pub game_status: String,
}

/// Information about a player in a benchmark game.
#[derive(Debug, Clone)]
pub struct BenchmarkPlayerOutcome {
    pub player_id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub position: i32,
    pub cards: Vec<i32>,
}

/// Outcome of a benchmark game creation.
#[derive(Debug, Clone)]
pub struct BenchmarkGameOutcome {
    pub game_id: Uuid,
    pub players: Vec<BenchmarkPlayerOutcome>,
    pub current_turn: i32,
    pub bet: i32,
}

/// Counts of records deleted during benchmark cleanup.
#[derive(Debug, Clone)]
pub struct BenchmarkCleanupCounts {
    pub users_deleted: u64,
    pub games_deleted: u64,
    pub game_cards_deleted: u64,
    pub players_deleted: u64,
    pub player_profiles_deleted: u64,
    pub game_invites_deleted: u64,
}

impl From<MultiplayerCreationOutcome> for MultiplayerGameResponse {
    fn from(o: MultiplayerCreationOutcome) -> Self {
        MultiplayerGameResponse {
            game_id: o.game_id,
            status: o.status,
            bet: o.bet,
            max_players: o.max_players,
            invite_expires_at: o.invite_expires_at,
        }
    }
}

impl From<PlayCardOutcome> for PlayCardResponse {
    fn from(o: PlayCardOutcome) -> Self {
        PlayCardResponse {
            success: true,
            message: "Card played successfully".to_string(),
            card_id: o.card_id,
            next_turn: o.next_turn,
            round_completed: o.round_completed,
            game_ended: o.game_ended,
            current_round: o.current_round,
        }
    }
}

impl From<QuickGameOutcome> for QuickGameResponse {
    fn from(o: QuickGameOutcome) -> Self {
        QuickGameResponse {
            game_id: o.game_id,
            players: o.players,
            status: o.status,
            current_turn: o.current_turn,
            bet: o.bet,
            max_players: o.max_players,
            invite_expires_at: o.invite_expires_at,
            deck_slots: o.deck_slots,
        }
    }
}

/// Trait abstracting game orchestration so handlers can be tested
/// with a mock implementation without a database.
#[allow(dead_code)]
#[async_trait]
pub trait GameOrchestratorTrait: Send + Sync + 'static {
    async fn play_card(
        &self,
        game_id: Uuid,
        player_id: Uuid,
        card_index: i32,
        correlation_id: Option<CorrelationId>,
        idempotency_key: Option<String>,
    ) -> Result<PlayCardOutcome, GameError>;

    async fn create_quick_game(
        &self,
        correlation_id: Option<CorrelationId>,
    ) -> Result<QuickGameOutcome, GameError>;

    async fn create_bot_only_game(&self) -> Result<QuickGameOutcome, GameError>;

    async fn create_quick_game_for_user(
        &self,
        user_id: Uuid,
        db: &DatabaseConnection,
    ) -> Result<QuickGameOutcome, GameError>;

    async fn create_multiplayer_game(
        &self,
        user_id: Uuid,
        pseudo: &str,
        bet: i32,
        max_players: i16,
    ) -> Result<MultiplayerCreationOutcome, GameError>;

    async fn create_benchmark_multiplayer_game(
        &self,
        user_ids: Vec<Uuid>,
        bet: i32,
    ) -> Result<BenchmarkGameOutcome, GameError>;

    async fn cleanup_benchmark_data(&self) -> Result<BenchmarkCleanupCounts, GameError>;

    async fn start_game(&self, game_id: Uuid, user_id: Uuid) -> Result<(), GameError>;

    async fn send_invites(
        &self,
        game_id: Uuid,
        creator_user_id: Uuid,
        invited_user_ids: Vec<Uuid>,
    ) -> Result<(), GameError>;

    async fn accept_invite(
        &self,
        game_id: Uuid,
        user_id: Uuid,
        pseudo: &str,
    ) -> Result<AcceptInviteOutcome, GameError>;

    async fn decline_invite(&self, game_id: Uuid, user_id: Uuid) -> Result<(), GameError>;

    #[allow(dead_code)]
    async fn cancel_game(&self, game_id: Uuid) -> Result<(), GameError>;
}
