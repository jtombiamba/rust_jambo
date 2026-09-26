use actix_web::{web, HttpRequest, HttpResponse, ResponseError};
use sea_orm::TransactionTrait;
use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;

use crate::api::dto::requests::CaptureOrderRequest;
use crate::api::dto::responses::{ApiErrorResponse, TopupCaptureResponse, TopupOrderResponse};
use crate::api::payment_limits::{current_month_start, monthly_limit_exceeded, parse_eur_to_cents};
use crate::api::unfreeze::close_window_html;
use crate::auth::extractors::AuthenticatedUser;
use crate::config::Config;
use crate::database::models::TopupTransactionKind;
use crate::database::repositories::{PlayerProfileRepository, TopupTransactionRepository};
use crate::error::AppError;
use crate::messaging::RedisClient;
use crate::observability::metrics::{
    GAME_STATE_CACHE_WRITE_ERRORS_TOTAL, PAYMENT_TOPUP_DURATION_SECONDS, PAYMENT_TOPUP_TOTAL,
};

const TOPUP_CAPTURE_PREFIX: &str = "topup_capture";
const TOPUP_ORDER_PREFIX: &str = "topup_order";
const TOPUP_TTL_SECS: u64 = 86400;
const TOPUP_IDEM_PREFIX: &str = "topup";

#[utoipa::path(
    post,
    path = "/api/me/topup",
    tag = "payments",
    security(("cookie_auth" = [])),
    responses(
        (status = 200, description = "Top-up order created", body = TopupOrderResponse),
        (status = 400, description = "Top-up not needed", body = ApiErrorResponse),
        (status = 401, description = "Authentication required", body = ApiErrorResponse),
        (status = 403, description = "Account frozen", body = ApiErrorResponse),
    )
)]
pub async fn create_topup_order(
    auth_user: AuthenticatedUser,
    payment_service: web::Data<Arc<crate::payment::PaymentService>>,
    config: web::Data<Config>,
    redis: web::Data<Option<RedisClient>>,
    db: web::Data<sea_orm::DatabaseConnection>,
) -> HttpResponse {
    if !payment_service.is_configured() {
        return AppError::Internal("Payment service is not configured".into()).error_response();
    }

    let profile_repo =
        crate::database::repositories::PlayerProfileRepository::new(db.get_ref().clone());
    let profile = match profile_repo.find_by_user_id(auth_user.user_id).await {
        Ok(Some(p)) => p,
        Ok(None) => {
            return AppError::NotFound("payment.profile_not_found").error_response();
        }
        Err(e) => return AppError::Database(e).error_response(),
    };

    if let Some(frozen_until) = profile.frozen_until {
        if frozen_until > chrono::Utc::now() {
            return AppError::Forbidden("payment.account_frozen_no_topup").error_response();
        }
    }

    if profile.credit <= 0 {
        return AppError::BadRequest("payment.credit_depleted").error_response();
    }

    let topup_repo = TopupTransactionRepository::new(db.get_ref().clone());
    let spent_cents = match topup_repo
        .sum_amount_eur_cents_since(auth_user.user_id, current_month_start(chrono::Utc::now()))
        .await
    {
        Ok(cents) => cents,
        Err(e) => return AppError::Database(e).error_response(),
    };
    let upcoming_cents = parse_eur_to_cents(&config.paypal_topup_amount_eur);
    if monthly_limit_exceeded(
        spent_cents,
        upcoming_cents,
        config.topup_monthly_limit_eur_cents,
    ) {
        return AppError::BadRequest("payment.monthly_limit_reached").error_response();
    }

    let return_url = format!(
        "{}/api/paypal/topup/return",
        config.frontend_url.trim_end_matches('/')
    );
    let cancel_url = format!(
        "{}/api/paypal/topup/cancel",
        config.frontend_url.trim_end_matches('/')
    );

    let create_start = Instant::now();
    let order = match payment_service
        .create_topup_order(&return_url, &cancel_url)
        .await
    {
        Ok(o) => o,
        Err(e) => {
            tracing::error!(
                "PayPal create topup order failed for user {}: {}",
                auth_user.user_id,
                e
            );
            PAYMENT_TOPUP_TOTAL.with_label_values(&["failed"]).inc();
            PAYMENT_TOPUP_DURATION_SECONDS
                .with_label_values(&["create_order"])
                .observe(create_start.elapsed().as_secs_f64());
            return AppError::Internal("Failed to create payment order".into()).error_response();
        }
    };
    PAYMENT_TOPUP_TOTAL.with_label_values(&["created"]).inc();
    PAYMENT_TOPUP_DURATION_SECONDS
        .with_label_values(&["create_order"])
        .observe(create_start.elapsed().as_secs_f64());

    if let Some(mut redis_client) = redis.get_ref().clone() {
        let order_key = format!("{}:{}", TOPUP_ORDER_PREFIX, order.order_id);
        if let Err(e) = redis_client
            .set_ex(&order_key, &auth_user.user_id.to_string(), TOPUP_TTL_SECS)
            .await
        {
            tracing::warn!(
                user_id = %auth_user.user_id,
                order_id = %order.order_id,
                error = %e,
                "failed to persist topup order to user mapping in redis"
            );
        }
    }

    HttpResponse::Ok().json(TopupOrderResponse {
        order_id: order.order_id,
        approval_url: order.approval_url,
    })
}

