use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::responses::PlayerInfoDto;

#[derive(Debug, Serialize)]
pub struct QuickGameOutcome {
    pub game_id: Uuid,
    pub players: Vec<PlayerInfoDto>,
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

#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ClaimSpecialOutcome {
    pub success: bool,
    pub winner_id: Uuid,
    pub winner_position: i32,
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
