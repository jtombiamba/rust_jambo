use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::time::Instant;
use uuid::Uuid;

use crate::database::models::{game_card, player, GameStatus, Player as PlayerModel};
use crate::observability::metrics::{
    CARD_PLAY_DURATION_SECONDS, GAME_CREATION_DURATION_SECONDS, ROUND_EVAL_DURATION_SECONDS,
};

pub(crate) const GAME_STATE_CACHE_TTL_SECS: u64 = 5 * 60;

pub(crate) struct CardPlayTimer(pub(crate) Instant);
impl Drop for CardPlayTimer {
    fn drop(&mut self) {
        CARD_PLAY_DURATION_SECONDS
            .with_label_values(&["update_card_play"])
            .observe(self.0.elapsed().as_secs_f64());
    }
}

pub(crate) struct RoundEvalTimer(pub(crate) Instant);
impl Drop for RoundEvalTimer {
    fn drop(&mut self) {
        ROUND_EVAL_DURATION_SECONDS
            .with_label_values(&[])
            .observe(self.0.elapsed().as_secs_f64());
    }
}

pub(crate) struct GameCreationTimer {
    pub(crate) start: Instant,
    pub(crate) label: &'static str,
}
impl GameCreationTimer {
    pub(crate) fn new(label: &'static str) -> Self {
        Self {
            start: Instant::now(),
            label,
        }
    }
}
impl Drop for GameCreationTimer {
    fn drop(&mut self) {
        let duration = self.start.elapsed().as_secs_f64();
        GAME_CREATION_DURATION_SECONDS
            .with_label_values(&[self.label])
            .observe(duration);
    }
}

