use crate::api::topup::{
    capture_topup_order, create_topup_order, finalize_topup, paypal_cancel_topup,
    paypal_return_topup,
};
use crate::auth::extractors::AuthenticatedUser;
use crate::config::Config;
use crate::database::models::{PlayerProfile, PlayerType, TopupTransaction, TopupTransactionKind};
use crate::error::AppError;
use crate::messaging::RedisClient;
use crate::test_helpers::MockPaymentService;
use actix_web::{web, HttpMessage};
use sea_orm::{DatabaseBackend, MockDatabase, MockExecResult};
use std::sync::Arc;
use uuid::Uuid;

fn authenticated_user() -> AuthenticatedUser {
    AuthenticatedUser {
        user_id: Uuid::new_v4(),
        pseudo: "TestPlayer".to_string(),
    }
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

fn make_tx(user_id: Uuid, amount_eur_cents: i32) -> TopupTransaction {
    TopupTransaction {
        id: Uuid::now_v7(),
        user_id,
        kind: TopupTransactionKind::Topup,
        amount_eur_cents,
        credits: amount_eur_cents,
        order_id: None,
        created_at: chrono::Utc::now(),
    }
}

async fn make_topup_app(
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
            .service(web::resource("/topup").route(web::post().to(create_topup_order)))
            .service(web::resource("/topup/capture").route(web::post().to(capture_topup_order))),
    )
    .await
}

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
        frozen_until: None,
        cashout_locked: false,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

#[tokio::test]
async fn finalize_topup_records_transaction_and_credits() {
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
        .append_query_results(vec![
            vec![make_profile(user_id, 100)],
            vec![make_profile(user_id, 350)],
        ])
        .into_connection();

    let credit = finalize_topup(
        &web::Data::new(db),
        None,
        "topup_capture:key",
        user_id,
        "ORDER_1",
        250,
        100,
    )
    .await
    .expect("finalize should succeed");

    assert_eq!(credit, 350);
}

#[tokio::test]
async fn finalize_topup_skips_credit_when_order_already_recorded() {
    let user_id = Uuid::now_v7();
    let db = MockDatabase::new(DatabaseBackend::Postgres)
        .append_exec_results(vec![MockExecResult {
            last_insert_id: 0,
            rows_affected: 0,
        }])
        .append_query_results(vec![vec![make_profile(user_id, 100)]])
        .into_connection();

    let credit = finalize_topup(
        &web::Data::new(db),
        None,
        "topup_capture:key",
        user_id,
        "ORDER_1",
        250,
        100,
    )
    .await
    .expect("finalize should succeed");

    assert_eq!(credit, 100);
}

#[tokio::test]
async fn finalize_topup_returns_not_found_when_profile_missing() {
    let user_id = Uuid::now_v7();
    let db = MockDatabase::new(DatabaseBackend::Postgres)
        .append_exec_results(vec![MockExecResult {
            last_insert_id: 0,
            rows_affected: 1,
        }])
        .append_query_results(vec![Vec::<PlayerProfile>::new()])
        .into_connection();

    let result = finalize_topup(
        &web::Data::new(db),
        None,
        "topup_capture:key",
        user_id,
        "ORDER_1",
        250,
        100,
    )
    .await;

    assert!(matches!(result, Err(AppError::NotFound(_))));
}

#[actix_web::test]
async fn create_topup_order_unconfigured_returns_500() {
    let db = MockDatabase::new(DatabaseBackend::Postgres).into_connection();
    let app = actix_web::test::init_service(
        actix_web::App::new()
            .app_data(web::Data::new(unconfigured_payment_service()))
            .app_data(web::Data::new(Config::default()))
            .app_data(web::Data::new(None::<RedisClient>))
            .app_data(web::Data::new(db))
            .route("/topup", web::post().to(create_topup_order)),
    )
    .await;

    let user = authenticated_user();
    let req = actix_web::test::TestRequest::post()
        .uri("/topup")
        .to_request();
    req.extensions_mut().insert(user);

    let resp = actix_web::test::call_service(&app, req).await;
    assert_eq!(resp.status(), 500);
    let body: serde_json::Value = actix_web::test::read_body_json(resp).await;
    assert_eq!(body["success"], false);
}

#[actix_web::test]
async fn capture_topup_order_unconfigured_returns_500() {
    let db = MockDatabase::new(DatabaseBackend::Postgres).into_connection();
    let app = actix_web::test::init_service(
        actix_web::App::new()
            .app_data(web::Data::new(unconfigured_payment_service()))
            .app_data(web::Data::new(Config::default()))
            .app_data(web::Data::new(None::<RedisClient>))
            .app_data(web::Data::new(db))
            .route("/topup/capture", web::post().to(capture_topup_order)),
    )
    .await;

    let user = authenticated_user();
    let req = actix_web::test::TestRequest::post()
        .uri("/topup/capture")
        .set_json(serde_json::json!({ "order_id": "ORDER_1" }))
        .to_request();
    req.extensions_mut().insert(user);

    let resp = actix_web::test::call_service(&app, req).await;
    assert_eq!(resp.status(), 500);
    let body: serde_json::Value = actix_web::test::read_body_json(resp).await;
    assert_eq!(body["success"], false);
}

