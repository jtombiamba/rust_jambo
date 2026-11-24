use std::sync::Arc;

use crate::api::webhooks::stripe_webhook;
use actix_web::{test, web, App};
use hmac::{Hmac, Mac};
use jambo_core::config::Config;
use jambo_core::payment::PaymentServiceTrait;
use jambo_db::models::{
    CashoutRequest, CashoutStatus, ConnectAccountStatus, PayoutMethod, PlayerProfile, PlayerType,
    User,
};
use jambo_domain::messaging::RedisClient;
use jambo_mailer::EmailQueue;
use jambo_test_helpers::MockPaymentService;
use sea_orm::{DatabaseBackend, MockDatabase, MockExecResult};
use sha2::Sha256;
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

const WEBHOOK_SECRET: &str = "whsec_test";

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

fn make_user(user_id: Uuid, account_id: Option<&str>, status: ConnectAccountStatus) -> User {
    User {
        id: user_id,
        pseudo: "alice".to_string(),
        email: "alice@example.com".to_string(),
        password_hash: "hash".to_string(),
        last_ip_hash: None,
        language: "en".to_string(),
        stripe_connect_account_id: account_id.map(String::from),
        stripe_connect_account_status: status,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        deletion_requested_at: None,
        deleted_at: None,
    }
}

fn make_cashout_request(id: Uuid, user_id: Uuid, status: CashoutStatus) -> CashoutRequest {
    CashoutRequest {
        id,
        user_id,
        credits: 100,
        amount_eur_cents: 100,
        status,
        payout_method: PayoutMethod::StripeConnect,
        payout_destination: "acct_dest".to_string(),
        payout_reference: Some("tr_ref".to_string()),
        admin_note: None,
        processed_by: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        processed_at: None,
    }
}

fn config_with_secret() -> Config {
    let mut cfg = Config::default();
    cfg.stripe_webhook_secret = WEBHOOK_SECRET.to_string();
    cfg
}

