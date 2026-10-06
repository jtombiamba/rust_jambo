use crate::api::game::{advance_bot, claim_special, decline_special, evaluate_round};
use crate::auth::config::AuthConfig;
use crate::error::GameError;
use crate::game::service::mock::MockGameService;
use crate::game::service::QuickGameOutcome;
use actix_web::{test, web, App};
use std::sync::Arc;
use uuid::Uuid;

fn quick_game_outcome() -> QuickGameOutcome {
    QuickGameOutcome {
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
    }
}

fn test_auth_config() -> AuthConfig {
    AuthConfig {
        jwt_secret: "test-secret-key-for-testing-only-1234567890".to_string(),
        jwt_expiry_hours: 24,
        ip_hash_pepper: "test-pepper".to_string(),
        frontend_url: "http://localhost:5173".to_string(),
    }
}

// ── advance_bot tests ──

async fn make_app_advance_bot(
    mock: Arc<dyn crate::game::service::GamePlayService>,
) -> impl actix_web::dev::Service<
    actix_http::Request,
    Response = actix_web::dev::ServiceResponse,
    Error = actix_web::Error,
> {
    test::init_service(
        App::new()
            .app_data(web::Data::new(mock))
            .service(web::resource("/game/{id}/advance-bot").to(advance_bot)),
    )
    .await
}

