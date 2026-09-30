use actix_web::{web, HttpRequest, HttpResponse, HttpResponseBuilder, ResponseError};
use sea_orm::TransactionTrait;
use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;

use crate::api::dto::requests::CaptureOrderRequest;
use crate::api::dto::responses::{
    ApiErrorResponse, UnfreezeCaptureResponse, UnfreezeOrderResponse,
};
use crate::api::payment_limits::{current_month_start, monthly_limit_exceeded, parse_eur_to_cents};
use crate::auth::extractors::AuthenticatedUser;
use crate::config::Config;
use crate::database::models::TopupTransactionKind;
use crate::database::repositories::{PlayerProfileRepository, TopupTransactionRepository};
use crate::error::AppError;
use crate::messaging::RedisClient;
use crate::observability::metrics::{PAYMENT_UNFREEZE_DURATION_SECONDS, PAYMENT_UNFREEZE_TOTAL};

const UNFREEZE_CAPTURE_PREFIX: &str = "unfreeze_capture";
const UNFREEZE_ORDER_PREFIX: &str = "unfreeze_order";
const UNFREEZE_TTL_SECS: u64 = 86400;
const UNFREEZE_IDEM_PREFIX: &str = "unfreeze";

pub(crate) fn close_window_html(title: &str) -> HttpResponse {
    window_html(HttpResponse::Ok(), title)
}

pub(crate) fn close_window_error_html(title: &str) -> HttpResponse {
    window_html(HttpResponse::InternalServerError(), title)
}

fn window_html(mut status: HttpResponseBuilder, title: &str) -> HttpResponse {
    status
        .content_type("text/html; charset=utf-8")
        .body(format!(
            "<!DOCTYPE html><html><head><title>{}</title></head>\
             <body style=\"font-family:sans-serif;text-align:center;padding-top:40px\">\
             <p>{}</p><p>This window will close automatically.</p>\
             <script>window.close();</script></body></html>",
            title, title
        ))
}

#[utoipa::path(
    post,
    path = "/api/me/unfreeze",
    tag = "payments",
    security(("cookie_auth" = [])),
    responses(
        (status = 200, description = "Unfreeze order created", body = UnfreezeOrderResponse),
        (status = 401, description = "Authentication required", body = ApiErrorResponse),
        (status = 500, description = "Payment service unavailable", body = ApiErrorResponse),
    )
)]
pub async fn create_unfreeze_order(
    auth_user: AuthenticatedUser,
    payment_service: web::Data<Arc<crate::payment::PaymentService>>,
    config: web::Data<Config>,
    redis: web::Data<Option<RedisClient>>,
    db: web::Data<sea_orm::DatabaseConnection>,
) -> HttpResponse {
    if !payment_service.is_configured() {
        return AppError::Internal("Payment service is not configured".into()).error_response();
    }

    let topup_repo = TopupTransactionRepository::new(db.get_ref().clone());
    let spent_cents = match topup_repo
        .sum_amount_eur_cents_since(auth_user.user_id, current_month_start(chrono::Utc::now()))
        .await
    {
        Ok(cents) => cents,
        Err(e) => return AppError::Database(e).error_response(),
    };
    let upcoming_cents = parse_eur_to_cents(&config.paypal_unfreeze_amount_eur);
    if monthly_limit_exceeded(
        spent_cents,
        upcoming_cents,
        config.topup_monthly_limit_eur_cents,
    ) {
        return AppError::BadRequest("payment.monthly_limit_reached").error_response();
    }

    let return_url = format!(
        "{}/api/paypal/return",
        config.frontend_url.trim_end_matches('/')
    );
    let cancel_url = format!(
        "{}/api/paypal/cancel",
        config.frontend_url.trim_end_matches('/')
    );

    let create_start = Instant::now();
    let order = match payment_service.create_order(&return_url, &cancel_url).await {
        Ok(o) => o,
        Err(e) => {
            tracing::error!(
                "PayPal create order failed for user {}: {}",
                auth_user.user_id,
                e
            );
            PAYMENT_UNFREEZE_TOTAL.with_label_values(&["failed"]).inc();
            PAYMENT_UNFREEZE_DURATION_SECONDS
                .with_label_values(&["create_order"])
                .observe(create_start.elapsed().as_secs_f64());
            return AppError::Internal("Failed to create payment order".into()).error_response();
        }
    };
    PAYMENT_UNFREEZE_TOTAL.with_label_values(&["created"]).inc();
    PAYMENT_UNFREEZE_DURATION_SECONDS
        .with_label_values(&["create_order"])
        .observe(create_start.elapsed().as_secs_f64());

    if let Some(mut redis_client) = redis.get_ref().clone() {
        let order_key = format!("{}:{}", UNFREEZE_ORDER_PREFIX, order.order_id);
        let _ = redis_client
            .set_ex(
                &order_key,
                &auth_user.user_id.to_string(),
                UNFREEZE_TTL_SECS,
            )
            .await;
    }

    HttpResponse::Ok().json(UnfreezeOrderResponse {
        order_id: order.order_id,
        approval_url: order.approval_url,
    })
}