#[actix_web::test]
async fn paypal_cancel_topup_returns_html() {
    let app = actix_web::test::init_service(
        actix_web::App::new().route("/paypal/topup/cancel", web::get().to(paypal_cancel_topup)),
    )
    .await;

    let req = actix_web::test::TestRequest::get()
        .uri("/paypal/topup/cancel")
        .to_request();
    let resp = actix_web::test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn create_topup_order_success() {
    let user_id = Uuid::new_v4();
    let db = MockDatabase::new(DatabaseBackend::Postgres)
        .append_query_results(vec![vec![make_profile(user_id, 100)]])
        .append_query_results(vec![Vec::<TopupTransaction>::new()])
        .into_connection();
    let app = make_topup_app(Arc::new(MockPaymentService::configured()), db).await;
    let user = authenticated_user();
    let req = actix_web::test::TestRequest::post()
        .uri("/topup")
        .to_request();
    req.extensions_mut().insert(user);
    let resp = actix_web::test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = actix_web::test::read_body_json(resp).await;
    assert_eq!(body["order_id"], "ORDER_1");
}

#[actix_web::test]
async fn create_topup_order_profile_not_found_returns_404() {
    let db = MockDatabase::new(DatabaseBackend::Postgres)
        .append_query_results(vec![Vec::<PlayerProfile>::new()])
        .into_connection();
    let app = make_topup_app(Arc::new(MockPaymentService::configured()), db).await;
    let user = authenticated_user();
    let req = actix_web::test::TestRequest::post()
        .uri("/topup")
        .to_request();
    req.extensions_mut().insert(user);
    let resp = actix_web::test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);
}

#[actix_web::test]
async fn create_topup_order_frozen_returns_403() {
    let user_id = Uuid::new_v4();
    let mut profile = make_profile(user_id, 100);
    profile.frozen_until = Some(chrono::Utc::now() + chrono::Duration::hours(1));
    let db = MockDatabase::new(DatabaseBackend::Postgres)
        .append_query_results(vec![vec![profile]])
        .into_connection();
    let app = make_topup_app(Arc::new(MockPaymentService::configured()), db).await;
    let user = authenticated_user();
    let req = actix_web::test::TestRequest::post()
        .uri("/topup")
        .to_request();
    req.extensions_mut().insert(user);
    let resp = actix_web::test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);
}

#[actix_web::test]
async fn create_topup_order_credit_depleted_returns_400() {
    let user_id = Uuid::new_v4();
    let db = MockDatabase::new(DatabaseBackend::Postgres)
        .append_query_results(vec![vec![make_profile(user_id, 0)]])
        .into_connection();
    let app = make_topup_app(Arc::new(MockPaymentService::configured()), db).await;
    let user = authenticated_user();
    let req = actix_web::test::TestRequest::post()
        .uri("/topup")
        .to_request();
    req.extensions_mut().insert(user);
    let resp = actix_web::test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
}

#[actix_web::test]
async fn create_topup_order_monthly_limit_returns_400() {
    let user_id = Uuid::new_v4();
    let db = MockDatabase::new(DatabaseBackend::Postgres)
        .append_query_results(vec![vec![make_profile(user_id, 100)]])
        .append_query_results(vec![vec![make_tx(user_id, 10000)]])
        .into_connection();
    let app = make_topup_app(Arc::new(MockPaymentService::configured()), db).await;
    let user = authenticated_user();
    let req = actix_web::test::TestRequest::post()
        .uri("/topup")
        .to_request();
    req.extensions_mut().insert(user);
    let resp = actix_web::test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
}

#[actix_web::test]
async fn capture_topup_order_already_added_returns_200() {
    let user_id = Uuid::new_v4();
    let db = MockDatabase::new(DatabaseBackend::Postgres)
        .append_query_results(vec![vec![make_tx(user_id, 100)]])
        .append_query_results(vec![vec![make_profile(user_id, 350)]])
        .into_connection();
    let app = make_topup_app(Arc::new(MockPaymentService::configured()), db).await;
    let user = authenticated_user();
    let req = actix_web::test::TestRequest::post()
        .uri("/topup/capture")
        .set_json(serde_json::json!({ "order_id": "ORDER_1" }))
        .to_request();
    req.extensions_mut().insert(user);
    let resp = actix_web::test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = actix_web::test::read_body_json(resp).await;
    assert_eq!(body["success"], true);
}

