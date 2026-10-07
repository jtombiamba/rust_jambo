use crate::api::dto::requests::{
    CreateGameRequest, InviteActionQuery, PlayCardRequest, UserSearchQuery,
};
use crate::error::ValidationError;
use uuid::Uuid;

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
