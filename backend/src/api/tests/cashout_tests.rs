use crate::api::cashout::{list_cashouts, request_cashout};
use crate::api::services::cashout_service::CashoutServiceTrait;
use crate::auth::extractors::AuthenticatedUser;
use crate::config::Config;
use crate::database::models::User;
use crate::error::AppError;
use crate::mailer::Mailer;
use crate::test_helpers::{MockCashoutService, MockMailer};
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

fn make_user(user_id: Uuid) -> User {
    let now = chrono::Utc::now();
    User {
        id: user_id,
        pseudo: "alice".to_string(),
        email: "a@b.co".to_string(),
        password_hash: "hash".to_string(),
        last_ip_hash: None,
        language: "en".to_string(),
        created_at: now,
        updated_at: now,
    }
}

async fn make_cashout_app(
    mock: Arc<dyn CashoutServiceTrait>,
) -> impl actix_web::dev::Service<
    actix_http::Request,
    Response = actix_web::dev::ServiceResponse,
    Error = actix_web::Error,
> {
    let mailer: Arc<dyn Mailer> = Arc::new(MockMailer::ok());
    let db = MockDatabase::new(DatabaseBackend::Postgres)
        .append_query_results(vec![Vec::<User>::new()])
        .into_connection();
    test::init_service(
        App::new()
            .app_data(web::Data::new(mock))
            .app_data(web::Data::new(mailer))
            .app_data(web::Data::new(Config::default()))
            .app_data(web::Data::new(db))
            .service(
                web::resource("/cashout")
                    .route(web::post().to(request_cashout))
                    .route(web::get().to(list_cashouts)),
            ),
    )
    .await
}

#[actix_web::test]
async fn request_cashout_success() {
    let app = make_cashout_app(Arc::new(MockCashoutService::new())).await;
    let user = authenticated_user();
    let req = test::TestRequest::post()
        .uri("/cashout")
        .set_json(serde_json::json!({ "credits": 250, "paypal_email": "a@b.co" }))
        .to_request();
    req.extensions_mut().insert(user);
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["credits"], 250);
}

#[actix_web::test]
async fn request_cashout_forbidden_returns_error() {
    let mock = Arc::new(MockCashoutService::new());
    mock.set_request_result(Err(AppError::Forbidden("cashout.unavailable")));
    let app = make_cashout_app(mock).await;
    let user = authenticated_user();
    let req = test::TestRequest::post()
        .uri("/cashout")
        .set_json(serde_json::json!({ "credits": 250, "paypal_email": "a@b.co" }))
        .to_request();
    req.extensions_mut().insert(user);
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);
}

#[actix_web::test]
async fn list_cashouts_success() {
    let app = make_cashout_app(Arc::new(MockCashoutService::new())).await;
    let user = authenticated_user();
    let req = test::TestRequest::get()
        .uri("/cashout?page=1&per_page=10")
        .to_request();
    req.extensions_mut().insert(user);
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["total"], 0);
}

#[actix_web::test]
async fn request_cashout_success_sends_emails() {
    let user = authenticated_user();
    let db = MockDatabase::new(DatabaseBackend::Postgres)
        .append_query_results(vec![vec![make_user(user.user_id)]])
        .into_connection();
    let mailer: Arc<dyn Mailer> = Arc::new(MockMailer::ok());
    let app = actix_web::test::init_service(
        App::new()
            .app_data(web::Data::new(
                Arc::new(MockCashoutService::new()) as Arc<dyn CashoutServiceTrait>
            ))
            .app_data(web::Data::new(mailer))
            .app_data(web::Data::new(Config::default()))
            .app_data(web::Data::new(db))
            .service(web::resource("/cashout").route(web::post().to(request_cashout))),
    )
    .await;

    let req = test::TestRequest::post()
        .uri("/cashout")
        .set_json(serde_json::json!({ "credits": 250, "paypal_email": "a@b.co" }))
        .to_request();
    req.extensions_mut().insert(user.clone());
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    // Yield to allow the spawned email task to run on the same runtime.
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
}
