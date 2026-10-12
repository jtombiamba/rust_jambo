use actix_web::{test, web, App};
use std::sync::Arc;
use uuid::Uuid;

use crate::api::game::play_card;
use crate::auth::config::AuthConfig;
use jambo_core::error::GameError;
use jambo_domain::game::service::mock::MockGameService;
use jambo_domain::game::service::types::{GamePlayService, PlayCardOutcome, QuickGameOutcome};

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

async fn make_play_app(
    mock: Arc<dyn GamePlayService>,
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

fn play_outcome(card_id: Uuid) -> PlayCardOutcome {
    PlayCardOutcome {
        card_id,
        next_turn: Some(Uuid::new_v4()),
        game_ended: false,
        round_completed: false,
        current_round: 1,
    }
}

fn quick_outcome(game_id: Uuid, step_by_step: bool) -> QuickGameOutcome {
    QuickGameOutcome {
        game_id,
        players: vec![],
        status: "active".into(),
        current_turn: 0,
        bet: 10,
        max_players: 4,
        invite_expires_at: None,
        deck_slots: None,
        ws_token: None,
        step_by_step,
    }
}

#[actix_web::test]
async fn play_card_happy_path() {
    let card_id = Uuid::new_v4();
    let mock: Arc<dyn GamePlayService> = Arc::new(MockGameService::new(
        Ok(play_outcome(card_id)),
        Ok(quick_outcome(Uuid::new_v4(), false)),
    ));
    let app = make_play_app(mock).await;

    let game_id = Uuid::new_v4();
    let player_id = Uuid::new_v4();
    let req = test::TestRequest::post()
        .uri(&format!("/game/{}/play", game_id))
        .set_json(serde_json::json!({"player_id": player_id, "card_index": 5}))
        .cookie(auth_cookie())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], true);
    assert_eq!(body["card_id"], card_id.to_string());
    assert_eq!(body["game_ended"], false);
}

#[actix_web::test]
async fn play_card_game_finished_returns_409() {
    let mock: Arc<dyn GamePlayService> = Arc::new(MockGameService::new(
        Err(GameError::GameFinished),
        Ok(quick_outcome(Uuid::new_v4(), false)),
    ));
    let app = make_play_app(mock).await;

    let game_id = Uuid::new_v4();
    let player_id = Uuid::new_v4();
    let req = test::TestRequest::post()
        .uri(&format!("/game/{}/play", game_id))
        .set_json(serde_json::json!({"player_id": player_id, "card_index": 0}))
        .cookie(auth_cookie())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 409);
}

#[actix_web::test]
async fn play_card_insufficient_credits_returns_402() {
    let mock: Arc<dyn GamePlayService> = Arc::new(MockGameService::new(
        Err(GameError::InsufficientCredits {
            required: 10,
            current: 5,
        }),
        Ok(quick_outcome(Uuid::new_v4(), false)),
    ));
    let app = make_play_app(mock).await;

    let game_id = Uuid::new_v4();
    let player_id = Uuid::new_v4();
    let req = test::TestRequest::post()
        .uri(&format!("/game/{}/play", game_id))
        .set_json(serde_json::json!({"player_id": player_id, "card_index": 0}))
        .cookie(auth_cookie())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 402);
}