#[utoipa::path(
    post,
    path = "/api/me/unfreeze/capture",
    tag = "payments",
    security(("cookie_auth" = [])),
    request_body = CaptureOrderRequest,
    responses(
        (status = 200, description = "Payment captured, account unfrozen", body = UnfreezeCaptureResponse),
        (status = 401, description = "Authentication required", body = ApiErrorResponse),
        (status = 500, description = "Capture failed", body = ApiErrorResponse),
    )
)]
pub async fn capture_unfreeze_order(
    auth_user: AuthenticatedUser,
    body: web::Json<CaptureOrderRequest>,
    payment_service: web::Data<Arc<crate::payment::PaymentService>>,
    redis: web::Data<Option<RedisClient>>,
    db: web::Data<sea_orm::DatabaseConnection>,
    config: web::Data<Config>,
) -> HttpResponse {
    if !payment_service.is_configured() {
        return AppError::Internal("Payment service is not configured".into()).error_response();
    }

    let order_id = &body.order_id;
    let redis_key = format!(
        "{}:{}:{}",
        UNFREEZE_CAPTURE_PREFIX, auth_user.user_id, order_id
    );
    let paypal_idem_key = format!("{}_{}", UNFREEZE_IDEM_PREFIX, order_id);

    let mut redis_opt = redis.get_ref().clone();
    if let Some(ref mut rc) = redis_opt {
        match rc.get(&redis_key).await {
            Ok(Some(ref cached)) if cached == "completed" => {
                return HttpResponse::Ok().json(UnfreezeCaptureResponse {
                    success: true,
                    message: "Account unfrozen. Welcome back!".into(),
                });
            }
            Ok(Some(ref cached)) if cached == "processing" => {
                return unfreeze_user_and_finalize(
                    auth_user.user_id,
                    &db,
                    redis_opt.as_mut(),
                    &redis_key,
                    config.unfreeze_credit_with_payment,
                    parse_eur_to_cents(&config.paypal_unfreeze_amount_eur),
                )
                .await;
            }
            _ => {}
        }
    }

    let capture_start = Instant::now();
    let result = match payment_service
        .capture_order(order_id, Some(&paypal_idem_key))
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(
                "PayPal capture failed for user {} order {}: {}",
                auth_user.user_id,
                order_id,
                e
            );
            PAYMENT_UNFREEZE_TOTAL.with_label_values(&["failed"]).inc();
            PAYMENT_UNFREEZE_DURATION_SECONDS
                .with_label_values(&["capture_order"])
                .observe(capture_start.elapsed().as_secs_f64());
            return AppError::Internal("Failed to capture payment".into()).error_response();
        }
    };

    PAYMENT_UNFREEZE_DURATION_SECONDS
        .with_label_values(&["capture_order"])
        .observe(capture_start.elapsed().as_secs_f64());

    if !result.success {
        PAYMENT_UNFREEZE_TOTAL.with_label_values(&["failed"]).inc();
        return AppError::Internal("Payment was not successful".into()).error_response();
    }

    if let Some(ref mut rc) = redis_opt {
        let _ = rc.set_ex(&redis_key, "processing", UNFREEZE_TTL_SECS).await;
    }

    unfreeze_user_and_finalize(
        auth_user.user_id,
        &db,
        redis_opt.as_mut(),
        &redis_key,
        config.unfreeze_credit_with_payment,
        parse_eur_to_cents(&config.paypal_unfreeze_amount_eur),
    )
    .await
}

