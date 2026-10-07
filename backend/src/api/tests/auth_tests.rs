use crate::api::auth::{forgot_password, login, logout, me, register, reset_password};
use crate::api::services::auth_service::AuthServiceTrait;
use crate::auth::config::AuthConfig;
use crate::auth::extractors::AuthenticatedUser;
use crate::i18n::Translator;
use crate::test_helpers::{
    auth_conflict_err, auth_not_found_err, auth_unauthorized_err, auth_validation_err,
    MockAuthService,
};
use actix_web::{test, web, App, HttpMessage};
use std::sync::Arc;

fn test_auth_config() -> AuthConfig {
    AuthConfig {
        jwt_secret: "test-secret-key-for-testing-only-1234567890".to_string(),
        jwt_expiry_hours: 24,
        ip_hash_pepper: "test-pepper".to_string(),
        frontend_url: "http://localhost:5173".to_string(),
    }
}

fn authenticated_user() -> AuthenticatedUser {
    AuthenticatedUser {
        user_id: uuid::Uuid::new_v4(),
        pseudo: "TestPlayer".to_string(),
    }
}

async fn make_auth_app(
    mock: Arc<dyn AuthServiceTrait>,
) -> impl actix_web::dev::Service<
    actix_http::Request,
    Response = actix_web::dev::ServiceResponse,
    Error = actix_web::Error,
> {
    test::init_service(
        App::new()
            .app_data(web::Data::new(mock))
            .app_data(web::Data::new(Arc::new(Translator::new())))
            .app_data(web::Data::new(test_auth_config()))
            .app_data(web::Data::new(None::<crate::messaging::RedisClient>))
            .service(web::resource("/register").route(web::post().to(register)))
            .service(web::resource("/login").route(web::post().to(login)))
            .service(web::resource("/forgot-password").route(web::post().to(forgot_password)))
            .service(web::resource("/reset-password").route(web::post().to(reset_password)))
            .service(web::resource("/logout").route(web::post().to(logout)))
            .service(web::resource("/me").route(web::get().to(me))),
    )
    .await
}

#[actix_web::test]
async fn register_success_sets_cookie() {
    let app = make_auth_app(Arc::new(MockAuthService::new())).await;
    let req = test::TestRequest::post()
        .uri("/register")
        .set_json(serde_json::json!({
            "pseudo": "alice",
            "email": "a@b.co",
            "password": "password1",
            "password_confirm": "password1"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    assert!(resp.headers().get("set-cookie").is_some());
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], true);
}

#[actix_web::test]
async fn register_conflict_returns_409() {
    let mock = Arc::new(MockAuthService::new());
    mock.set_register_result(Err(auth_conflict_err()));
    let app = make_auth_app(mock).await;
    let req = test::TestRequest::post()
        .uri("/register")
        .set_json(serde_json::json!({
            "pseudo": "alice",
            "email": "a@b.co",
            "password": "password1",
            "password_confirm": "password1"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 409);
}

#[actix_web::test]
async fn register_validation_returns_400() {
    let mock = Arc::new(MockAuthService::new());
    mock.set_register_result(Err(auth_validation_err()));
    let app = make_auth_app(mock).await;
    let req = test::TestRequest::post()
        .uri("/register")
        .set_json(serde_json::json!({
            "pseudo": "alice",
            "email": "a@b.co",
            "password": "password1",
            "password_confirm": "password1"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
}

#[actix_web::test]
async fn login_success_sets_cookie() {
    let app = make_auth_app(Arc::new(MockAuthService::new())).await;
    let req = test::TestRequest::post()
        .uri("/login")
        .set_json(serde_json::json!({"email": "a@b.co", "password": "password1"}))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    assert!(resp.headers().get("set-cookie").is_some());
}

#[actix_web::test]
async fn login_unauthorized_returns_401() {
    let mock = Arc::new(MockAuthService::new());
    mock.set_login_result(Err(auth_unauthorized_err()));
    let app = make_auth_app(mock).await;
    let req = test::TestRequest::post()
        .uri("/login")
        .set_json(serde_json::json!({"email": "a@b.co", "password": "wrong"}))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
}

#[actix_web::test]
async fn forgot_password_returns_ok() {
    let app = make_auth_app(Arc::new(MockAuthService::new())).await;
    let req = test::TestRequest::post()
        .uri("/forgot-password")
        .set_json(serde_json::json!({"email": "a@b.co"}))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], true);
}

#[actix_web::test]
async fn reset_password_success() {
    let app = make_auth_app(Arc::new(MockAuthService::new())).await;
    let req = test::TestRequest::post()
        .uri("/reset-password")
        .set_json(serde_json::json!({
            "token": "t",
            "password": "password1",
            "password_confirm": "password1"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn reset_password_unauthorized_returns_error() {
    let mock = Arc::new(MockAuthService::new());
    mock.set_reset_password_result(Err(auth_unauthorized_err()));
    let app = make_auth_app(mock).await;
    let req = test::TestRequest::post()
        .uri("/reset-password")
        .set_json(serde_json::json!({
            "token": "bad",
            "password": "password1",
            "password_confirm": "password1"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
}

#[actix_web::test]
async fn logout_returns_ok() {
    let app = make_auth_app(Arc::new(MockAuthService::new())).await;
    let req = test::TestRequest::post().uri("/logout").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], true);
}

#[actix_web::test]
async fn logout_with_valid_cookie_returns_ok() {
    let app = make_auth_app(Arc::new(MockAuthService::new())).await;
    let token =
        crate::auth::jwt::generate_token(uuid::Uuid::new_v4(), "alice", &test_auth_config())
            .unwrap();
    let req = test::TestRequest::post()
        .uri("/logout")
        .cookie(actix_web::cookie::Cookie::new("Authorization", token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], true);
}

#[actix_web::test]
async fn me_returns_user_info() {
    let app = make_auth_app(Arc::new(MockAuthService::new())).await;
    let user = authenticated_user();
    let req = test::TestRequest::get().uri("/me").to_request();
    req.extensions_mut().insert(user);
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["pseudo"], "alice");
}

#[actix_web::test]
async fn me_not_found_returns_404() {
    let mock = Arc::new(MockAuthService::new());
    mock.set_me_result(Err(auth_not_found_err()));
    let app = make_auth_app(mock).await;
    let user = authenticated_user();
    let req = test::TestRequest::get().uri("/me").to_request();
    req.extensions_mut().insert(user);
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);
}
