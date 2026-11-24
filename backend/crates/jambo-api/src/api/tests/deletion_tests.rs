use crate::api::deletion::confirm_account_deletion;
use actix_web::{test, web, App};
use jambo_core::config::Config;
use jambo_core::deletion_token;
use jambo_db::models::User;
use sea_orm::{DatabaseBackend, MockDatabase};
use uuid::Uuid;

fn make_user(user_id: Uuid) -> User {
    let now = chrono::Utc::now();
    User {
        id: user_id,
        pseudo: "alice".to_string(),
        email: "a@b.co".to_string(),
        password_hash: "hash".to_string(),
        last_ip_hash: None,
        language: "en".to_string(),
        stripe_connect_account_id: None,
        stripe_connect_account_status: jambo_db::models::ConnectAccountStatus::NotCreated,
        created_at: now,
        updated_at: now,
        deletion_requested_at: None,
        deleted_at: None,
    }
}

async fn make_app(
    db: sea_orm::DatabaseConnection,
    config: Config,
) -> impl actix_web::dev::Service<
    actix_http::Request,
    Response = actix_web::dev::ServiceResponse,
    Error = actix_web::Error,
> {
    actix_web::test::init_service(
        App::new()
            .app_data(web::Data::new(db))
            .app_data(web::Data::new(config))
            .service(
                web::resource("/deletion/confirm").route(web::get().to(confirm_account_deletion)),
            ),
    )
    .await
}

#[actix_web::test]
async fn confirm_account_deletion_records_request() {
    let user_id = Uuid::now_v7();
    let config = Config::default();
    let token = deletion_token::generate_deletion_token(user_id, &config.jwt_secret).unwrap();

    let updated = User {
        deletion_requested_at: Some(chrono::Utc::now()),
        ..make_user(user_id)
    };
    let db = MockDatabase::new(DatabaseBackend::Postgres)
        .append_query_results(vec![vec![make_user(user_id)], vec![updated]])
        .into_connection();
    let app = make_app(db, config).await;

    let req = test::TestRequest::get()
        .uri(&format!("/deletion/confirm?token={token}"))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), 200);
    let content_type = resp
        .headers()
        .get(actix_web::http::header::CONTENT_TYPE)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(content_type.contains("text/html"));
}

#[actix_web::test]
async fn confirm_account_deletion_rejects_invalid_token() {
    let db = MockDatabase::new(DatabaseBackend::Postgres).into_connection();
    let app = make_app(db, Config::default()).await;

    let req = test::TestRequest::get()
        .uri("/deletion/confirm?token=not-a-token")
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), 400);
}