/// Serializable snapshot of an active game for caching in Redis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CachedGameState {
    pub(crate) status: String,
    pub(crate) roll: i32,
    pub(crate) rank: Option<i32>,
    pub(crate) bet: i32,
    pub(crate) current_winning_card: Option<i32>,
    pub(crate) current_winning_player_position: Option<i32>,
    pub(crate) players: Vec<CachedPlayer>,
    pub(crate) cards: Vec<CachedCard>,
    pub(crate) round_completed: bool,
    pub(crate) next_player_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CachedPlayer {
    pub(crate) id: Uuid,
    pub(crate) name: String,
    pub(crate) position: i32,
    pub(crate) player_type: String,
    pub(crate) credits: i32,
    pub(crate) user_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CachedCard {
    pub(crate) player_id: Option<Uuid>,
    pub(crate) card_index: i32,
    pub(crate) played: bool,
    pub(crate) round: Option<i32>,
}

/// Rich result returned by `update_card_play` after a successful card play.
pub struct CardPlayResult {
    pub card: game_card::Model,
    pub next_player_id: Uuid,
    pub players: Vec<player::Model>,
    pub game_ended: bool,
    pub round_completed: bool,
    pub current_round: i32,
    pub step_by_step: bool,
}

pub struct BotMoveOutcome {
    pub card_played: i32,
    pub next_player_id: Uuid,
    pub round_complete: bool,
    pub game_ended: bool,
    pub players: Vec<player::Model>,
}

/// Result of creating a multiplayer game.
#[allow(dead_code)]
pub struct MultiplayerGameOutcome {
    pub game_id: Uuid,
    pub player_id: Uuid,
    pub status: GameStatus,
    pub bet: i32,
    pub max_players: i16,
    pub invite_expires_at: chrono::DateTime<Utc>,
}

/// Result of evaluating a completed round inside a transaction.
pub struct RoundEvaluationResult {
    pub(crate) round: i32,
    pub(crate) winner_id: Uuid,
    pub(crate) winner_position: usize,
    pub(crate) game_ended: bool,
    pub(crate) final_status: GameStatus,
    pub(crate) players: Vec<PlayerModel>,
}

#[derive(Debug, Serialize)]
pub struct QuickGameOutcome {
    pub game_id: Uuid,
    pub players: Vec<crate::api::dto::responses::PlayerInfoDto>,
    pub status: String,
    pub current_turn: i32,
    pub bet: i32,
    pub max_players: i32,
    pub invite_expires_at: Option<String>,
    pub deck_slots: Option<Vec<i32>>,
    pub ws_token: Option<String>,
    pub step_by_step: bool,
}

pub struct AcceptInviteOutcome {
    pub player_id: Uuid,
    pub position: i32,
    pub player_count: i32,
    pub max_players: i32,
    pub game_status: String,
}

pub struct AdvanceBotOutcome {
    pub card_played: i32,
    pub next_player_id: Uuid,
    pub next_is_bot: bool,
    pub round_complete: bool,
    pub game_ended: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PlayCardOutcome {
    pub card_id: Uuid,
    pub next_turn: Option<Uuid>,
    pub game_ended: bool,
    pub round_completed: bool,
    pub current_round: i32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EvaluateRoundOutcome {
    pub round_number: i32,
    pub winner_id: Option<Uuid>,
    pub winner_position: i32,
    pub game_ended: bool,
}

pub struct MultiplayerCreationOutcome {
    pub game_id: Uuid,
    pub status: String,
    pub bet: i32,
    pub max_players: i16,
    pub invite_expires_at: String,
}

pub struct BenchmarkGameOutcome {
    pub game_id: Uuid,
    pub players: Vec<BenchmarkPlayerOutcome>,
    pub current_turn: i32,
    pub bet: i32,
}

#[derive(Debug)]
pub struct BenchmarkPlayerOutcome {
    pub player_id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub position: i32,
    pub cards: Vec<i32>,
}

pub struct BenchmarkCleanupCounts {
    pub users_deleted: u64,
    pub games_deleted: u64,
    pub game_cards_deleted: u64,
    pub players_deleted: u64,
    pub player_profiles_deleted: u64,
    pub game_invites_deleted: u64,
}

/// Core gameplay operations: playing cards, advancing bots, evaluating rounds.
#[async_trait::async_trait]
#[allow(unused_variables)]
pub trait GamePlayService: Send + Sync {
    async fn play_card(
        &self,
        game_id: Uuid,
        player_id: Uuid,
        card_index: i32,
        correlation_id: Option<crate::observability::CorrelationId>,
        idempotency_key: Option<String>,
    ) -> Result<PlayCardOutcome, crate::error::GameError>;

    async fn advance_bot(
        &self,
        game_id: Uuid,
        human_player_id: Uuid,
    ) -> Result<AdvanceBotOutcome, crate::error::GameError>;

    async fn evaluate_round(
        &self,
        game_id: Uuid,
        human_player_id: Uuid,
        idempotency_key: Option<String>,
    ) -> Result<EvaluateRoundOutcome, crate::error::GameError>;

    async fn verify_player_ownership(
        &self,
        game_id: Uuid,
        player_id: Uuid,
        user_id: Uuid,
    ) -> Result<bool, crate::error::GameError>;
}

/// Game invitation management: sending, accepting, and declining invites.
#[async_trait::async_trait]
#[allow(unused_variables)]
pub trait InviteService: Send + Sync {
    async fn send_invites(
        &self,
        game_id: Uuid,
        creator_user_id: Uuid,
        invited_user_ids: Vec<Uuid>,
    ) -> Result<(), crate::error::GameError>;

    async fn accept_invite(
        &self,
        game_id: Uuid,
        user_id: Uuid,
        pseudo: &str,
    ) -> Result<AcceptInviteOutcome, crate::error::GameError>;

    async fn decline_invite(
        &self,
        game_id: Uuid,
        user_id: Uuid,
    ) -> Result<(), crate::error::GameError>;
}

/// Game lifecycle operations: creation, start, and cancellation.
#[async_trait::async_trait]
#[allow(unused_variables)]
pub trait GameLifecycleService: Send + Sync {
    async fn create_quick_game(
        &self,
        correlation_id: Option<crate::observability::CorrelationId>,
        step_by_step: bool,
    ) -> Result<QuickGameOutcome, crate::error::GameError>;

    #[allow(dead_code)]
    async fn create_bot_only_game(&self) -> Result<QuickGameOutcome, crate::error::GameError>;

    async fn create_quick_game_for_user_with_step_by_step(
        &self,
        user_id: Uuid,
        db: &sea_orm::DatabaseConnection,
        step_by_step: bool,
    ) -> Result<QuickGameOutcome, crate::error::GameError>;

    async fn create_multiplayer_game(
        &self,
        user_id: Uuid,
        pseudo: &str,
        bet: i32,
        max_players: i16,
    ) -> Result<MultiplayerCreationOutcome, crate::error::GameError>;

    async fn start_game(&self, game_id: Uuid, user_id: Uuid)
        -> Result<(), crate::error::GameError>;

    #[allow(dead_code)]
    async fn cancel_game(&self, game_id: Uuid) -> Result<(), crate::error::GameError>;
}

/// Benchmark-specific operations for load testing and benchmarking.
#[async_trait::async_trait]
#[allow(unused_variables)]
pub trait BenchmarkService: Send + Sync {
    async fn create_benchmark_multiplayer_game(
        &self,
        user_ids: Vec<Uuid>,
        bet: i32,
    ) -> Result<BenchmarkGameOutcome, crate::error::GameError>;

    async fn cleanup_benchmark_data(
        &self,
    ) -> Result<BenchmarkCleanupCounts, crate::error::GameError>;
}

/// Backward-compatible supertrait combining all game service concerns.
/// Implemented by any type that implements all four sub-traits.
pub trait GameServiceTrait:
    GamePlayService + InviteService + GameLifecycleService + BenchmarkService
{
}
