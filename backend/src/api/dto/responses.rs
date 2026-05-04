use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub struct PlayCardResponse {
    pub success: bool,
    pub message: String,
    pub card_id: Uuid,
    pub next_turn: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlayerInfoDto {
    pub id: Uuid,
    #[serde(rename = "type")]
    pub player_type: String,
    pub name: String,
    pub position: i32,
    pub cards: Vec<i32>,
    pub cards_count: i32,
}

#[derive(Debug, Serialize)]
pub struct QuickGameResponse {
    pub game_id: Uuid,
    pub players: Vec<PlayerInfoDto>,
    pub status: String,
    pub current_turn: i32,
    pub bet: i32,
}

#[derive(Debug, Serialize)]
pub struct GameListItem {
    pub id: Uuid,
    pub status: String,
    pub bet: i32,
}

#[derive(Debug, Serialize)]
pub struct AnonymousStatsResponse {
    pub games_allowed: i32,
    pub games_played: i32,
    pub total_wins: i32,
    pub credits: i32,
}