#[utoipa::path(
    post,
    path = "/api/me/topup/capture",
    tag = "payments",
    security(("cookie_auth" = [])),
    request_body = CaptureOrderRequest,
    responses(
        (status = 200, description = "Credits topped up", body = TopupCaptureResponse),
        (status = 401, description = "Authentication required", body = ApiErrorResponse),
        (status = 500, description = "Capture failed", body = ApiErrorResponse),
    )
)]
pub async fn capture_topup_order(
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
        TOPUP_CAPTURE_PREFIX, auth_user.user_id, order_id
    );
    let paypal_idem_key = format!("{}_{}", TOPUP_IDEM_PREFIX, order_id);
    let credit_add = config.topup_credit_amount;
    let amount_eur_cents = parse_eur_to_cents(&config.paypal_topup_amount_eur);

    let topup_repo = TopupTransactionRepository::new(db.get_ref().clone());

    // Database-level idempotency: replaying an already-completed order is a
    // no-op regardless of Redis availability.
    match topup_repo.exists_by_order_id(order_id).await {
        Ok(true) => {
            let profile_repo = PlayerProfileRepository::new(db.get_ref().clone());
            let credit = profile_repo
                .find_by_user_id(auth_user.user_id)
                .await
                .ok()
                .flatten()
                .map(|p| p.credit)
                .unwrap_or(0);
            return HttpResponse::Ok().json(TopupCaptureResponse {
                success: true,
                message: "Credits already added!".into(),
                credit,
            });
        }
        Ok(false) => {}
        Err(e) => {
            tracing::error!(
                user_id = %auth_user.user_id,
                order_id = %order_id,
                error = %e,
                "failed to check topup order idempotency"
            );
            return AppError::Database(e).error_response();
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
            PAYMENT_TOPUP_TOTAL.with_label_values(&["failed"]).inc();
            PAYMENT_TOPUP_DURATION_SECONDS
                .with_label_values(&["capture_order"])
                .observe(capture_start.elapsed().as_secs_f64());
            return AppError::Internal("Failed to capture payment".into()).error_response();
        }
    };

    PAYMENT_TOPUP_DURATION_SECONDS
        .with_label_values(&["capture_order"])
        .observe(capture_start.elapsed().as_secs_f64());

    if !result.success {
        PAYMENT_TOPUP_TOTAL.with_label_values(&["failed"]).inc();
        return AppError::Internal("Payment was not successful".into()).error_response();
    }

    let mut redis_opt = redis.get_ref().clone();
    match finalize_topup(
        &db,
        redis_opt.as_mut(),
        &redis_key,
        auth_user.user_id,
        order_id,
        credit_add,
        amount_eur_cents,
    )
    .await
    {
        Ok(credit) => {
            PAYMENT_TOPUP_TOTAL.with_label_values(&["captured"]).inc();
            HttpResponse::Ok().json(TopupCaptureResponse {
                success: true,
                message: "Credits topped up!".into(),
                credit,
            })
        }
        Err(e) => e.error_response(),
    }
}

#[utoipa::path(
    get,
    path = "/api/paypal/topup/return",
    tag = "payments",
    params(("token" = String, Query, description = "PayPal order token")),
    responses((status = 200, description = "Top-up result page", content_type = "text/html"))
)]
pub async fn paypal_return_topup(
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
            let order_key = format!("{}:{}", TOPUP_ORDER_PREFIX, order_id);
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

    let redis_key = format!("{}:{}:{}", TOPUP_CAPTURE_PREFIX, user_id, order_id);
    let paypal_idem_key = format!("{}_{}", TOPUP_IDEM_PREFIX, order_id);
    let credit_add = config.topup_credit_amount;
    let amount_eur_cents = parse_eur_to_cents(&config.paypal_topup_amount_eur);

    let topup_repo = TopupTransactionRepository::new(db.get_ref().clone());

    // Database-level idempotency: a replay of an already-completed order (e.g.
    // after the Redis TTL expires or Redis restarts) must not double-credit.
    match topup_repo.exists_by_order_id(&order_id).await {
        Ok(true) => return close_window_html("Payment Complete — Credits Added"),
        Ok(false) => {}
        Err(e) => {
            tracing::error!(
                order_id = %order_id,
                error = %e,
                "failed to check topup order idempotency"
            );
            return close_window_html("Payment Error — unable to verify payment status");
        }
    }

    let capture_start = Instant::now();
    let capture_ok = match payment_service
        .capture_order(&order_id, Some(&paypal_idem_key))
        .await
    {
        Ok(r) => r.success,
        Err(e) => {
            tracing::error!(
                "PayPal capture on return for topup failed for order {}: {}",
                order_id,
                e
            );
            false
        }
    };

    PAYMENT_TOPUP_DURATION_SECONDS
        .with_label_values(&["capture_order"])
        .observe(capture_start.elapsed().as_secs_f64());

    if !capture_ok {
        PAYMENT_TOPUP_TOTAL.with_label_values(&["failed"]).inc();
        return close_window_html("Payment Error — capture failed");
    }

    match finalize_topup(
        &db,
        redis_opt.as_mut(),
        &redis_key,
        user_id,
        &order_id,
        credit_add,
        amount_eur_cents,
    )
    .await
    {
        Ok(_) => {
            PAYMENT_TOPUP_TOTAL.with_label_values(&["captured"]).inc();
            close_window_html("Payment Complete — Credits Added")
        }
        Err(_) => close_window_html("Payment Complete — top up in progress (retry if needed)"),
    }
}

