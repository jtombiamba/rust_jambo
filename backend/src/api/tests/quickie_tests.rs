use crate::api::quickie::create_quick_game;
use crate::auth::config::AuthConfig;
use crate::error::GameError;
use crate::game::service::{
    mock::MockGameService, GameLifecycleService, PlayCardOutcome, QuickGameOutcome,
};
use crate::messaging::RedisClient;
use actix_web::{test, web, App};
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

async fn make_app(
    mock: Arc<dyn GameLifecycleService>,
) -> impl actix_web::dev::Service<
    actix_http::Request,
    Response = actix_web::dev::ServiceResponse,
    Error = actix_web::Error,
> {
    test::init_service(
        App::new()
            .app_data(web::Data::new(mock))
            .app_data(web::Data::new(test_auth_config()))
            .app_data(web::Data::new(None::<RedisClient>))
            .service(create_quick_game),
    )
    .await
}

#[actix_web::test]
async fn create_quick_game_success() {
    let game_id = Uuid::new_v4();
    let mock = Arc::new(MockGameService::new(
        Ok(PlayCardOutcome {
            card_id: Uuid::new_v4(),
            next_turn: Some(Uuid::new_v4()),
            game_ended: false,
            round_completed: false,
            current_round: 1,
        }),
        Ok(QuickGameOutcome {
            game_id,
            players: vec![],
            status: "active".into(),
            current_turn: 2,
            bet: 10,
            max_players: 4,
            invite_expires_at: None,
            deck_slots: None,
            ws_token: None,
            step_by_step: false,
        }),
    ));
    let app = make_app(mock).await;

    let req = test::TestRequest::post().uri("/quickie").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["game_id"], game_id.to_string());
    assert_eq!(body["status"], "active");
    assert_eq!(body["bet"], 10);
}

#[actix_web::test]
async fn create_quick_game_error() {
    let mock = Arc::new(MockGameService::new(
        Ok(PlayCardOutcome {
            card_id: Uuid::new_v4(),
            next_turn: None,
            game_ended: true,
            round_completed: false,
            current_round: 1,
        }),
        Err(GameError::Database(sea_orm::DbErr::Custom(
            "db error".into(),
        ))),
    ));
    let app = make_app(mock).await;

    let req = test::TestRequest::post().uri("/quickie").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 500);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], false);
}

#[actix_web::test]
async fn create_quick_game_with_step_by_step() {
    let game_id = Uuid::new_v4();
    let mock = Arc::new(MockGameService::new(
        Ok(PlayCardOutcome {
            card_id: Uuid::new_v4(),
            next_turn: Some(Uuid::new_v4()),
            game_ended: false,
            round_completed: false,
            current_round: 1,
        }),
        Ok(QuickGameOutcome {
            game_id,
            players: vec![],
            status: "active".into(),
            current_turn: 2,
            bet: 10,
            max_players: 4,
            invite_expires_at: None,
            deck_slots: None,
            ws_token: None,
            step_by_step: true,
        }),
    ));
    let app = make_app(mock).await;
    let req = test::TestRequest::post()
        .uri("/quickie?step_by_step=true")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
}

#[actix_web::test]
async fn create_quick_game_authenticated_skips_token() {
    let game_id = Uuid::new_v4();
    let mock = Arc::new(MockGameService::new(
        Ok(PlayCardOutcome {
            card_id: Uuid::new_v4(),
            next_turn: Some(Uuid::new_v4()),
            game_ended: false,
            round_completed: false,
            current_round: 1,
        }),
        Ok(QuickGameOutcome {
            game_id,
            players: vec![],
            status: "active".into(),
            current_turn: 2,
            bet: 10,
            max_players: 4,
            invite_expires_at: None,
            deck_slots: None,
            ws_token: None,
            step_by_step: false,
        }),
    ));
    let app = make_app(mock).await;
    let token =
        crate::auth::jwt::generate_token(Uuid::new_v4(), "alice", &test_auth_config()).unwrap();
    let req = test::TestRequest::post()
        .uri("/quickie")
        .cookie(actix_web::cookie::Cookie::new("Authorization", token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert!(body["ws_token"].is_null());
}
