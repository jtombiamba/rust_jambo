use crate::api::game::play_card;
use crate::auth::config::AuthConfig;
use actix_web::{test, web, App};
use jambo_core::error::GameError;
use jambo_domain::game::service::mock::MockGameService;
use jambo_domain::game::service::QuickGameOutcome;
use std::sync::Arc;
use uuid::Uuid;

fn test_auth_config() -> AuthConfig {
    AuthConfig {
        jwt_secret: "test-secret-key-for-testing-only-1234567890".to_string(),
        jwt_expiry_hours: 24,
        ip_hash_pepper: "test-pepper".to_string(),
        frontend_url: "http://localhost:5173".to_string(),
    }
}

fn auth_cookie() -> actix_web::cookie::Cookie<'static> {
    let token =
        crate::auth::jwt::generate_token(Uuid::new_v4(), "alice", &test_auth_config()).unwrap();
    actix_web::cookie::Cookie::new("Authorization", token)
}

async fn make_app(
    mock: Arc<dyn jambo_domain::game::service::GamePlayService>,
) -> impl actix_web::dev::Service<
    actix_http::Request,
    Response = actix_web::dev::ServiceResponse,
    Error = actix_web::Error,
> {
    test::init_service(
        App::new()
            .app_data(web::Data::new(mock))
            .app_data(web::Data::new(test_auth_config()))
            .service(play_card),
    )
    .await
}

fn authed_post(uri: &str, json: serde_json::Value) -> actix_http::Request {
    test::TestRequest::post()
        .uri(uri)
        .set_json(json)
        .cookie(auth_cookie())
        .to_request()
}

// ── validation tests ──

#[actix_web::test]
async fn play_card_valid_payload() {
    let mock = Arc::new(MockGameService::ok());
    let app = make_app(mock).await;
    let game_id = Uuid::new_v4();
    let player_id = Uuid::new_v4();

    let req = authed_post(
        &format!("/game/{}/play", game_id),
        serde_json::json!({ "player_id": player_id, "card_index": 15 }),
    );
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], true);
    assert_eq!(body["message"], "Card played successfully");
}

#[actix_web::test]
async fn play_card_negative_index() {
    let mock = Arc::new(MockGameService::ok());
    let app = make_app(mock).await;
    let game_id = Uuid::new_v4();
    let player_id = Uuid::new_v4();

    let req = authed_post(
        &format!("/game/{}/play", game_id),
        serde_json::json!({ "player_id": player_id, "card_index": -1 }),
    );
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], false);
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("out of valid range"));
}

#[actix_web::test]
async fn play_card_index_out_of_range() {
    let mock = Arc::new(MockGameService::ok());
    let app = make_app(mock).await;
    let game_id = Uuid::new_v4();
    let player_id = Uuid::new_v4();

    let req = authed_post(
        &format!("/game/{}/play", game_id),
        serde_json::json!({ "player_id": player_id, "card_index": 32 }),
    );
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], false);
}

#[actix_web::test]
async fn play_card_missing_player_id() {
    let mock = Arc::new(MockGameService::ok());
    let app = make_app(mock).await;
    let game_id = Uuid::new_v4();

    let req = authed_post(
        &format!("/game/{}/play", game_id),
        serde_json::json!({ "card_index": 0 }),
    );
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
}

#[actix_web::test]
async fn play_card_missing_card_index() {
    let mock = Arc::new(MockGameService::ok());
    let app = make_app(mock).await;
    let game_id = Uuid::new_v4();
    let player_id = Uuid::new_v4();

    let req = authed_post(
        &format!("/game/{}/play", game_id),
        serde_json::json!({ "player_id": player_id }),
    );
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
}

#[actix_web::test]
async fn play_card_invalid_player_id_uuid() {
    let mock = Arc::new(MockGameService::ok());
    let app = make_app(mock).await;
    let game_id = Uuid::new_v4();

    let req = authed_post(
        &format!("/game/{}/play", game_id),
        serde_json::json!({ "player_id": "not-a-uuid", "card_index": 0 }),
    );
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
}