#[actix_web::test]
async fn capture_topup_order_paypal_failure_returns_500() {
    let payment = Arc::new(MockPaymentService::configured());
    payment.set_capture_order_result(Err("paypal down".to_string()));
    let db = MockDatabase::new(DatabaseBackend::Postgres)
        .append_query_results(vec![Vec::<TopupTransaction>::new()])
        .into_connection();
    let app = make_topup_app(payment, db).await;
    let user = authenticated_user();
    let req = actix_web::test::TestRequest::post()
        .uri("/topup/capture")
        .set_json(serde_json::json!({ "order_id": "ORDER_1" }))
        .to_request();
    req.extensions_mut().insert(user);
    let resp = actix_web::test::call_service(&app, req).await;
    assert_eq!(resp.status(), 500);
}

#[actix_web::test]
async fn capture_topup_order_success() {
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
        .append_query_results(vec![Vec::<TopupTransaction>::new()])
        .append_query_results(vec![vec![make_profile(user_id, 100)]])
        .append_query_results(vec![vec![make_profile(user_id, 350)]])
        .into_connection();
    let app = make_topup_app(Arc::new(MockPaymentService::configured()), db).await;
    let user = authenticated_user();
    let req = actix_web::test::TestRequest::post()
        .uri("/topup/capture")
        .set_json(serde_json::json!({ "order_id": "ORDER_1" }))
        .to_request();
    req.extensions_mut().insert(user);
    let resp = actix_web::test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = actix_web::test::read_body_json(resp).await;
    assert_eq!(body["success"], true);
}

#[actix_web::test]
async fn create_topup_order_paypal_failure_returns_500() {
    let user_id = Uuid::new_v4();
    let payment = Arc::new(MockPaymentService::configured());
    payment.set_create_topup_order_result(Err("paypal down".to_string()));
    let db = MockDatabase::new(DatabaseBackend::Postgres)
        .append_query_results(vec![vec![make_profile(user_id, 100)]])
        .append_query_results(vec![Vec::<TopupTransaction>::new()])
        .into_connection();
    let app = make_topup_app(payment, db).await;
    let user = authenticated_user();
    let req = actix_web::test::TestRequest::post()
        .uri("/topup")
        .to_request();
    req.extensions_mut().insert(user);
    let resp = actix_web::test::call_service(&app, req).await;
    assert_eq!(resp.status(), 500);
}

#[actix_web::test]
async fn capture_topup_order_not_successful_returns_500() {
    let payment = Arc::new(MockPaymentService::configured());
    payment.set_capture_order_result(Ok(crate::payment::CaptureResult {
        success: false,
        order_id: "ORDER_1".to_string(),
    }));
    let db = MockDatabase::new(DatabaseBackend::Postgres)
        .append_query_results(vec![Vec::<TopupTransaction>::new()])
        .into_connection();
    let app = make_topup_app(payment, db).await;
    let user = authenticated_user();
    let req = actix_web::test::TestRequest::post()
        .uri("/topup/capture")
        .set_json(serde_json::json!({ "order_id": "ORDER_1" }))
        .to_request();
    req.extensions_mut().insert(user);
    let resp = actix_web::test::call_service(&app, req).await;
    assert_eq!(resp.status(), 500);
}

#[actix_web::test]
async fn paypal_return_topup_missing_order_id_returns_html() {
    let payment: Arc<dyn crate::payment::PaymentServiceTrait> =
        Arc::new(MockPaymentService::configured());
    let db = MockDatabase::new(DatabaseBackend::Postgres).into_connection();
    let app = actix_web::test::init_service(
        actix_web::App::new()
            .app_data(web::Data::new(payment))
            .app_data(web::Data::new(Config::default()))
            .app_data(web::Data::new(None::<RedisClient>))
            .app_data(web::Data::new(db))
            .route("/paypal/topup/return", web::get().to(paypal_return_topup)),
    )
    .await;
    let req = actix_web::test::TestRequest::get()
        .uri("/paypal/topup/return")
        .to_request();
    let resp = actix_web::test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn paypal_return_topup_session_expired_returns_html() {
    let payment: Arc<dyn crate::payment::PaymentServiceTrait> =
        Arc::new(MockPaymentService::configured());
    let db = MockDatabase::new(DatabaseBackend::Postgres).into_connection();
    let app = actix_web::test::init_service(
        actix_web::App::new()
            .app_data(web::Data::new(payment))
            .app_data(web::Data::new(Config::default()))
            .app_data(web::Data::new(None::<RedisClient>))
            .app_data(web::Data::new(db))
            .route("/paypal/topup/return", web::get().to(paypal_return_topup)),
    )
    .await;
    let req = actix_web::test::TestRequest::get()
        .uri("/paypal/topup/return?token=ORDER_1")
        .to_request();
    let resp = actix_web::test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}
