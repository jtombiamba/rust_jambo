use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct PlayerProfileResponse {
    pub credit: i32,
    pub game_played: i32,
    pub wins: i32,
    pub kora_wins: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frozen_until: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GameHistoryItem {
    pub game_id: String,
    pub status: String,
    pub bet: i32,
    pub result: String,
    pub played_at: String,
    pub player_count: i32,
}

#[derive(Debug, Serialize, Deserialize)]
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
    pub status: Option<String>,
    pub order_by: Option<String>,
    pub bet_min: Option<i32>,
    pub bet_max: Option<i32>,
}

#[derive(Debug, Clone)]
pub struct GameFilter {
    pub statuses: Vec<String>,
    pub order_by: String,
    pub bet_min: Option<i32>,
    pub bet_max: Option<i32>,
}

impl PaginationParams {
    pub fn to_filter(&self) -> GameFilter {
        let statuses = self
            .status
            .as_ref()
            .map(|s| s.split(',').map(|x| x.trim().to_lowercase()).collect())
            .unwrap_or_default();

        let order_by = self.order_by.as_deref().unwrap_or("date_desc").to_string();

        GameFilter {
            statuses,
            order_by,
            bet_min: self.bet_min,
            bet_max: self.bet_max,
        }
    }
}