#[actix_web::test]
async fn play_card_non_uuid_path() {
    let mock = Arc::new(MockGameService::ok());
    let app = make_app(mock).await;
    let player_id = Uuid::new_v4();

    let req = test::TestRequest::post()
        .uri("/game/not-a-uuid/play")
        .set_json(serde_json::json!({ "player_id": player_id, "card_index": 0 }))
        .cookie(auth_cookie())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);
}

// ── error mapping tests ──

#[actix_web::test]
async fn play_card_game_not_found() {
    let mock = Arc::new(MockGameService::new(
        Err(GameError::GameNotFound),
        Ok(QuickGameOutcome {
            game_id: Uuid::new_v4(),
            players: vec![],
            status: "active".into(),
            current_turn: 0,
            bet: 10,
            max_players: 4,
            invite_expires_at: None,
            deck_slots: None,
            ws_token: None,
            step_by_step: false,
        }),
    ));
    let app = make_app(mock).await;
    let game_id = Uuid::new_v4();
    let player_id = Uuid::new_v4();
    let req = authed_post(
        &format!("/game/{}/play", game_id),
        serde_json::json!({ "player_id": player_id, "card_index": 0 }),
    );
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);
}

#[actix_web::test]
async fn play_card_player_not_found() {
    let mock = Arc::new(MockGameService::new(
        Err(GameError::PlayerNotFound),
        Ok(QuickGameOutcome {
            game_id: Uuid::new_v4(),
            players: vec![],
            status: "active".into(),
            current_turn: 0,
            bet: 10,
            max_players: 4,
            invite_expires_at: None,
            deck_slots: None,
            ws_token: None,
            step_by_step: false,
        }),
    ));
    let app = make_app(mock).await;
    let game_id = Uuid::new_v4();
    let player_id = Uuid::new_v4();
    let req = authed_post(
        &format!("/game/{}/play", game_id),
        serde_json::json!({ "player_id": player_id, "card_index": 0 }),
    );
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);
}

#[actix_web::test]
async fn play_card_card_not_found() {
    let mock = Arc::new(MockGameService::new(
        Err(GameError::CardNotFound),
        Ok(QuickGameOutcome {
            game_id: Uuid::new_v4(),
            players: vec![],
            status: "active".into(),
            current_turn: 0,
            bet: 10,
            max_players: 4,
            invite_expires_at: None,
            deck_slots: None,
            ws_token: None,
            step_by_step: false,
        }),
    ));
    let app = make_app(mock).await;
    let game_id = Uuid::new_v4();
    let player_id = Uuid::new_v4();
    let req = authed_post(
        &format!("/game/{}/play", game_id),
        serde_json::json!({ "player_id": player_id, "card_index": 0 }),
    );
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);
}

#[actix_web::test]
async fn play_card_not_your_turn() {
    let mock = Arc::new(MockGameService::new(
        Err(GameError::NotYourTurn),
        Ok(QuickGameOutcome {
            game_id: Uuid::new_v4(),
            players: vec![],
            status: "active".into(),
            current_turn: 0,
            bet: 10,
            max_players: 4,
            invite_expires_at: None,
            deck_slots: None,
            ws_token: None,
            step_by_step: false,
        }),
    ));
    let app = make_app(mock).await;
    let game_id = Uuid::new_v4();
    let player_id = Uuid::new_v4();
    let req = authed_post(
        &format!("/game/{}/play", game_id),
        serde_json::json!({ "player_id": player_id, "card_index": 0 }),
    );
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);
}

#[actix_web::test]
async fn play_card_invalid_card() {
    let mock = Arc::new(MockGameService::new(
        Err(GameError::InvalidCard),
        Ok(QuickGameOutcome {
            game_id: Uuid::new_v4(),
            players: vec![],
            status: "active".into(),
            current_turn: 0,
            bet: 10,
            max_players: 4,
            invite_expires_at: None,
            deck_slots: None,
            ws_token: None,
            step_by_step: false,
        }),
    ));
    let app = make_app(mock).await;
    let game_id = Uuid::new_v4();
    let player_id = Uuid::new_v4();
    let req = authed_post(
        &format!("/game/{}/play", game_id),
        serde_json::json!({ "player_id": player_id, "card_index": 0 }),
    );
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);
}