#[actix_web::test]
async fn advance_bot_success() {
    let mock = Arc::new(MockGameService::ok());
    let app = make_app_advance_bot(mock).await;
    let game_id = Uuid::new_v4();
    let player_id = Uuid::new_v4();
    let req = test::TestRequest::post()
        .uri(&format!("/game/{}/advance-bot", game_id))
        .set_json(serde_json::json!({ "player_id": player_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], true);
}

#[actix_web::test]
async fn advance_bot_game_not_found() {
    let mock = Arc::new(MockGameService::new(
        Err(GameError::GameNotFound),
        Ok(quick_game_outcome()),
    ));
    let app = make_app_advance_bot(mock).await;
    let game_id = Uuid::new_v4();
    let player_id = Uuid::new_v4();
    let req = test::TestRequest::post()
        .uri(&format!("/game/{}/advance-bot", game_id))
        .set_json(serde_json::json!({ "player_id": player_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], true);
}

#[actix_web::test]
async fn advance_bot_not_a_bot() {
    let mock = Arc::new(MockGameService::ok());
    let app = make_app_advance_bot(mock).await;
    let game_id = Uuid::new_v4();
    let player_id = Uuid::new_v4();
    let req = test::TestRequest::post()
        .uri(&format!("/game/{}/advance-bot", game_id))
        .set_json(serde_json::json!({ "player_id": player_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], true);
}

// ── evaluate_round tests ──

async fn make_app_evaluate_round(
    mock: Arc<dyn crate::game::service::GamePlayService>,
) -> impl actix_web::dev::Service<
    actix_http::Request,
    Response = actix_web::dev::ServiceResponse,
    Error = actix_web::Error,
> {
    test::init_service(
        App::new()
            .app_data(web::Data::new(mock))
            .service(web::resource("/game/{id}/evaluate-round").to(evaluate_round)),
    )
    .await
}

#[actix_web::test]
async fn evaluate_round_success() {
    let mock = Arc::new(MockGameService::ok());
    let app = make_app_evaluate_round(mock).await;
    let game_id = Uuid::new_v4();
    let player_id = Uuid::new_v4();
    let req = test::TestRequest::post()
        .uri(&format!("/game/{}/evaluate-round", game_id))
        .set_json(serde_json::json!({ "player_id": player_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], true);
}

#[actix_web::test]
async fn evaluate_round_game_not_found() {
    let mock = Arc::new(MockGameService::new(
        Err(GameError::GameNotFound),
        Ok(quick_game_outcome()),
    ));
    let app = make_app_evaluate_round(mock).await;
    let game_id = Uuid::new_v4();
    let player_id = Uuid::new_v4();
    let req = test::TestRequest::post()
        .uri(&format!("/game/{}/evaluate-round", game_id))
        .set_json(serde_json::json!({ "player_id": player_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], true);
}

#[actix_web::test]
async fn evaluate_round_round_not_complete() {
    let mock = Arc::new(MockGameService::ok());
    let app = make_app_evaluate_round(mock).await;
    let game_id = Uuid::new_v4();
    let player_id = Uuid::new_v4();
    let req = test::TestRequest::post()
        .uri(&format!("/game/{}/evaluate-round", game_id))
        .set_json(serde_json::json!({ "player_id": player_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], true);
}

// ── claim_special / decline_special tests ──

async fn make_app_special(
    mock: Arc<dyn crate::game::service::GamePlayService>,
) -> impl actix_web::dev::Service<
    actix_http::Request,
    Response = actix_web::dev::ServiceResponse,
    Error = actix_web::Error,
> {
    test::init_service(
        App::new()
            .app_data(web::Data::new(mock))
            .app_data(web::Data::new(test_auth_config()))
            .app_data(web::Data::new(None::<crate::messaging::RedisClient>))
            .service(web::resource("/game/{id}/claim-special").to(claim_special))
            .service(web::resource("/game/{id}/decline-special").to(decline_special)),
    )
    .await
}

#[actix_web::test]
async fn claim_special_unauthenticated_success() {
    let mock = Arc::new(MockGameService::ok());
    let app = make_app_special(mock).await;
    let game_id = Uuid::new_v4();
    let player_id = Uuid::new_v4();

    let req = test::TestRequest::post()
        .uri(&format!("/game/{}/claim-special", game_id))
        .set_json(serde_json::json!({ "player_id": player_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], true);
}

#[actix_web::test]
async fn claim_special_authenticated_ownership_ok() {
    let mock = Arc::new(MockGameService::ok());
    let app = make_app_special(mock).await;
    let game_id = Uuid::new_v4();
    let player_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();

    let token = crate::auth::jwt::generate_token(user_id, "alice", &test_auth_config()).unwrap();

    let req = test::TestRequest::post()
        .uri(&format!("/game/{}/claim-special", game_id))
        .cookie(actix_web::cookie::Cookie::new("Authorization", token))
        .set_json(serde_json::json!({ "player_id": player_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn claim_special_ownership_denied_returns_403() {
    let mock = Arc::new(MockGameService::ok());
    mock.set_verify_ownership_result(Ok(false));
    let app = make_app_special(mock).await;
    let game_id = Uuid::new_v4();
    let player_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();

    let token = crate::auth::jwt::generate_token(user_id, "alice", &test_auth_config()).unwrap();

    let req = test::TestRequest::post()
        .uri(&format!("/game/{}/claim-special", game_id))
        .cookie(actix_web::cookie::Cookie::new("Authorization", token))
        .set_json(serde_json::json!({ "player_id": player_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);
}

#[actix_web::test]
async fn decline_special_unauthenticated_success() {
    let mock = Arc::new(MockGameService::ok());
    let app = make_app_special(mock).await;
    let game_id = Uuid::new_v4();
    let player_id = Uuid::new_v4();

    let req = test::TestRequest::post()
        .uri(&format!("/game/{}/decline-special", game_id))
        .set_json(serde_json::json!({ "player_id": player_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], true);
}

#[actix_web::test]
async fn decline_special_ownership_denied_returns_403() {
    let mock = Arc::new(MockGameService::ok());
    mock.set_verify_ownership_result(Ok(false));
    let app = make_app_special(mock).await;
    let game_id = Uuid::new_v4();
    let player_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();

    let token = crate::auth::jwt::generate_token(user_id, "alice", &test_auth_config()).unwrap();

    let req = test::TestRequest::post()
        .uri(&format!("/game/{}/decline-special", game_id))
        .cookie(actix_web::cookie::Cookie::new("Authorization", token))
        .set_json(serde_json::json!({ "player_id": player_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);
}

// ── advance_bot / evaluate_round authenticated ownership tests ──

async fn make_advance_bot_auth_app(
    mock: Arc<dyn crate::game::service::GamePlayService>,
) -> impl actix_web::dev::Service<
    actix_http::Request,
    Response = actix_web::dev::ServiceResponse,
    Error = actix_web::Error,
> {
    test::init_service(
        App::new()
            .app_data(web::Data::new(mock))
            .app_data(web::Data::new(test_auth_config()))
            .app_data(web::Data::new(None::<crate::messaging::RedisClient>))
            .service(web::resource("/game/{id}/advance-bot").to(advance_bot)),
    )
    .await
}

async fn make_evaluate_round_auth_app(
    mock: Arc<dyn crate::game::service::GamePlayService>,
) -> impl actix_web::dev::Service<
    actix_http::Request,
    Response = actix_web::dev::ServiceResponse,
    Error = actix_web::Error,
> {
    test::init_service(
        App::new()
            .app_data(web::Data::new(mock))
            .app_data(web::Data::new(test_auth_config()))
            .app_data(web::Data::new(None::<crate::messaging::RedisClient>))
            .service(web::resource("/game/{id}/evaluate-round").to(evaluate_round)),
    )
    .await
}

#[actix_web::test]
async fn advance_bot_authenticated_ownership_ok() {
    let mock = Arc::new(MockGameService::ok());
    let app = make_advance_bot_auth_app(mock).await;
    let game_id = Uuid::new_v4();
    let player_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let token = crate::auth::jwt::generate_token(user_id, "alice", &test_auth_config()).unwrap();
    let req = test::TestRequest::post()
        .uri(&format!("/game/{}/advance-bot", game_id))
        .cookie(actix_web::cookie::Cookie::new("Authorization", token))
        .set_json(serde_json::json!({ "player_id": player_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn advance_bot_ownership_denied_returns_403() {
    let mock = Arc::new(MockGameService::ok());
    mock.set_verify_ownership_result(Ok(false));
    let app = make_advance_bot_auth_app(mock).await;
    let game_id = Uuid::new_v4();
    let player_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let token = crate::auth::jwt::generate_token(user_id, "alice", &test_auth_config()).unwrap();
    let req = test::TestRequest::post()
        .uri(&format!("/game/{}/advance-bot", game_id))
        .cookie(actix_web::cookie::Cookie::new("Authorization", token))
        .set_json(serde_json::json!({ "player_id": player_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);
}

#[actix_web::test]
async fn evaluate_round_authenticated_ownership_ok() {
    let mock = Arc::new(MockGameService::ok());
    let app = make_evaluate_round_auth_app(mock).await;
    let game_id = Uuid::new_v4();
    let player_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let token = crate::auth::jwt::generate_token(user_id, "alice", &test_auth_config()).unwrap();
    let req = test::TestRequest::post()
        .uri(&format!("/game/{}/evaluate-round", game_id))
        .cookie(actix_web::cookie::Cookie::new("Authorization", token))
        .set_json(serde_json::json!({ "player_id": player_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn evaluate_round_ownership_denied_returns_403() {
    let mock = Arc::new(MockGameService::ok());
    mock.set_verify_ownership_result(Ok(false));
    let app = make_evaluate_round_auth_app(mock).await;
    let game_id = Uuid::new_v4();
    let player_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let token = crate::auth::jwt::generate_token(user_id, "alice", &test_auth_config()).unwrap();
    let req = test::TestRequest::post()
        .uri(&format!("/game/{}/evaluate-round", game_id))
        .cookie(actix_web::cookie::Cookie::new("Authorization", token))
        .set_json(serde_json::json!({ "player_id": player_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);
}

#[actix_web::test]
async fn claim_special_service_error_returns_404() {
    let mock = Arc::new(MockGameService::ok());
    mock.set_claim_special_result(Err(GameError::GameNotFound));
    let app = make_app_special(mock).await;
    let game_id = Uuid::new_v4();
    let player_id = Uuid::new_v4();
    let req = test::TestRequest::post()
        .uri(&format!("/game/{}/claim-special", game_id))
        .set_json(serde_json::json!({ "player_id": player_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);
}

#[actix_web::test]
async fn decline_special_service_error_returns_404() {
    let mock = Arc::new(MockGameService::ok());
    mock.set_decline_special_result(Err(GameError::GameNotFound));
    let app = make_app_special(mock).await;
    let game_id = Uuid::new_v4();
    let player_id = Uuid::new_v4();
    let req = test::TestRequest::post()
        .uri(&format!("/game/{}/decline-special", game_id))
        .set_json(serde_json::json!({ "player_id": player_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);
}

#[actix_web::test]
async fn advance_bot_service_error_returns_404() {
    let mock = Arc::new(MockGameService::ok());
    mock.set_advance_bot_result(Err(GameError::GameNotFound));
    let app = make_app_advance_bot(mock).await;
    let game_id = Uuid::new_v4();
    let player_id = Uuid::new_v4();
    let req = test::TestRequest::post()
        .uri(&format!("/game/{}/advance-bot", game_id))
        .set_json(serde_json::json!({ "player_id": player_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);
}

#[actix_web::test]
async fn evaluate_round_service_error_returns_404() {
    let mock = Arc::new(MockGameService::ok());
    mock.set_evaluate_round_result(Err(GameError::GameNotFound));
    let app = make_app_evaluate_round(mock).await;
    let game_id = Uuid::new_v4();
    let player_id = Uuid::new_v4();
    let req = test::TestRequest::post()
        .uri(&format!("/game/{}/evaluate-round", game_id))
        .set_json(serde_json::json!({ "player_id": player_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);
}

#[actix_web::test]
async fn advance_bot_with_game_token_skips_ownership() {
    let mock = Arc::new(MockGameService::ok());
    let app = make_advance_bot_auth_app(mock).await;
    let game_id = Uuid::new_v4();
    let player_id = Uuid::new_v4();
    let token = crate::auth::jwt::generate_game_token(game_id, &test_auth_config(), 3600)
        .unwrap()
        .0;
    let req = test::TestRequest::post()
        .uri(&format!("/game/{}/advance-bot?token={}", game_id, token))
        .set_json(serde_json::json!({ "player_id": player_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn evaluate_round_with_game_token_skips_ownership() {
    let mock = Arc::new(MockGameService::ok());
    let app = make_evaluate_round_auth_app(mock).await;
    let game_id = Uuid::new_v4();
    let player_id = Uuid::new_v4();
    let token = crate::auth::jwt::generate_game_token(game_id, &test_auth_config(), 3600)
        .unwrap()
        .0;
    let req = test::TestRequest::post()
        .uri(&format!("/game/{}/evaluate-round?token={}", game_id, token))
        .set_json(serde_json::json!({ "player_id": player_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}
