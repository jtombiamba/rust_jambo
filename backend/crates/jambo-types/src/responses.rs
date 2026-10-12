use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
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

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ApiErrorResponse {
    pub success: bool,
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
}
