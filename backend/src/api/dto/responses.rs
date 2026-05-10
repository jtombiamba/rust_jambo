use serde::Serialize;
use uuid::Uuid;

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
pub struct AcceptInviteResponse {
    pub success: bool,
    pub message: String,
    pub player_id: Uuid,
    pub position: i32,
    pub player_count: i32,
    pub max_players: i32,
    pub game_status: String,
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