#[utoipa::path(
    get,
    path = "/api/paypal/topup/cancel",
    tag = "payments",
    responses((status = 200, description = "Top-up cancelled page", content_type = "text/html"))
)]
pub async fn paypal_cancel_topup() -> HttpResponse {
    close_window_html("Payment Cancelled")
}

/// Record a successful top-up and atomically credit the user in a single
/// database transaction.
///
/// Idempotency is enforced by the `UNIQUE` constraint on `topup_transactions.
/// order_id`: if a row with this `order_id` already exists, the insert is a
/// no-op and no credit is applied. The Redis "completed" flag and dashboard
/// cache invalidation are best-effort optimisations only; they never gate the
/// credit.
async fn finalize_topup(
    db: &web::Data<sea_orm::DatabaseConnection>,
    redis_client: Option<&mut RedisClient>,
    redis_key: &str,
    user_id: Uuid,
    order_id: &str,
    credit_add: i32,
    amount_eur_cents: i32,
) -> Result<i32, AppError> {
    let topup_repo = TopupTransactionRepository::new(db.get_ref().clone());
    let profile_repo = PlayerProfileRepository::new(db.get_ref().clone());

    let txn = db.begin().await.map_err(|e| {
        tracing::error!(
            user_id = %user_id,
            order_id = %order_id,
            error = %e,
            "failed to begin topup database transaction"
        );
        AppError::Database(e)
    })?;

    match topup_repo
        .insert_order_if_absent_in_txn(
            &txn,
            user_id,
            TopupTransactionKind::Topup,
            amount_eur_cents,
            credit_add,
            order_id,
        )
        .await
    {
        Ok(Some(_)) => {
            let profile = profile_repo
                .find_by_user_id_in_txn(&txn, user_id)
                .await
                .map_err(|e| {
                    tracing::error!(
                        user_id = %user_id,
                        order_id = %order_id,
                        error = %e,
                        "failed to read player profile during topup; transaction will roll back"
                    );
                    AppError::Database(e)
                })?;
            if profile.is_none() {
                tracing::error!(
                    user_id = %user_id,
                    order_id = %order_id,
                    "player profile not found during topup; transaction will roll back"
                );
                return Err(AppError::NotFound("payment.profile_not_found"));
            }

            if let Err(e) = profile_repo
                .credit_in_txn(&txn, user_id, credit_add, chrono::Utc::now())
                .await
            {
                tracing::error!(
                    user_id = %user_id,
                    order_id = %order_id,
                    error = %e,
                    "failed to credit user after topup; transaction will roll back"
                );
                return Err(AppError::Database(e));
            }
        }
        Ok(None) => {
            tracing::info!(
                user_id = %user_id,
                order_id = %order_id,
                "topup order already recorded; skipping credit"
            );
        }
        Err(e) => {
            tracing::error!(
                user_id = %user_id,
                order_id = %order_id,
                error = %e,
                "failed to record topup transaction; transaction will roll back"
            );
            return Err(AppError::Database(e));
        }
    }

    txn.commit().await.map_err(|e| {
        tracing::error!(
            user_id = %user_id,
            order_id = %order_id,
            error = %e,
            "failed to commit topup transaction"
        );
        AppError::Database(e)
    })?;

    if let Some(rc) = redis_client {
        if let Err(e) = rc.del(&format!("dashboard:profile:{user_id}")).await {
            tracing::warn!(
                user_id = %user_id,
                error = %e,
                "failed to invalidate profile cache after topup"
            );
            GAME_STATE_CACHE_WRITE_ERRORS_TOTAL.inc();
        }
        if let Err(e) = rc.set_ex(redis_key, "completed", TOPUP_TTL_SECS).await {
            tracing::warn!(
                user_id = %user_id,
                order_id = %order_id,
                error = %e,
                "failed to mark topup completed in redis"
            );
        }
    }

    let credit = profile_repo
        .find_by_user_id(user_id)
        .await
        .ok()
        .flatten()
        .map(|p| p.credit)
        .unwrap_or(credit_add);

    Ok(credit)
}

#[cfg(test)]
#[path = "topup_tests.rs"]
mod tests;
