use serde::Deserialize;
use uuid::Uuid;

use crate::error::ValidationError;

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct PlayCardRequest {
    pub player_id: Uuid,
    pub card_index: i32,
}

impl PlayCardRequest {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if !(0..32).contains(&self.card_index) {
            return Err(ValidationError::CardIndexOutOfRange(self.card_index));
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ClaimSpecialRequest {
    pub player_id: Uuid,
}

fn default_max_players() -> i16 {
    4
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateGameRequest {
    #[serde(default = "default_bet")]
    pub bet: i32,
    #[serde(default = "default_game_mode")]
    pub game_mode: String,
    #[serde(default = "default_max_players")]
    pub max_players: i16,
    #[serde(default)]
    pub step_by_step: bool,
}

fn default_bet() -> i32 {
    10
}

fn default_game_mode() -> String {
    "solo".to_string()
}

impl CreateGameRequest {
    pub fn validate(&self) -> Result<(), ValidationError> {
        match self.game_mode.as_str() {
            "solo" | "multiplayer" => {}
            _ => {
                return Err(ValidationError::MissingField(
                    "game_mode must be 'solo' or 'multiplayer'".to_string(),
                ))
            }
        }
        if self.game_mode == "multiplayer" {
            if self.bet <= 0 {
                return Err(ValidationError::MissingField(
                    "bet must be positive for multiplayer".to_string(),
                ));
            }
            if !(2..=4).contains(&self.max_players) {
                return Err(ValidationError::MissingField(
                    "max_players must be between 2 and 4".to_string(),
                ));
            }
        }
        if self.bet > 500 {
            return Err(ValidationError::MissingField("bet too large".to_string()));
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct InviteActionQuery {
    pub action: String,
}

impl InviteActionQuery {
    pub fn validate(&self) -> Result<&str, ValidationError> {
        match self.action.as_str() {
            "accept" | "decline" => Ok(self.action.as_str()),
            _ => Err(ValidationError::MissingField(
                "action must be 'accept' or 'decline'".to_string(),
            )),
        }
    }
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct SendInvitesRequest {
    #[serde(default)]
    pub user_ids: Vec<Uuid>,
    #[serde(default)]
    pub pseudos: Vec<String>,
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct UserSearchQuery {
    pub q: String,
    #[serde(default = "default_search_limit")]
    pub limit: u64,
}

fn default_search_limit() -> u64 {
    10
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CaptureOrderRequest {
    pub order_id: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CashoutRequest {
    pub credits: i32,
    pub paypal_email: String,
}
