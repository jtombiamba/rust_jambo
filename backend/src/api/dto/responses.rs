use serde::Serialize;
use uuid::Uuid;

use crate::game::service::types::{MultiplayerCreationOutcome, PlayCardOutcome, QuickGameOutcome};

#[derive(Debug, Serialize)]
pub struct PlayCardResponse {
    pub success: bool,
    pub message: String,
    pub card_id: Uuid,
    pub next_turn: Option<Uuid>,
    pub round_completed: bool,
    pub game_ended: bool,
    pub current_round: i32,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlayerInfoDto {
    pub id: Uuid,
    #[serde(rename = "type")]
    pub player_type: String,
    pub name: String,
    pub position: i32,
    pub display_position: i32,
    pub cards: Vec<i32>,
    pub cards_count: i32,
    pub is_current_user: bool,
}

#[derive(Debug, Serialize)]
pub struct QuickGameResponse {
    pub game_id: Uuid,
    pub players: Vec<PlayerInfoDto>,
    pub status: String,
    pub current_turn: i32,
    pub bet: i32,
    pub max_players: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invite_expires_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deck_slots: Option<Vec<Option<i32>>>,
    /// One-time WebSocket authentication token for anonymous users.
    /// Present only when the user is not authenticated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ws_token: Option<String>,
    pub step_by_step: bool,
}

#[allow(dead_code)]
#[derive(Debug, Serialize)]
pub struct GameListItem {
    pub id: Uuid,
    pub status: String,
    pub bet: i32,
}

#[derive(Debug, Serialize)]
pub struct MultiplayerGameResponse {
    pub game_id: Uuid,
    pub status: String,
    pub bet: i32,
    pub max_players: i16,
    pub invite_expires_at: String,
}

#[derive(Debug, Serialize)]
pub struct AnonymousStatsResponse {
    pub games_allowed: i32,
    pub games_played: i32,
    pub total_wins: i32,
    pub credits: i32,
}

#[derive(Debug, Serialize)]
pub struct RespondToInviteResponse {
    pub success: bool,
    pub message: String,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub player_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub player_count: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_players: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub game_status: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct InvitationItem {
    pub invite_id: Uuid,
    pub game_id: Uuid,
    pub creator_pseudo: String,
    pub bet: i32,
    pub player_count: i64,
    pub max_players: i32,
    pub created_at: String,
    pub expires_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct InvitationsResponse {
    pub invitations: Vec<InvitationItem>,
}

#[derive(Debug, Serialize)]
pub struct UserSearchItem {
    pub id: Uuid,
    pub pseudo: String,
}

#[derive(Debug, Serialize)]
pub struct UserSearchResponse {
    pub users: Vec<UserSearchItem>,
}

#[derive(Debug, Serialize)]
pub struct UnfreezeOrderResponse {
    pub order_id: String,
    pub approval_url: String,
}

#[derive(Debug, Serialize)]
pub struct UnfreezeCaptureResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct TopupOrderResponse {
    pub order_id: String,
    pub approval_url: String,
}

#[derive(Debug, Serialize)]
pub struct TopupCaptureResponse {
    pub success: bool,
    pub message: String,
    pub credit: i32,
}

#[derive(Debug, Serialize)]
pub struct ApiErrorResponse {
    pub success: bool,
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AdvanceBotResponse {
    pub success: bool,
    pub card_played: i32,
    pub next_player_id: Uuid,
    pub next_is_bot: bool,
    pub round_complete: bool,
    pub game_ended: bool,
}

#[derive(Debug, Serialize)]
pub struct EvaluateRoundResponse {
    pub success: bool,
    pub round_number: i32,
    pub winner_id: Uuid,
    pub winner_position: i32,
    pub game_ended: bool,
}

#[derive(Debug, serde::Deserialize)]
pub struct PlayerActionRequest {
    pub player_id: Uuid,
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
            deck_slots: o.deck_slots.map(|v| v.into_iter().map(Some).collect()),
            ws_token: o.ws_token,
            step_by_step: o.step_by_step,
        }
    }
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
