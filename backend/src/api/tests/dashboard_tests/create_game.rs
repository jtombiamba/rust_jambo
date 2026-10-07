use crate::api::dashboard::create_game;
use crate::auth::extractors::AuthenticatedUser;
use crate::error::GameError;
use crate::game::service::{mock::MockGameService, GameLifecycleService};
use actix_web::{test, web, App, HttpMessage};
use sea_orm::{DatabaseBackend, MockDatabase};
use std::sync::Arc;
use uuid::Uuid;

fn authenticated_user() -> AuthenticatedUser {
    AuthenticatedUser {
        user_id: Uuid::new_v4(),
        pseudo: "TestPlayer".to_string(),
    }
}

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

#[actix_web::test]
async fn create_game_multiplayer_service_error_returns_500() {
    let db = MockDatabase::new(DatabaseBackend::Postgres).into_connection();
    let mock = Arc::new(MockGameService::ok());
    mock.set_create_multiplayer_game_result(Err(GameError::internal("boom")));
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
    assert_eq!(resp.status(), 500);
}