#[utoipa::path(
    get,
    path = "/api/paypal/return",
    tag = "payments",
    params(("token" = String, Query, description = "PayPal order token")),
    responses((status = 200, description = "Payment result page", content_type = "text/html"))
)]
pub async fn paypal_return(
    req: HttpRequest,
    payment_service: web::Data<Arc<crate::payment::PaymentService>>,
    redis: web::Data<Option<RedisClient>>,
    db: web::Data<sea_orm::DatabaseConnection>,
    config: web::Data<Config>,
) -> HttpResponse {
    let query =
        web::Query::<std::collections::HashMap<String, String>>::from_query(req.query_string())
            .ok()
            .unwrap_or_else(|| {
                let mut map = std::collections::HashMap::new();
                map.insert("token".to_string(), String::new());
                web::Query(map)
            });
    let order_id = match query.get("token") {
        Some(id) => id.clone(),
        None => return close_window_html("Payment Error — missing order ID"),
    };

    let mut redis_opt = redis.get_ref().clone();
    let user_id = match redis_opt.as_mut() {
        Some(rc) => {
            let order_key = format!("{}:{}", UNFREEZE_ORDER_PREFIX, order_id);
            match rc.get(&order_key).await {
                Ok(Some(uid_str)) => Uuid::parse_str(&uid_str).ok(),
                _ => None,
            }
        }
        None => None,
    };

    let user_id = match user_id {
        Some(uid) => uid,
        None => return close_window_html("Payment Error — session expired"),
    };

    let redis_key = format!("{}:{}:{}", UNFREEZE_CAPTURE_PREFIX, user_id, order_id);
    let paypal_idem_key = format!("{}_{}", UNFREEZE_IDEM_PREFIX, order_id);

    if let Some(ref mut rc) = redis_opt {
        if let Ok(Some(ref cached)) = rc.get(&redis_key).await {
            if cached == "completed" {
                return close_window_html("Payment Complete — Account Unfrozen");
            }
        }
    }

    if let Some(ref mut rc) = redis_opt {
        let _ = rc.set_ex(&redis_key, "processing", UNFREEZE_TTL_SECS).await;
    }

    let capture_start = Instant::now();
    let capture_ok = match payment_service
        .capture_order(&order_id, Some(&paypal_idem_key))
        .await
    {
        Ok(r) => r.success,
        Err(e) => {
            tracing::error!(
                "PayPal capture on return failed for order {}: {}",
                order_id,
                e
            );
            false
        }
    };

    PAYMENT_UNFREEZE_DURATION_SECONDS
        .with_label_values(&["capture_order"])
        .observe(capture_start.elapsed().as_secs_f64());

    if !capture_ok {
        PAYMENT_UNFREEZE_TOTAL.with_label_values(&["failed"]).inc();
        if let Some(ref mut rc) = redis_opt {
            let _ = rc.del(&redis_key).await;
        }
        return close_window_html("Payment Error — capture failed");
    }

    match finalize_unfreeze(
        user_id,
        &db,
        config.unfreeze_credit_with_payment,
        parse_eur_to_cents(&config.paypal_unfreeze_amount_eur),
    )
    .await
    {
        Ok(_) => {
            if let Some(ref mut rc) = redis_opt {
                let _ = rc.set_ex(&redis_key, "completed", UNFREEZE_TTL_SECS).await;
            }
            PAYMENT_UNFREEZE_TOTAL
                .with_label_values(&["captured"])
                .inc();
            close_window_html("Payment Complete — Account Unfrozen")
        }
        Err(e) => {
            tracing::error!(
                "Failed to finalize unfreeze for user {} after payment on return: {}",
                user_id,
                e
            );
            close_window_error_html("Payment Error — could not finalize unfreeze (contact support)")
        }
    }
}

