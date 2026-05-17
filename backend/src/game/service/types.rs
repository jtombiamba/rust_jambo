use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::time::Instant;
use thiserror::Error;
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

#[allow(dead_code)]
#[derive(Error, Debug)]
pub enum GameServiceError {
    #[error("Database error: {0}")]
    Database(#[from] Box<dyn std::error::Error + Send>),
    #[error("Game not found")]
    GameNotFound,
    #[error("Player not found")]
    PlayerNotFound,
    #[error("Card not found or already played")]
    CardNotFound,
    #[error("Not your turn to play")]
    NotYourTurn,
    #[error("Invalid card play: you must follow suit if possible")]
    InvalidCard,
    #[error("Round not complete")]
    RoundNotComplete,
    #[error("Game already finished")]
    GameFinished,
    #[error("Insufficient credits for bet")]
    InsufficientCredits,
    #[error("Game is not pending")]
    GameNotPending,
    #[error("User is not the game creator")]
    NotCreator,
    #[error("User is not invited to this game")]
    NotInvited,
    #[error("User is already a player in this game")]
    AlreadyJoined,
    #[error("Game is full")]
    GameFull,
    #[error("Invite has expired")]
    InviteExpired,
    #[error("Creator cannot join their own game")]
    CreatorCannotJoin,
    #[error("Game is not in ready state")]
    GameNotReady,
    #[error("Duplicate player: user is already a player in this game")]
    DuplicatePlayer,
    #[error("Optimistic lock conflict: game state was modified concurrently")]
    VersionConflict,
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Rich result returned by `update_card_play` after a successful card play.
pub struct CardPlayResult {
    pub card: game_card::Model,
    pub next_player_id: Uuid,
    pub players: Vec<player::Model>,
    pub game_ended: bool,
    pub round_completed: bool,
    pub current_round: i32,
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
pub(crate) struct RoundEvaluationResult {
    pub(crate) round: i32,
    pub(crate) winner_id: Uuid,
    pub(crate) winner_position: usize,
    pub(crate) game_ended: bool,
    pub(crate) final_status: GameStatus,
    pub(crate) players: Vec<PlayerModel>,
}
