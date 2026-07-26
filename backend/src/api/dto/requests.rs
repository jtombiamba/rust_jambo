use serde::Deserialize;
use uuid::Uuid;

use crate::error::ValidationError;

#[derive(Debug, Deserialize)]
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

fn default_max_players() -> i16 {
    4
}

#[derive(Debug, Deserialize)]
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

#[derive(Debug, Deserialize)]
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

#[derive(Debug, Deserialize)]
pub struct SendInvitesRequest {
    #[serde(default)]
    pub user_ids: Vec<Uuid>,
    #[serde(default)]
    pub pseudos: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct UserSearchQuery {
    pub q: String,
    #[serde(default = "default_search_limit")]
    pub limit: u64,
}

fn default_search_limit() -> u64 {
    10
}

#[derive(Debug, Deserialize)]
pub struct CaptureOrderRequest {
    pub order_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_action_accept() {
        let query = InviteActionQuery {
            action: "accept".to_string(),
        };
        assert_eq!(query.validate().unwrap(), "accept");
    }

    #[test]
    fn test_validate_action_decline() {
        let query = InviteActionQuery {
            action: "decline".to_string(),
        };
        assert_eq!(query.validate().unwrap(), "decline");
    }

    #[test]
    fn test_validate_action_invalid() {
        let query = InviteActionQuery {
            action: "foo".to_string(),
        };
        let err = query.validate().unwrap_err();
        assert!(matches!(err, ValidationError::MissingField(_)));
    }

    #[test]
    fn test_create_game_request_defaults() {
        let req = CreateGameRequest {
            bet: 0, // Uses default_bet=10
            game_mode: "solo".to_string(),
            max_players: 4,
            step_by_step: false,
        };
        assert!(req.validate().is_ok());
    }

    #[test]
    fn test_create_game_request_multiplier_valid() {
        let req = CreateGameRequest {
            bet: 10,
            game_mode: "multiplayer".to_string(),
            max_players: 2,
            step_by_step: false,
        };
        assert!(req.validate().is_ok());
    }

    #[test]
    fn test_create_game_request_bet_too_large() {
        let req = CreateGameRequest {
            bet: 600,
            game_mode: "solo".to_string(),
            max_players: 4,
            step_by_step: false,
        };
        assert!(req.validate().is_err());
    }

    #[test]
    fn test_create_game_request_multiplayer_bet_zero() {
        let req = CreateGameRequest {
            bet: 0,
            game_mode: "multiplayer".to_string(),
            max_players: 4,
            step_by_step: false,
        };
        assert!(req.validate().is_err());
    }

    #[test]
    fn test_create_game_request_invalid_max_players() {
        let req = CreateGameRequest {
            bet: 10,
            game_mode: "multiplayer".to_string(),
            max_players: 5,
            step_by_step: false,
        };
        assert!(req.validate().is_err());
    }

    #[test]
    fn test_create_game_request_multiplayer_min_players() {
        let req = CreateGameRequest {
            bet: 10,
            game_mode: "multiplayer".to_string(),
            max_players: 1,
            step_by_step: false,
        };
        assert!(req.validate().is_err());
    }

    #[test]
    fn test_play_card_request_validate_edge() {
        assert!(PlayCardRequest {
            player_id: Uuid::new_v4(),
            card_index: 0,
        }
        .validate()
        .is_ok());
        assert!(PlayCardRequest {
            player_id: Uuid::new_v4(),
            card_index: 31,
        }
        .validate()
        .is_ok());
        assert!(PlayCardRequest {
            player_id: Uuid::new_v4(),
            card_index: -5,
        }
        .validate()
        .is_err());
    }

    #[test]
    fn test_user_search_query_defaults() {
        let q: UserSearchQuery = serde_json::from_str(r#"{"q":"test"}"#).unwrap();
        assert_eq!(q.q, "test");
        assert_eq!(q.limit, 10);
    }

    #[test]
    fn test_validate_action_empty() {
        let query = InviteActionQuery {
            action: String::new(),
        };
        let err = query.validate().unwrap_err();
        assert!(matches!(err, ValidationError::MissingField(_)));
    }
}