fn sign(secret: &str, timestamp: u64, payload: &str) -> String {
    let signed = format!("{}.{}", timestamp, payload);
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(signed.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

fn signature_header(secret: &str, payload: &str) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    format!("t={now},v1={}", sign(secret, now, payload))
}

async fn make_app(
    config: Config,
    db: sea_orm::DatabaseConnection,
) -> impl actix_web::dev::Service<
    actix_http::Request,
    Response = actix_web::dev::ServiceResponse,
    Error = actix_web::Error,
> {
    let payment_service: Arc<dyn PaymentServiceTrait> = Arc::new(MockPaymentService::configured());
    let (email_queue, _rx) = EmailQueue::channel();
    test::init_service(
        App::new()
            .app_data(web::Data::new(config))
            .app_data(web::Data::new(None::<RedisClient>))
            .app_data(web::Data::new(db))
            .app_data(web::Data::new(payment_service))
            .app_data(web::Data::new(email_queue))
            .route("/webhooks/stripe", web::post().to(stripe_webhook)),
    )
    .await
}

#[actix_web::test]
async fn stripe_webhook_rejects_invalid_signature() {
    let db = MockDatabase::new(DatabaseBackend::Postgres).into_connection();
    let app = make_app(config_with_secret(), db).await;
    let req = test::TestRequest::post()
        .uri("/webhooks/stripe")
        .insert_header(("Stripe-Signature", "t=1,v1=deadbeef"))
        .set_payload(r#"{"type":"checkout.session.completed"}"#)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
}

#[actix_web::test]
async fn stripe_webhook_missing_signature_returns_400() {
    let db = MockDatabase::new(DatabaseBackend::Postgres).into_connection();
    let app = make_app(config_with_secret(), db).await;
    let req = test::TestRequest::post()
        .uri("/webhooks/stripe")
        .set_payload(r#"{"type":"checkout.session.completed"}"#)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
}

#[actix_web::test]
async fn stripe_webhook_acknowledges_unknown_event() {
    let db = MockDatabase::new(DatabaseBackend::Postgres).into_connection();
    let app = make_app(config_with_secret(), db).await;
    let payload = r#"{"type":"charge.succeeded"}"#;
    let req = test::TestRequest::post()
        .uri("/webhooks/stripe")
        .insert_header((
            "Stripe-Signature",
            signature_header(WEBHOOK_SECRET, payload),
        ))
        .set_payload(payload)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn stripe_webhook_completes_topup_checkout() {
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
            vec![make_profile(user_id, 1350)],
        ])
        .into_connection();
    let app = make_app(config_with_secret(), db).await;

    let payload = serde_json::json!({
        "type": "checkout.session.completed",
        "data": { "object": {
            "id": "cs_test_1",
            "client_reference_id": user_id.to_string(),
            "metadata": { "kind": "topup", "amount_eur_cents": "500", "credits": "1250" }
        } }
    })
    .to_string();

    let req = test::TestRequest::post()
        .uri("/webhooks/stripe")
        .insert_header((
            "Stripe-Signature",
            signature_header(WEBHOOK_SECRET, &payload),
        ))
        .set_payload(payload)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn stripe_webhook_missing_client_reference_returns_500() {
    let db = MockDatabase::new(DatabaseBackend::Postgres).into_connection();
    let app = make_app(config_with_secret(), db).await;
    let payload = serde_json::json!({
        "type": "checkout.session.completed",
        "data": { "object": {
            "id": "cs_test_1",
            "metadata": { "kind": "topup", "amount_eur_cents": "500", "credits": "1250" }
        } }
    })
    .to_string();
    let req = test::TestRequest::post()
        .uri("/webhooks/stripe")
        .insert_header((
            "Stripe-Signature",
            signature_header(WEBHOOK_SECRET, &payload),
        ))
        .set_payload(payload)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 500);
}

#[actix_web::test]
async fn stripe_webhook_account_updated_marks_verified() {
    let user_id = Uuid::now_v7();
    let account_id = "acct_test";
    let user = make_user(user_id, Some(account_id), ConnectAccountStatus::NotCreated);

    let db = MockDatabase::new(DatabaseBackend::Postgres)
        .append_exec_results(vec![MockExecResult {
            last_insert_id: 0,
            rows_affected: 1,
        }])
        .append_query_results(vec![vec![user.clone()], vec![user.clone()], vec![user]])
        .into_connection();
    let app = make_app(config_with_secret(), db).await;

    let payload = serde_json::json!({
        "type": "account.updated",
        "data": { "object": { "id": account_id } }
    })
    .to_string();

    let req = test::TestRequest::post()
        .uri("/webhooks/stripe")
        .insert_header((
            "Stripe-Signature",
            signature_header(WEBHOOK_SECRET, &payload),
        ))
        .set_payload(payload)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn stripe_webhook_transfer_paid_marks_paid() {
    let user_id = Uuid::now_v7();
    let request_id = Uuid::now_v7();
    let request = make_cashout_request(request_id, user_id, CashoutStatus::Processing);
    let user = make_user(user_id, None, ConnectAccountStatus::Verified);

    let db = MockDatabase::new(DatabaseBackend::Postgres)
        .append_exec_results(vec![MockExecResult {
            last_insert_id: 0,
            rows_affected: 1,
        }])
        .append_query_results(vec![vec![request.clone()], vec![request]])
        .append_query_results(vec![vec![user]])
        .into_connection();
    let app = make_app(config_with_secret(), db).await;

    let payload = serde_json::json!({
        "type": "transfer.paid",
        "data": { "object": {
            "id": "tr_test",
            "metadata": { "item_id": request_id.to_string() }
        } }
    })
    .to_string();

    let req = test::TestRequest::post()
        .uri("/webhooks/stripe")
        .insert_header((
            "Stripe-Signature",
            signature_header(WEBHOOK_SECRET, &payload),
        ))
        .set_payload(payload)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn stripe_webhook_transfer_failed_refunds() {
    let user_id = Uuid::now_v7();
    let request_id = Uuid::now_v7();
    let request = make_cashout_request(request_id, user_id, CashoutStatus::Processing);
    let user = make_user(user_id, None, ConnectAccountStatus::Verified);

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
            MockExecResult {
                last_insert_id: 0,
                rows_affected: 1,
            },
        ])
        .append_query_results(vec![vec![request.clone()], vec![request]])
        .append_query_results(vec![vec![user]])
        .into_connection();
    let app = make_app(config_with_secret(), db).await;

    let payload = serde_json::json!({
        "type": "transfer.failed",
        "data": { "object": {
            "id": "tr_test",
            "metadata": { "item_id": request_id.to_string() }
        } }
    })
    .to_string();

    let req = test::TestRequest::post()
        .uri("/webhooks/stripe")
        .insert_header((
            "Stripe-Signature",
            signature_header(WEBHOOK_SECRET, &payload),
        ))
        .set_payload(payload)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}
