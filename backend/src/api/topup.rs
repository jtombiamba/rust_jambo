use actix_web::{web, HttpRequest, HttpResponse, ResponseError};
use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;

use crate::api::dto::requests::CaptureOrderRequest;
use crate::api::dto::responses::{TopupCaptureResponse, TopupOrderResponse};
use crate::api::unfreeze::close_window_html;
use crate::auth::extractors::AuthenticatedUser;
use crate::config::Config;
use crate::error::AppError;
use crate::messaging::RedisClient;
use crate::observability::metrics::{PAYMENT_TOPUP_DURATION_SECONDS, PAYMENT_TOPUP_TOTAL};

const TOPUP_CAPTURE_PREFIX: &str = "topup_capture";
const TOPUP_ORDER_PREFIX: &str = "topup_order";
const TOPUP_TTL_SECS: u64 = 86400;
const TOPUP_IDEM_PREFIX: &str = "topup";

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
            return AppError::NotFound("Player profile not found".into()).error_response();
        }
        Err(e) => return AppError::Database(e).error_response(),
    };

    if let Some(frozen_until) = profile.frozen_until {
        if frozen_until > chrono::Utc::now() {
            return AppError::Forbidden("Account is frozen, cannot top up".into()).error_response();
        }
    }

    if profile.credit >= config.topup_credit_threshold {
        return AppError::BadRequest("Credit is already sufficient, top up not needed".into())
            .error_response();
    }

    if profile.credit <= 0 {
        return AppError::BadRequest("Credit depleted, use unfreeze instead".into())
            .error_response();
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
        let _ = redis_client
            .set_ex(&order_key, &auth_user.user_id.to_string(), TOPUP_TTL_SECS)
            .await;
    }

    HttpResponse::Ok().json(TopupOrderResponse {
        order_id: order.order_id,
        approval_url: order.approval_url,
    })
}

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

    let mut redis_opt = redis.get_ref().clone();
    if let Some(ref mut rc) = redis_opt {
        match rc.get(&redis_key).await {
            Ok(Some(ref cached)) if cached == "completed" => {
                let profile_repo = crate::database::repositories::PlayerProfileRepository::new(
                    db.get_ref().clone(),
                );
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
            Ok(Some(ref cached)) if cached == "processing" => {
                return topup_user_and_finalize(
                    auth_user.user_id,
                    &db,
                    redis_opt.as_mut(),
                    &redis_key,
                    config.topup_credit_amount,
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

    if let Some(ref mut rc) = redis_opt {
        let _ = rc.set_ex(&redis_key, "processing", TOPUP_TTL_SECS).await;
    }

    topup_user_and_finalize(
        auth_user.user_id,
        &db,
        redis_opt.as_mut(),
        &redis_key,
        config.topup_credit_amount,
    )
    .await
}

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

    if let Some(ref mut rc) = redis_opt {
        if let Ok(Some(ref cached)) = rc.get(&redis_key).await {
            if cached == "completed" {
                return close_window_html("Payment Complete — Credits Added");
            }
        }
    }

    if let Some(ref mut rc) = redis_opt {
        let _ = rc.set_ex(&redis_key, "processing", TOPUP_TTL_SECS).await;
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
        if let Some(ref mut rc) = redis_opt {
            let _ = rc.del(&redis_key).await;
        }
        return close_window_html("Payment Error — capture failed");
    }

    let profile_repo =
        crate::database::repositories::PlayerProfileRepository::new(db.get_ref().clone());
    let profile = match profile_repo.find_by_user_id(user_id).await {
        Ok(Some(p)) => p,
        _ => return close_window_html("Payment Error — profile not found"),
    };

    let new_credit = profile.credit + config.topup_credit_amount;

    match profile_repo
        .update_credit_and_frozen_until(user_id, new_credit, profile.frozen_until)
        .await
    {
        Ok(_) => {
            if let Some(ref mut rc) = redis_opt {
                let _ = rc.set_ex(&redis_key, "completed", TOPUP_TTL_SECS).await;
            }
            PAYMENT_TOPUP_TOTAL.with_label_values(&["captured"]).inc();
            close_window_html("Payment Complete — Credits Added")
        }
        Err(e) => {
            tracing::error!("Failed to top up user {} after payment: {}", user_id, e);
            close_window_html("Payment Complete — top up in progress (retry if needed)")
        }
    }
}

pub async fn paypal_cancel_topup() -> HttpResponse {
    close_window_html("Payment Cancelled")
}

async fn topup_user_and_finalize(
    user_id: Uuid,
    db: &web::Data<sea_orm::DatabaseConnection>,
    redis_client: Option<&mut RedisClient>,
    redis_key: &str,
    credit_add: i32,
) -> HttpResponse {
    let profile_repo =
        crate::database::repositories::PlayerProfileRepository::new(db.get_ref().clone());
    let profile = match profile_repo.find_by_user_id(user_id).await {
        Ok(Some(p)) => p,
        Ok(None) => {
            return AppError::NotFound("Player profile not found".into()).error_response();
        }
        Err(e) => return AppError::Database(e).error_response(),
    };

    let new_credit = profile.credit + credit_add;
    match profile_repo
        .update_credit_and_frozen_until(user_id, new_credit, profile.frozen_until)
        .await
    {
        Ok(_) => {
            if let Some(rc) = redis_client {
                // Invalidate the dashboard profile cache so the user sees their updated credit immediately
                let _ = rc.del(&format!("dashboard:profile:{user_id}")).await;
                let _ = rc.set_ex(redis_key, "completed", TOPUP_TTL_SECS).await;
            }
            PAYMENT_TOPUP_TOTAL.with_label_values(&["captured"]).inc();
            HttpResponse::Ok().json(TopupCaptureResponse {
                success: true,
                message: "Credits topped up!".into(),
                credit: new_credit,
            })
        }
        Err(e) => {
            tracing::error!("Failed to top up user {} after payment: {}", user_id, e);
            AppError::Database(e).error_response()
        }
    }
}
