use super::*;
use crate::auth::extractors::AuthenticatedUser;
use crate::error::GameError;
use crate::game::service::{
    mock::MockGameService, AcceptInviteOutcome, GameLifecycleService, InviteService,
};
use crate::i18n::Translator;
use actix_web::{test, web, App};
use sea_orm::{DatabaseBackend, MockDatabase};
use std::sync::Arc;
use uuid::Uuid;

async fn make_respond_app(
    mock: Arc<dyn InviteService>,
) -> impl actix_web::dev::Service<
    actix_http::Request,
    Response = actix_web::dev::ServiceResponse,
    Error = actix_web::Error,
> {
    test::init_service(
        App::new()
            .app_data(web::Data::new(mock))
            .app_data(web::Data::new(Arc::new(Translator::new())))
            .route("/{game_id}/respond", web::post().to(respond_to_invite)),
    )
    .await
}

fn authenticated_user() -> AuthenticatedUser {
    AuthenticatedUser {
        user_id: Uuid::new_v4(),
        pseudo: "TestPlayer".to_string(),
    }
}

fn accept_outcome() -> AcceptInviteOutcome {
    AcceptInviteOutcome {
        player_id: Uuid::new_v4(),
        position: 1,
        player_count: 2,
        max_players: 4,
        game_status: "pending".to_string(),
    }
}

#[actix_web::test]
async fn respond_accept_success() {
    let outcome = accept_outcome();
    let mock = Arc::new(MockGameService::ok());
    mock.set_accept_invite_result(Ok(outcome));
    let app = make_respond_app(mock).await;
    let user = authenticated_user();
    let game_id = Uuid::new_v4();

    let req = test::TestRequest::post()
        .uri(&format!("/{game_id}/respond?action=accept"))
        .to_request();
    req.extensions_mut().insert(user);

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], true);
    assert_eq!(body["action"], "accept");
    assert_eq!(body["message"], "Joined game successfully");
    assert!(body["player_id"].is_string());
    assert_eq!(body["position"], 1);
    assert_eq!(body["player_count"], 2);
    assert_eq!(body["max_players"], 4);
    assert_eq!(body["game_status"], "pending");
}

#[actix_web::test]
async fn respond_accept_game_ready() {
    let mut outcome = accept_outcome();
    outcome.game_status = "ready".to_string();
    let mock = Arc::new(MockGameService::ok());
    mock.set_accept_invite_result(Ok(outcome));
    let app = make_respond_app(mock).await;
    let user = authenticated_user();
    let game_id = Uuid::new_v4();

    let req = test::TestRequest::post()
        .uri(&format!("/{game_id}/respond?action=accept"))
        .to_request();
    req.extensions_mut().insert(user);

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], true);
    assert_eq!(body["message"], "Game is ready to start!");
}

#[actix_web::test]
async fn respond_accept_not_invited() {
    let mock = Arc::new(MockGameService::ok());
    mock.set_accept_invite_result(Err(GameError::NotInvited));
    let app = make_respond_app(mock).await;
    let user = authenticated_user();
    let game_id = Uuid::new_v4();

    let req = test::TestRequest::post()
        .uri(&format!("/{game_id}/respond?action=accept"))
        .to_request();
    req.extensions_mut().insert(user);

    let resp = test::call_service(&app, req).await;
    assert!(!resp.status().is_success());
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], false);
}

#[actix_web::test]
async fn respond_decline_success() {
    let mock = Arc::new(MockGameService::ok());
    let app = make_respond_app(mock).await;
    let user = authenticated_user();
    let game_id = Uuid::new_v4();

    let req = test::TestRequest::post()
        .uri(&format!("/{game_id}/respond?action=decline"))
        .to_request();
    req.extensions_mut().insert(user);

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], true);
    assert_eq!(body["action"], "decline");
    assert_eq!(body["message"], "Invitation declined");
    assert!(body["player_id"].is_null());
    assert!(body["position"].is_null());
    assert!(body["player_count"].is_null());
    assert!(body["max_players"].is_null());
    assert!(body["game_status"].is_null());
}

