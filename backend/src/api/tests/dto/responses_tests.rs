use crate::api::dto::responses::{
    MultiplayerGameResponse, PlayCardResponse, PlayerInfoDto, QuickGameResponse,
};
use crate::game::service::types::{MultiplayerCreationOutcome, PlayCardOutcome, QuickGameOutcome};
use uuid::Uuid;

#[test]
fn player_info_dto_renames_type_field() {
    let dto = PlayerInfoDto {
        id: Uuid::now_v7(),
        player_type: "human".into(),
        name: "alice".into(),
        position: 0,
        display_position: 0,
        cards: vec![1, 2],
        cards_count: 2,
        is_current_user: true,
    };
    let json = serde_json::to_value(&dto).unwrap();
    assert_eq!(json["type"], "human");
    assert!(json.get("player_type").is_none());
}

#[test]
fn quick_game_response_skips_optional_fields() {
    let response = QuickGameResponse {
        game_id: Uuid::now_v7(),
        players: vec![],
        status: "active".into(),
        current_turn: 0,
        bet: 10,
        max_players: 4,
        invite_expires_at: None,
        deck_slots: None,
        ws_token: None,
        step_by_step: false,
        special_cards: None,
    };
    let json = serde_json::to_value(&response).unwrap();
    assert!(json.get("invite_expires_at").is_none());
    assert!(json.get("deck_slots").is_none());
    assert!(json.get("ws_token").is_none());
    assert!(json.get("special_cards").is_none());
}

#[test]
fn play_card_response_serializes_none_next_turn_as_null() {
    let response = PlayCardResponse {
        success: true,
        message: "ok".into(),
        card_id: Uuid::now_v7(),
        next_turn: None,
        round_completed: false,
        game_ended: false,
        current_round: 1,
    };
    let json = serde_json::to_value(&response).unwrap();
    assert!(json["next_turn"].is_null());
}

#[test]
fn play_card_response_from_outcome() {
    let card_id = Uuid::now_v7();
    let next = Uuid::now_v7();
    let response: PlayCardResponse = PlayCardOutcome {
        card_id,
        next_turn: Some(next),
        game_ended: false,
        round_completed: true,
        current_round: 3,
    }
    .into();
    assert!(response.success);
    assert_eq!(response.card_id, card_id);
    assert_eq!(response.next_turn, Some(next));
    assert_eq!(response.current_round, 3);
}

#[test]
fn quick_game_response_from_outcome_maps_deck_slots() {
    let game_id = Uuid::now_v7();
    let response: QuickGameResponse = QuickGameOutcome {
        game_id,
        players: vec![],
        status: "active".into(),
        current_turn: 1,
        bet: 10,
        max_players: 4,
        invite_expires_at: None,
        deck_slots: Some(vec![3, 5]),
        ws_token: Some("token".into()),
        step_by_step: true,
    }
    .into();
    assert_eq!(response.game_id, game_id);
    assert_eq!(response.deck_slots, Some(vec![Some(3), Some(5)]));
    assert_eq!(response.ws_token, Some("token".into()));
    assert!(response.step_by_step);
}

#[test]
fn multiplayer_game_response_from_outcome() {
    let game_id = Uuid::now_v7();
    let response: MultiplayerGameResponse = MultiplayerCreationOutcome {
        game_id,
        status: "pending".into(),
        bet: 20,
        max_players: 3,
        invite_expires_at: "2026-01-01T00:00:00+00:00".into(),
    }
    .into();
    assert_eq!(response.game_id, game_id);
    assert_eq!(response.bet, 20);
    assert_eq!(response.max_players, 3);
}
