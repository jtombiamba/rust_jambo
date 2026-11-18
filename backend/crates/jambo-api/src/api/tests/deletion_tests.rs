use crate::api::deletion::request_account_deletion;
use crate::auth::extractors::AuthenticatedUser;
use actix_web::{test, web, App, HttpMessage};
use jambo_core::config::Config;
use jambo_core::i18n::Translator;
use jambo_db::models::User;
use sea_orm::{DatabaseBackend, MockDatabase};
use std::sync::Arc;
use uuid::Uuid;

fn authenticated_user() -> AuthenticatedUser {
    AuthenticatedUser {
        user_id: Uuid::now_v7(),
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
        deletion_requested_at: None,
        deleted_at: None,
    }
}

async fn make_app(
    db: sea_orm::DatabaseConnection,
) -> impl actix_web::dev::Service<
    actix_http::Request,
    Response = actix_web::dev::ServiceResponse,
    Error = actix_web::Error,
> {
    actix_web::test::init_service(
        App::new()
            .app_data(web::Data::new(db))
            .app_data(web::Data::new(Config::default()))
            .app_data(web::Data::new(Arc::new(Translator::new())))
            .service(
                web::resource("/deletion-request").route(web::post().to(request_account_deletion)),
            ),
    )
    .await
}

#[actix_web::test]
async fn request_account_deletion_success() {
    let user = authenticated_user();
    let updated = User {
        deletion_requested_at: Some(chrono::Utc::now()),
        ..make_user(user.user_id)
    };
    let db = MockDatabase::new(DatabaseBackend::Postgres)
        .append_query_results(vec![vec![make_user(user.user_id)], vec![updated]])
        .into_connection();
    let app = make_app(db).await;

    let req = test::TestRequest::post()
        .uri("/deletion-request")
        .to_request();
    req.extensions_mut().insert(user);
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], true);
    assert!(body["message"].is_string());
}

#[actix_web::test]
async fn request_account_deletion_requires_auth() {
    let db = MockDatabase::new(DatabaseBackend::Postgres).into_connection();
    let app = make_app(db).await;

    let req = test::TestRequest::post()
        .uri("/deletion-request")
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), 401);
}