#[utoipa::path(
    get,
    path = "/api/paypal/cancel",
    tag = "payments",
    responses((status = 200, description = "Payment cancelled page", content_type = "text/html"))
)]
pub async fn paypal_cancel() -> HttpResponse {
    close_window_html("Payment Cancelled")
}

async fn unfreeze_user_and_finalize(
    user_id: Uuid,
    db: &web::Data<sea_orm::DatabaseConnection>,
    redis_client: Option<&mut RedisClient>,
    redis_key: &str,
    credit: i32,
    amount_eur_cents: i32,
) -> HttpResponse {
    match finalize_unfreeze(user_id, db, credit, amount_eur_cents).await {
        Ok(_) => {
            if let Some(rc) = redis_client {
                let _ = rc.set_ex(redis_key, "completed", UNFREEZE_TTL_SECS).await;
            }
            PAYMENT_UNFREEZE_TOTAL
                .with_label_values(&["captured"])
                .inc();
            HttpResponse::Ok().json(UnfreezeCaptureResponse {
                success: true,
                message: "Account unfrozen. Welcome back!".into(),
            })
        }
        Err(e) => e.error_response(),
    }
}

/// Record a successful unfreeze payment and atomically set the user's credit
/// and unfreeze their account in a single database transaction.
///
/// The ledger row (`topup_transactions`) and the profile credit/freeze mutation
/// commit together: if either fails, the whole transaction rolls back, so the
/// monthly spending cap is never charged for a payment whose credits were not
/// applied. The Redis "completed" flag is a best-effort cache only.
async fn finalize_unfreeze(
    user_id: Uuid,
    db: &web::Data<sea_orm::DatabaseConnection>,
    credit: i32,
    amount_eur_cents: i32,
) -> Result<i32, AppError> {
    let topup_repo = TopupTransactionRepository::new(db.get_ref().clone());
    let profile_repo = PlayerProfileRepository::new(db.get_ref().clone());

    let txn = db.begin().await.map_err(|e| {
        tracing::error!(
            user_id = %user_id,
            error = %e,
            "failed to begin unfreeze database transaction"
        );
        AppError::Database(e)
    })?;

    if let Err(e) = topup_repo
        .insert_in_txn(
            &txn,
            user_id,
            TopupTransactionKind::Unfreeze,
            amount_eur_cents,
            credit,
        )
        .await
    {
        tracing::error!(
            user_id = %user_id,
            error = %e,
            "failed to record unfreeze transaction; transaction will roll back"
        );
        return Err(AppError::Database(e));
    }

    let profile = profile_repo
        .find_by_user_id_in_txn(&txn, user_id)
        .await
        .map_err(|e| {
            tracing::error!(
                user_id = %user_id,
                error = %e,
                "failed to read player profile during unfreeze; transaction will roll back"
            );
            AppError::Database(e)
        })?;
    if profile.is_none() {
        tracing::error!(
            user_id = %user_id,
            "player profile not found during unfreeze; transaction will roll back"
        );
        return Err(AppError::NotFound("payment.profile_not_found"));
    }

    if let Err(e) = profile_repo
        .set_credit_and_unfreeze_in_txn(&txn, user_id, credit, chrono::Utc::now())
        .await
    {
        tracing::error!(
            user_id = %user_id,
            error = %e,
            "failed to unfreeze user; transaction will roll back"
        );
        return Err(AppError::Database(e));
    }

    txn.commit().await.map_err(|e| {
        tracing::error!(
            user_id = %user_id,
            error = %e,
            "failed to commit unfreeze transaction"
        );
        AppError::Database(e)
    })?;

    Ok(credit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::models::{PlayerProfile, PlayerType};
    use sea_orm::{DatabaseBackend, MockDatabase, MockExecResult};
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
}