#[actix_web::test]
async fn play_card_game_finished() {
    let mock = Arc::new(MockGameService::new(
        Err(GameError::GameFinished),
        Ok(QuickGameOutcome {
            game_id: Uuid::new_v4(),
            players: vec![],
            status: "active".into(),
            current_turn: 0,
            bet: 10,
            max_players: 4,
            invite_expires_at: None,
            deck_slots: None,
            ws_token: None,
            step_by_step: false,
        }),
    ));
    let app = make_app(mock).await;
    let game_id = Uuid::new_v4();
    let player_id = Uuid::new_v4();
    let req = authed_post(
        &format!("/game/{}/play", game_id),
        serde_json::json!({ "player_id": player_id, "card_index": 0 }),
    );
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 409);
}

#[actix_web::test]
async fn play_card_round_not_complete() {
    let mock = Arc::new(MockGameService::new(
        Err(GameError::RoundNotComplete),
        Ok(QuickGameOutcome {
            game_id: Uuid::new_v4(),
            players: vec![],
            status: "active".into(),
            current_turn: 0,
            bet: 10,
            max_players: 4,
            invite_expires_at: None,
            deck_slots: None,
            ws_token: None,
            step_by_step: false,
        }),
    ));
    let app = make_app(mock).await;
    let game_id = Uuid::new_v4();
    let player_id = Uuid::new_v4();
    let req = authed_post(
        &format!("/game/{}/play", game_id),
        serde_json::json!({ "player_id": player_id, "card_index": 0 }),
    );
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
}

#[actix_web::test]
async fn play_card_database_error() {
    let mock = Arc::new(MockGameService::new(
        Err(GameError::Database(sea_orm::DbErr::Custom(
            "db down".into(),
        ))),
        Ok(QuickGameOutcome {
            game_id: Uuid::new_v4(),
            players: vec![],
            status: "active".into(),
            current_turn: 0,
            bet: 10,
            max_players: 4,
            invite_expires_at: None,
            deck_slots: None,
            ws_token: None,
            step_by_step: false,
        }),
    ));
    let app = make_app(mock).await;
    let game_id = Uuid::new_v4();
    let player_id = Uuid::new_v4();
    let req = authed_post(
        &format!("/game/{}/play", game_id),
        serde_json::json!({ "player_id": player_id, "card_index": 0 }),
    );
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 500);
}

#[actix_web::test]
async fn play_card_internal_error() {
    let mock = Arc::new(MockGameService::new(
        Err(GameError::internal("internal kaboom")),
        Ok(QuickGameOutcome {
            game_id: Uuid::new_v4(),
            players: vec![],
            status: "active".into(),
            current_turn: 0,
            bet: 10,
            max_players: 4,
            invite_expires_at: None,
            deck_slots: None,
            ws_token: None,
            step_by_step: false,
        }),
    ));
    let app = make_app(mock).await;
    let game_id = Uuid::new_v4();
    let player_id = Uuid::new_v4();
    let req = authed_post(
        &format!("/game/{}/play", game_id),
        serde_json::json!({ "player_id": player_id, "card_index": 0 }),
    );
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 500);
}

// ── authorization tests ──

#[actix_web::test]
async fn play_card_unauthenticated_returns_403() {
    let mock = Arc::new(MockGameService::ok());
    let app = make_app(mock).await;
    let game_id = Uuid::new_v4();
    let player_id = Uuid::new_v4();

    // No cookie and no game token: the request must be rejected outright.
    let req = test::TestRequest::post()
        .uri(&format!("/game/{}/play", game_id))
        .set_json(serde_json::json!({ "player_id": player_id, "card_index": 0 }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);
}

#[actix_web::test]
async fn play_card_wrong_owner_returns_403() {
    let mock = Arc::new(MockGameService::ok());
    mock.set_verify_ownership_result(Ok(false));
    let app = make_app(mock).await;
    let game_id = Uuid::new_v4();
    let player_id = Uuid::new_v4();

    let req = authed_post(
        &format!("/game/{}/play", game_id),
        serde_json::json!({ "player_id": player_id, "card_index": 0 }),
    );
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);
}