#[actix_web::test]
async fn respond_invalid_action_returns_400() {
    let mock = Arc::new(MockGameService::ok());
    let app = make_respond_app(mock).await;
    let user = authenticated_user();
    let game_id = Uuid::new_v4();

    let req = test::TestRequest::post()
        .uri(&format!("/{game_id}/respond?action=foo"))
        .to_request();
    req.extensions_mut().insert(user);

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], false);
    assert!(body["error"].as_str().unwrap().contains("accept"));
}

#[actix_web::test]
async fn respond_missing_action_returns_error() {
    let mock = Arc::new(MockGameService::ok());
    let app = make_respond_app(mock).await;
    let user = authenticated_user();
    let game_id = Uuid::new_v4();

    let req = test::TestRequest::post()
        .uri(&format!("/{game_id}/respond"))
        .to_request();
    req.extensions_mut().insert(user);

    let resp = test::call_service(&app, req).await;
    assert!(!resp.status().is_success());
}

#[actix_web::test]
async fn respond_accept_account_frozen() {
    let mock = Arc::new(MockGameService::ok());
    mock.set_accept_invite_result(Err(GameError::AccountFrozen {
        until: "2026-05-18T12:00:00+00:00".to_string(),
    }));
    let app = make_respond_app(mock).await;
    let user = authenticated_user();
    let game_id = Uuid::new_v4();

    let req = test::TestRequest::post()
        .uri(&format!("/{game_id}/respond?action=accept"))
        .to_request();
    req.extensions_mut().insert(user);

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], false);
    assert!(body["error"].as_str().unwrap().contains("frozen"));
}

// ── create_game tests ──

async fn make_create_game_app(
    mock: Arc<dyn GameLifecycleService>,
    db: sea_orm::DatabaseConnection,
) -> impl actix_web::dev::Service<
    actix_http::Request,
    Response = actix_web::dev::ServiceResponse,
    Error = actix_web::Error,
> {
    test::init_service(
        App::new()
            .app_data(web::Data::new(mock))
            .app_data(web::Data::new(db))
            .service(web::resource("/games").route(web::post().to(create_game))),
    )
    .await
}

#[actix_web::test]
async fn create_game_solo_success() {
    let db = MockDatabase::new(DatabaseBackend::Postgres).into_connection();
    let mock = Arc::new(MockGameService::ok());
    let app = make_create_game_app(mock, db).await;
    let user = authenticated_user();

    let req = test::TestRequest::post()
        .uri("/games")
        .set_json(serde_json::json!({
            "game_mode": "solo",
            "bet": 10,
            "step_by_step": false
        }))
        .to_request();
    req.extensions_mut().insert(user);

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
}

#[actix_web::test]
async fn create_game_multiplayer_success() {
    let db = MockDatabase::new(DatabaseBackend::Postgres).into_connection();
    let mock = Arc::new(MockGameService::ok());
    let app = make_create_game_app(mock, db).await;
    let user = authenticated_user();

    let req = test::TestRequest::post()
        .uri("/games")
        .set_json(serde_json::json!({
            "game_mode": "multiplayer",
            "bet": 10,
            "max_players": 4
        }))
        .to_request();
    req.extensions_mut().insert(user);

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
}

#[actix_web::test]
async fn create_game_invalid_mode() {
    let db = MockDatabase::new(DatabaseBackend::Postgres).into_connection();
    let mock = Arc::new(MockGameService::ok());
    let app = make_create_game_app(mock, db).await;
    let user = authenticated_user();

    let req = test::TestRequest::post()
        .uri("/games")
        .set_json(serde_json::json!({
            "game_mode": "invalid",
            "bet": 10
        }))
        .to_request();
    req.extensions_mut().insert(user);

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_client_error());
}

#[actix_web::test]
async fn create_game_multiplayer_negative_bet() {
    let db = MockDatabase::new(DatabaseBackend::Postgres).into_connection();
    let mock = Arc::new(MockGameService::ok());
    let app = make_create_game_app(mock, db).await;
    let user = authenticated_user();

    let req = test::TestRequest::post()
        .uri("/games")
        .set_json(serde_json::json!({
            "game_mode": "multiplayer",
            "bet": -5,
            "max_players": 4
        }))
        .to_request();
    req.extensions_mut().insert(user);

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_client_error());
}
