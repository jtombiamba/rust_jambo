use crate::api::unfreeze::{
    capture_unfreeze_order, close_window_error_html, close_window_html, create_unfreeze_order,
    finalize_unfreeze, paypal_cancel, paypal_return, UNFREEZE_CAPTURE_PREFIX, UNFREEZE_IDEM_PREFIX,
    UNFREEZE_ORDER_PREFIX,
};
use crate::auth::extractors::AuthenticatedUser;
use crate::config::Config;
use crate::database::models::{PlayerProfile, PlayerType, TopupTransaction, TopupTransactionKind};
use crate::error::AppError;
use crate::messaging::RedisClient;
use actix_web::{web, HttpMessage};
use sea_orm::{DatabaseBackend, MockDatabase, MockExecResult};
use std::sync::Arc;
use uuid::Uuid;

fn make_profile(user_id: Uuid, credit: i32) -> PlayerProfile {
    PlayerProfile {
        id: Uuid::now_v7(),
        user_id,
        player_type: PlayerType::Human,
        credit,
        game_played: 0,
        wins: 0,
        kora_wins: 0,
        winning_streak: 0,
        latitude: None,
        longitude: None,
        country_code: None,
        city: None,
        frozen_until: Some(chrono::Utc::now()),
        cashout_locked: false,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

#[test]
fn test_unfreeze_redis_key_format() {
    let user_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
    let order_id = "ORDER123";
    let key = format!("{}:{}:{}", UNFREEZE_CAPTURE_PREFIX, user_id, order_id);
    assert_eq!(
        key,
        "unfreeze_capture:550e8400-e29b-41d4-a716-446655440000:ORDER123"
    );
}

#[test]
fn test_paypal_idem_key_format() {
    let order_id = "ORDER456";
    let key = format!("{}_{}", UNFREEZE_IDEM_PREFIX, order_id);
    assert_eq!(key, "unfreeze_ORDER456");
}

#[test]
fn test_order_redis_key_format() {
    let order_id = "ORDER789";
    let key = format!("{}:{}", UNFREEZE_ORDER_PREFIX, order_id);
    assert_eq!(key, "unfreeze_order:ORDER789");
}

#[test]
fn test_close_window_html_returns_ok() {
    let resp = close_window_html("Test Title");
    assert!(resp.status().is_success());
}

#[test]
fn test_close_window_error_html_returns_server_error() {
    let resp = close_window_error_html("Test Error");
    assert!(resp.status().is_server_error());
}

#[tokio::test]
async fn finalize_unfreeze_records_and_unfreezes() {
    let user_id = Uuid::now_v7();
    let db = MockDatabase::new(DatabaseBackend::Postgres)
        .append_exec_results(vec![
            MockExecResult {
                last_insert_id: 0,
                rows_affected: 1,
            },
            MockExecResult {
                last_insert_id: 0,
                rows_affected: 1,
            },
        ])
        .append_query_results(vec![vec![make_profile(user_id, 50)]])
        .into_connection();

    let credit = finalize_unfreeze(user_id, &web::Data::new(db), 250, 100)
        .await
        .expect("finalize should succeed");

    assert_eq!(credit, 250);
}

#[tokio::test]
async fn finalize_unfreeze_returns_error_when_credit_write_fails() {
    let user_id = Uuid::now_v7();
    let db = MockDatabase::new(DatabaseBackend::Postgres)
        .append_exec_results(vec![MockExecResult {
            last_insert_id: 0,
            rows_affected: 1,
        }])
        .append_exec_errors(vec![sea_orm::DbErr::Custom(
            "mock credit write failure".into(),
        )])
        .append_query_results(vec![vec![make_profile(user_id, 50)]])
        .into_connection();

    let result = finalize_unfreeze(user_id, &web::Data::new(db), 250, 100).await;

    assert!(matches!(result, Err(AppError::Database(_))));
}

#[tokio::test]
async fn finalize_unfreeze_returns_not_found_when_profile_missing() {
    let user_id = Uuid::now_v7();
    let db = MockDatabase::new(DatabaseBackend::Postgres)
        .append_exec_results(vec![MockExecResult {
            last_insert_id: 0,
            rows_affected: 1,
        }])
        .append_query_results(vec![Vec::<PlayerProfile>::new()])
        .into_connection();

    let result = finalize_unfreeze(user_id, &web::Data::new(db), 250, 100).await;

    assert!(matches!(result, Err(AppError::NotFound(_))));
}

fn unconfigured_payment_service() -> Arc<dyn crate::payment::PaymentServiceTrait> {
    Arc::new(crate::payment::PaymentService::new(
        "".to_string(),
        "".to_string(),
        "sandbox".to_string(),
        "1.00".to_string(),
        "1.00".to_string(),
        "https://api-m.sandbox.paypal.com".to_string(),
        "https://api-m.paypal.com".to_string(),
    ))
}

fn authenticated_user() -> AuthenticatedUser {
    AuthenticatedUser {
        user_id: Uuid::new_v4(),
        pseudo: "TestPlayer".to_string(),
    }
}

#[actix_web::test]
async fn create_unfreeze_order_unconfigured_returns_500() {
    let db = MockDatabase::new(DatabaseBackend::Postgres).into_connection();
    let app = actix_web::test::init_service(
        actix_web::App::new()
            .app_data(web::Data::new(unconfigured_payment_service()))
            .app_data(web::Data::new(Config::default()))
            .app_data(web::Data::new(None::<RedisClient>))
            .app_data(web::Data::new(db))
            .route("/unfreeze", web::post().to(create_unfreeze_order)),
    )
    .await;

    let user = authenticated_user();
    let req = actix_web::test::TestRequest::post()
        .uri("/unfreeze")
        .to_request();
    req.extensions_mut().insert(user);

    let resp = actix_web::test::call_service(&app, req).await;
    assert_eq!(resp.status(), 500);
}

#[actix_web::test]
async fn capture_unfreeze_order_unconfigured_returns_500() {
    let db = MockDatabase::new(DatabaseBackend::Postgres).into_connection();
    let app = actix_web::test::init_service(
        actix_web::App::new()
            .app_data(web::Data::new(unconfigured_payment_service()))
            .app_data(web::Data::new(Config::default()))
            .app_data(web::Data::new(None::<RedisClient>))
            .app_data(web::Data::new(db))
            .route("/unfreeze/capture", web::post().to(capture_unfreeze_order)),
    )
    .await;

    let user = authenticated_user();
    let req = actix_web::test::TestRequest::post()
        .uri("/unfreeze/capture")
        .set_json(serde_json::json!({ "order_id": "ORDER_1" }))
        .to_request();
    req.extensions_mut().insert(user);

    let resp = actix_web::test::call_service(&app, req).await;
    assert_eq!(resp.status(), 500);
}

#[actix_web::test]
async fn paypal_cancel_returns_html() {
    let app = actix_web::test::init_service(
        actix_web::App::new().route("/paypal/cancel", web::get().to(paypal_cancel)),
    )
    .await;

    let req = actix_web::test::TestRequest::get()
        .uri("/paypal/cancel")
        .to_request();
    let resp = actix_web::test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

fn make_tx(user_id: Uuid, amount_eur_cents: i32) -> TopupTransaction {
    TopupTransaction {
        id: Uuid::now_v7(),
        user_id,
        kind: TopupTransactionKind::Unfreeze,
        amount_eur_cents,
        credits: 250,
        order_id: None,
        created_at: chrono::Utc::now(),
    }
}

async fn make_unfreeze_app(
    payment: Arc<dyn crate::payment::PaymentServiceTrait>,
    db: sea_orm::DatabaseConnection,
) -> impl actix_web::dev::Service<
    actix_http::Request,
    Response = actix_web::dev::ServiceResponse,
    Error = actix_web::Error,
> {
    actix_web::test::init_service(
        actix_web::App::new()
            .app_data(web::Data::new(payment))
            .app_data(web::Data::new(Config::default()))
            .app_data(web::Data::new(None::<RedisClient>))
            .app_data(web::Data::new(db))
            .service(web::resource("/unfreeze").route(web::post().to(create_unfreeze_order)))
            .service(
                web::resource("/unfreeze/capture").route(web::post().to(capture_unfreeze_order)),
            )
            .route("/paypal/return", web::get().to(paypal_return)),
    )
    .await
}

#[actix_web::test]
async fn create_unfreeze_order_success() {
    let db = MockDatabase::new(DatabaseBackend::Postgres)
        .append_query_results(vec![Vec::<TopupTransaction>::new()])
        .into_connection();
    let app = make_unfreeze_app(
        Arc::new(crate::test_helpers::MockPaymentService::configured()),
        db,
    )
    .await;
    let user = authenticated_user();
    let req = actix_web::test::TestRequest::post()
        .uri("/unfreeze")
        .to_request();
    req.extensions_mut().insert(user);
    let resp = actix_web::test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = actix_web::test::read_body_json(resp).await;
    assert_eq!(body["order_id"], "ORDER_1");
}

#[actix_web::test]
async fn create_unfreeze_order_monthly_limit_returns_400() {
    let user_id = Uuid::new_v4();
    let db = MockDatabase::new(DatabaseBackend::Postgres)
        .append_query_results(vec![vec![make_tx(user_id, 10000)]])
        .into_connection();
    let app = make_unfreeze_app(
        Arc::new(crate::test_helpers::MockPaymentService::configured()),
        db,
    )
    .await;
    let user = authenticated_user();
    let req = actix_web::test::TestRequest::post()
        .uri("/unfreeze")
        .to_request();
    req.extensions_mut().insert(user);
    let resp = actix_web::test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
}

#[actix_web::test]
async fn capture_unfreeze_order_paypal_failure_returns_500() {
    let payment = Arc::new(crate::test_helpers::MockPaymentService::configured());
    payment.set_capture_order_result(Err("paypal down".to_string()));
    let db = MockDatabase::new(DatabaseBackend::Postgres).into_connection();
    let app = make_unfreeze_app(payment, db).await;
    let user = authenticated_user();
    let req = actix_web::test::TestRequest::post()
        .uri("/unfreeze/capture")
        .set_json(serde_json::json!({ "order_id": "ORDER_1" }))
        .to_request();
    req.extensions_mut().insert(user);
    let resp = actix_web::test::call_service(&app, req).await;
    assert_eq!(resp.status(), 500);
}

#[actix_web::test]
async fn capture_unfreeze_order_success() {
    let user_id = Uuid::new_v4();
    let db = MockDatabase::new(DatabaseBackend::Postgres)
        .append_exec_results(vec![
            MockExecResult {
                last_insert_id: 0,
                rows_affected: 1,
            },
            MockExecResult {
                last_insert_id: 0,
                rows_affected: 1,
            },
        ])
        .append_query_results(vec![vec![make_profile(user_id, 50)]])
        .into_connection();
    let app = make_unfreeze_app(
        Arc::new(crate::test_helpers::MockPaymentService::configured()),
        db,
    )
    .await;
    let user = authenticated_user();
    let req = actix_web::test::TestRequest::post()
        .uri("/unfreeze/capture")
        .set_json(serde_json::json!({ "order_id": "ORDER_1" }))
        .to_request();
    req.extensions_mut().insert(user);
    let resp = actix_web::test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = actix_web::test::read_body_json(resp).await;
    assert_eq!(body["success"], true);
}

#[actix_web::test]
async fn paypal_return_missing_order_id_returns_html() {
    let payment: Arc<dyn crate::payment::PaymentServiceTrait> =
        Arc::new(crate::test_helpers::MockPaymentService::configured());
    let db = MockDatabase::new(DatabaseBackend::Postgres).into_connection();
    let app = make_unfreeze_app(payment, db).await;
    let req = actix_web::test::TestRequest::get()
        .uri("/paypal/return")
        .to_request();
    let resp = actix_web::test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn paypal_return_session_expired_returns_html() {
    let payment: Arc<dyn crate::payment::PaymentServiceTrait> =
        Arc::new(crate::test_helpers::MockPaymentService::configured());
    let db = MockDatabase::new(DatabaseBackend::Postgres).into_connection();
    let app = make_unfreeze_app(payment, db).await;
    let req = actix_web::test::TestRequest::get()
        .uri("/paypal/return?token=ORDER_1")
        .to_request();
    let resp = actix_web::test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn create_unfreeze_order_paypal_failure_returns_500() {
    let payment = Arc::new(crate::test_helpers::MockPaymentService::configured());
    payment.set_create_order_result(Err("paypal down".to_string()));
    let db = MockDatabase::new(DatabaseBackend::Postgres)
        .append_query_results(vec![Vec::<TopupTransaction>::new()])
        .into_connection();
    let app = make_unfreeze_app(payment, db).await;
    let user = authenticated_user();
    let req = actix_web::test::TestRequest::post()
        .uri("/unfreeze")
        .to_request();
    req.extensions_mut().insert(user);
    let resp = actix_web::test::call_service(&app, req).await;
    assert_eq!(resp.status(), 500);
}

#[actix_web::test]
async fn capture_unfreeze_order_not_successful_returns_500() {
    let payment = Arc::new(crate::test_helpers::MockPaymentService::configured());
    payment.set_capture_order_result(Ok(crate::payment::CaptureResult {
        success: false,
        order_id: "ORDER_1".to_string(),
    }));
    let db = MockDatabase::new(DatabaseBackend::Postgres).into_connection();
    let app = make_unfreeze_app(payment, db).await;
    let user = authenticated_user();
    let req = actix_web::test::TestRequest::post()
        .uri("/unfreeze/capture")
        .set_json(serde_json::json!({ "order_id": "ORDER_1" }))
        .to_request();
    req.extensions_mut().insert(user);
    let resp = actix_web::test::call_service(&app, req).await;
    assert_eq!(resp.status(), 500);
}
