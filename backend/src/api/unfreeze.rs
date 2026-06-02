use actix_web::{web, HttpRequest, HttpResponse, ResponseError};
use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;

use crate::api::dto::requests::CaptureOrderRequest;
use crate::api::dto::responses::{UnfreezeCaptureResponse, UnfreezeOrderResponse};
use crate::auth::extractors::AuthenticatedUser;
use crate::config::Config;
use crate::error::AppError;
use crate::messaging::RedisClient;
use crate::observability::metrics::{PAYMENT_UNFREEZE_DURATION_SECONDS, PAYMENT_UNFREEZE_TOTAL};

const UNFREEZE_CAPTURE_PREFIX: &str = "unfreeze_capture";
const UNFREEZE_ORDER_PREFIX: &str = "unfreeze_order";
const UNFREEZE_TTL_SECS: u64 = 86400;
const UNFREEZE_IDEM_PREFIX: &str = "unfreeze";

pub(crate) fn close_window_html(title: &str) -> HttpResponse {
    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(format!(
            "<!DOCTYPE html><html><head><title>{}</title></head>\
             <body style=\"font-family:sans-serif;text-align:center;padding-top:40px\">\
             <p>{}</p><p>This window will close automatically.</p>\
             <script>window.close();</script></body></html>",
            title, title
        ))
}

pub async fn create_unfreeze_order(
    auth_user: AuthenticatedUser,
    payment_service: web::Data<Arc<crate::payment::PaymentService>>,
    config: web::Data<Config>,
    redis: web::Data<Option<RedisClient>>,
) -> HttpResponse {
    if !payment_service.is_configured() {
        return AppError::Internal("Payment service is not configured".into()).error_response();
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
    )
    .await
}

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

    let profile_repo =
        crate::database::repositories::PlayerProfileRepository::new(db.get_ref().clone());
    match profile_repo
        .update_credit_and_frozen_until(user_id, config.unfreeze_credit_with_payment, None)
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
                "Failed to unfreeze user {} after payment on return: {}",
                user_id,
                e
            );
            close_window_html("Payment Complete — unfreeze in progress (retry if needed)")
        }
    }
}

pub async fn paypal_cancel() -> HttpResponse {
    close_window_html("Payment Cancelled")
}

async fn unfreeze_user_and_finalize(
    user_id: Uuid,
    db: &web::Data<sea_orm::DatabaseConnection>,
    redis_client: Option<&mut RedisClient>,
    redis_key: &str,
    credit: i32,
) -> HttpResponse {
    let profile_repo =
        crate::database::repositories::PlayerProfileRepository::new(db.get_ref().clone());
    match profile_repo
        .update_credit_and_frozen_until(user_id, credit, None)
        .await
    {
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
        Err(e) => {
            tracing::error!("Failed to unfreeze user {} after payment: {}", user_id, e);
            AppError::Database(e).error_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

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
}
