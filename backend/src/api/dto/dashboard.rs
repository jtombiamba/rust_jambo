use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct PlayerProfileResponse {
    pub credit: i32,
    pub game_played: i32,
    pub wins: i32,
    pub kora_wins: i32,
}

#[derive(Debug, Serialize)]
pub struct GameHistoryItem {
    pub game_id: String,
    pub status: String,
    pub bet: i32,
    pub result: String,
    pub played_at: String,
    pub credits_after: i32,
}

#[derive(Debug, Serialize)]
pub struct GameHistoryResponse {
    pub games: Vec<GameHistoryItem>,
    pub total: u64,
    pub page: u64,
    pub per_page: u64,
}

#[derive(Debug, Deserialize)]
pub struct PaginationParams {
    pub page: Option<u64>,
    pub per_page: Option<u64>,
}
