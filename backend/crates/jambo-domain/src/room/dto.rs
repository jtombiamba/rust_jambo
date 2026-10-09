use serde::Serialize;
use uuid::Uuid;

use jambo_db::models::RunStatus;

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct StartNextGameResponse {
    pub game_id: Uuid,
    pub game_index: i32,
    pub total_games: i32,
    pub current_game_index: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub all_games_created: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct CurrentGameResponse {
    pub run_id: Uuid,
    pub game_id: Uuid,
    pub game_index: i32,
    pub status: RunStatus,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct CreateRunResponse {
    pub run_id: Uuid,
    pub room_id: Uuid,
    pub num_games: i32,
    pub bet_per_game: i32,
    pub num_players: i32,
    pub status: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct JoinRunResponse {
    pub run_id: Uuid,
    pub provisioned_credits: i32,
    pub profile_credit_remaining: i32,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ActiveRunPlayerInfo {
    pub user_id: Uuid,
    pub pseudo: String,
    pub position: i32,
    pub provisioned_credits: i32,
    pub kicked: bool,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ActiveRunGameInfo {
    pub game_id: Uuid,
    pub game_index: i32,
    pub status: RunStatus,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ActiveRunResponse {
    pub id: Uuid,
    pub room_id: Uuid,
    pub num_games: i32,
    pub bet_per_game: i32,
    pub current_game_index: i32,
    pub status: RunStatus,
    pub players: Vec<ActiveRunPlayerInfo>,
    pub games: Vec<ActiveRunGameInfo>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct RoomListItem {
    pub id: Uuid,
    pub name: String,
    pub creator_id: Uuid,
    pub invitation_code: String,
    pub created_at: String,
    pub member_count: usize,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct RoomMemberInfo {
    pub user_id: Uuid,
    pub pseudo: String,
    pub joined_at: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct RoomGameInfo {
    pub game_id: Uuid,
    pub game_index: i32,
    pub status: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ActiveRunSummary {
    pub id: Uuid,
    pub num_games: i32,
    pub bet_per_game: i32,
    pub current_game_index: i32,
    pub status: String,
    pub all_games_created: bool,
    pub current_game: Option<RoomGameInfo>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct RoomDetailResponse {
    pub id: Uuid,
    pub name: String,
    pub creator_id: Uuid,
    pub invitation_code: String,
    pub created_at: String,
    pub members: Vec<RoomMemberInfo>,
    pub member_count: usize,
    pub active_run: Option<ActiveRunSummary>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct RunListItem {
    pub id: Uuid,
    pub num_games: i32,
    pub bet_per_game: i32,
    pub current_game_index: i32,
    pub status: String,
    pub created_at: String,
}
