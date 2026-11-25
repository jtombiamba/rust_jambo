use std::sync::Arc;

use actix_web::{web, HttpRequest, HttpResponse};
use uuid::Uuid;

use crate::api::topup::finalize_topup;
use crate::api::unfreeze::finalize_unfreeze;
use jambo_core::config::Config;
use jambo_core::error::AppError;
use jambo_core::i18n::Lang;
use jambo_core::payment::stripe::verify_stripe_signature;
use jambo_core::payment::PaymentServiceTrait;
use jambo_db::models::{evaluate_account_status, CashoutStatus, ConnectAccountStatus};
use jambo_db::repositories::{CashoutRepository, UserRepository};
use jambo_domain::messaging::RedisClient;
use jambo_mailer::{EmailJob, EmailQueue};

/// Stripe Checkout metadata keys used to resolve a completed session without a
/// Redis or database lookup.
pub(crate) const META_KIND: &str = "kind";
pub(crate) const META_AMOUNT_CENTS: &str = "amount_eur_cents";
pub(crate) const META_CREDITS: &str = "credits";
pub(crate) const META_VALUE_TOPUP: &str = "topup";
pub(crate) const META_VALUE_UNFREEZE: &str = "unfreeze";

const STRIPE_EVENT_CHECKOUT_COMPLETED: &str = "checkout.session.completed";
const STRIPE_EVENT_ACCOUNT_UPDATED: &str = "account.updated";
const STRIPE_EVENT_TRANSFER_CREATED: &str = "transfer.created";
const STRIPE_EVENT_TRANSFER_PAID: &str = "transfer.paid";
const STRIPE_EVENT_TRANSFER_FAILED: &str = "transfer.failed";

enum WebhookError {
    /// The event is malformed in a way that will never change on retry; ack it
    /// so Stripe stops retrying.
    Permanent(String),
    /// A transient failure (DB/Redis/provider down); let Stripe retry.
    Transient(String),
}

fn classify_app_error(e: AppError) -> WebhookError {
    match e {
        AppError::Database(_)
        | AppError::Internal(_)
        | AppError::ExternalService(_)
        | AppError::Serialization(_)
        | AppError::Config(_) => WebhookError::Transient(e.to_string()),
        AppError::Game(_)
        | AppError::Validation(_)
        | AppError::NotFound(_)
        | AppError::Forbidden(_)
        | AppError::Unauthorized(_)
        | AppError::Conflict(_)
        | AppError::BadRequest(_)
        | AppError::BadRequestParams { .. } => WebhookError::Permanent(e.to_string()),
    }
}

/// Stripe webhook receiver. Payment confirmation is webhook-driven for Stripe:
/// the redirect back to the app is display-only, and credits are applied here
/// only after the signature is verified and the session is completed.
///
/// Returns `400` only for an invalid signature; permanent malformed events are
/// acknowledged with `200` so Stripe stops retrying, while transient failures
/// return `500` so Stripe retries.
pub async fn stripe_webhook(
    req: HttpRequest,
    body: web::Bytes,
    config: web::Data<Config>,
    redis: web::Data<Option<RedisClient>>,
    db: web::Data<sea_orm::DatabaseConnection>,
    payment_service: web::Data<Arc<dyn PaymentServiceTrait>>,
    email_queue: web::Data<EmailQueue>,
) -> HttpResponse {
    let signature = match req.headers().get("Stripe-Signature") {
        Some(v) => match v.to_str() {
            Ok(s) => s.to_string(),
            Err(_) => return HttpResponse::BadRequest().body("invalid signature header"),
        },
        None => return HttpResponse::BadRequest().body("missing signature header"),
    };

    if let Err(e) = verify_stripe_signature(&body, &signature, &config.stripe_webhook_secret) {
        tracing::warn!(error = %e, "rejected stripe webhook with invalid signature");
        return HttpResponse::BadRequest().body("invalid signature");
    }

    let event: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "stripe webhook body is not valid json");
            return HttpResponse::BadRequest().body("invalid json");
        }
    };

    let event_type = event
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or_default();

    match event_type {
        STRIPE_EVENT_CHECKOUT_COMPLETED => {
            match handle_checkout_completed(&event, &db, redis.get_ref().clone()).await {
                Ok(()) => {}
                Err(WebhookError::Permanent(msg)) => {
                    tracing::warn!(error = %msg, "stripe checkout event permanently unprocessable; acknowledging");
                }
                Err(WebhookError::Transient(msg)) => {
                    tracing::error!(error = %msg, "failed to finalize stripe checkout session");
                    return HttpResponse::InternalServerError().body("finalization failed");
                }
            }
        }
        STRIPE_EVENT_ACCOUNT_UPDATED => {
            if let Err(e) = handle_account_updated(
                &event,
                &db,
                &config,
                payment_service.get_ref().as_ref(),
                email_queue.get_ref(),
            )
            .await
            {
                tracing::error!(error = %e, "failed to handle stripe account.updated event");
                return HttpResponse::InternalServerError().body("account update failed");
            }
        }
        STRIPE_EVENT_TRANSFER_CREATED
        | STRIPE_EVENT_TRANSFER_PAID
        | STRIPE_EVENT_TRANSFER_FAILED => {
            if let Err(e) = handle_transfer_event(&event, &db, &config, email_queue.get_ref()).await
            {
                tracing::error!(error = %e, "failed to handle stripe transfer event");
                return HttpResponse::InternalServerError().body("transfer event failed");
            }
        }
        other => {
            tracing::debug!(event = %other, "ignoring unhandled stripe webhook event");
        }
    }

    HttpResponse::Ok().body("{}")
}

async fn handle_checkout_completed(
    event: &serde_json::Value,
    db: &web::Data<sea_orm::DatabaseConnection>,
    mut redis: Option<RedisClient>,
) -> Result<(), WebhookError> {
    let session = event
        .get("data")
        .and_then(|d| d.get("object"))
        .ok_or_else(|| WebhookError::Permanent("missing data.object".to_string()))?;

    let order_id = session
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| WebhookError::Permanent("missing session id".to_string()))?;

    let user_id_str = session
        .get("client_reference_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| WebhookError::Permanent("missing client_reference_id".to_string()))?;
    let user_id = Uuid::parse_str(user_id_str)
        .map_err(|e| WebhookError::Permanent(format!("invalid client_reference_id: {e}")))?;

    let metadata = session.get("metadata").cloned().unwrap_or_default();
    let kind = metadata
        .get(META_KIND)
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let amount_eur_cents = metadata
        .get(META_AMOUNT_CENTS)
        .and_then(|v| v.as_str())
        .and_then(|v| v.parse::<i32>().ok())
        .ok_or_else(|| WebhookError::Permanent("missing metadata amount_eur_cents".to_string()))?;
    let credits = metadata
        .get(META_CREDITS)
        .and_then(|v| v.as_str())
        .and_then(|v| v.parse::<i32>().ok())
        .ok_or_else(|| WebhookError::Permanent("missing metadata credits".to_string()))?;

    match kind {
        META_VALUE_TOPUP => {
            let redis_key = format!("topup_capture:{user_id}:{order_id}");
            finalize_topup(
                db,
                redis.as_mut(),
                &redis_key,
                user_id,
                order_id,
                credits,
                amount_eur_cents,
            )
            .await
            .map_err(classify_app_error)?;
        }
        META_VALUE_UNFREEZE => {
            finalize_unfreeze(user_id, db, credits, amount_eur_cents)
                .await
                .map_err(classify_app_error)?;
        }
        other => {
            tracing::warn!(session = %order_id, kind = %other, "unknown checkout metadata kind");
        }
    }

    Ok(())
}

/// Re-evaluate a Connect account's onboarding/verification status from an
/// `account.updated` event and notify the user when it changes.
async fn handle_account_updated(
    event: &serde_json::Value,
    db: &web::Data<sea_orm::DatabaseConnection>,
    config: &Config,
    payment_service: &dyn PaymentServiceTrait,
    email_queue: &EmailQueue,
) -> Result<(), String> {
    let account_id = event
        .get("data")
        .and_then(|d| d.get("object"))
        .and_then(|o| o.get("id"))
        .and_then(|v| v.as_str())
        .unwrap_or_default();

    if account_id.is_empty() {
        tracing::warn!("account.updated event missing account id");
        return Ok(());
    }

    let user_repo = UserRepository::new(db.get_ref().clone(), config.default_credit);
    let user = match user_repo
        .find_by_stripe_connect_account_id(account_id)
        .await
        .map_err(|e| e.to_string())?
    {
        Some(user) => user,
        None => {
            tracing::debug!(account = %account_id, "account.updated for unknown connect account");
            return Ok(());
        }
    };

    let details = payment_service.retrieve_account(account_id).await?;

    let new_status = evaluate_account_status(&details, user.stripe_connect_account_status);
    if new_status == user.stripe_connect_account_status {
        return Ok(());
    }

    user_repo
        .set_stripe_connect_account_status(user.id, new_status)
        .await
        .map_err(|e| e.to_string())?;

    let lang = Lang::parse(&user.language).unwrap_or_default();
    let job = match new_status {
        ConnectAccountStatus::Verified => Some(EmailJob::ConnectAccountVerified {
            to_email: user.email.clone(),
            lang,
        }),
        ConnectAccountStatus::VerificationFailed => {
            Some(EmailJob::ConnectAccountVerificationFailed {
                to_email: user.email.clone(),
                lang,
            })
        }
        _ => None,
    };
    if let Some(job) = job {
        email_queue.enqueue(job);
    }

    Ok(())
}

/// Reconcile the outcome of a Stripe Transfer against the originating cashout
/// request. `transfer.paid` finalizes as paid, `transfer.failed` refunds the
/// reserved credits, and `transfer.created` records the reference.
async fn handle_transfer_event(
    event: &serde_json::Value,
    db: &web::Data<sea_orm::DatabaseConnection>,
    config: &Config,
    email_queue: &EmailQueue,
) -> Result<(), String> {
    let object = event
        .get("data")
        .and_then(|d| d.get("object"))
        .ok_or_else(|| "missing data.object".to_string())?;

    let item_id = object
        .get("metadata")
        .and_then(|m| m.get("item_id"))
        .and_then(|v| v.as_str())
        .unwrap_or_default();

    let Ok(request_id) = Uuid::parse_str(item_id) else {
        tracing::warn!(
            item_id,
            "transfer event missing valid metadata.item_id; ignoring"
        );
        return Ok(());
    };

    let cashout_repo = CashoutRepository::new(db.get_ref().clone());
    let request = match cashout_repo.find_by_id(request_id).await {
        Ok(Some(request)) => request,
        Ok(None) => {
            tracing::warn!(request_id = %request_id, "transfer event for unknown cashout request");
            return Ok(());
        }
        Err(e) => return Err(e.to_string()),
    };

    let event_type = event
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    match event_type {
        STRIPE_EVENT_TRANSFER_CREATED => {
            if request.status == CashoutStatus::Approved {
                let transfer_id = object
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                if !transfer_id.is_empty() {
                    cashout_repo
                        .mark_processing(request_id, transfer_id, "stripe-webhook")
                        .await
                        .map_err(|e| e.to_string())?;
                }
            }
        }
        STRIPE_EVENT_TRANSFER_PAID => match request.status {
            CashoutStatus::Paid => {}
            CashoutStatus::Processing => {
                cashout_repo
                    .mark_paid_by_id(request_id)
                    .await
                    .map_err(|e| e.to_string())?;
                notify_payout_result(
                    db,
                    config,
                    email_queue,
                    request.user_id,
                    request.amount_eur_cents,
                    true,
                )
                .await;
            }
            other => {
                tracing::warn!(request_id = %request_id, status = %other, "transfer.paid on unexpected status");
            }
        },
        STRIPE_EVENT_TRANSFER_FAILED => match request.status {
            CashoutStatus::Rejected => {}
            CashoutStatus::Processing => {
                cashout_repo
                    .fail_and_refund(request_id, "stripe transfer failed")
                    .await
                    .map_err(|e| e.to_string())?;
                notify_payout_result(
                    db,
                    config,
                    email_queue,
                    request.user_id,
                    request.amount_eur_cents,
                    false,
                )
                .await;
            }
            other => {
                tracing::warn!(request_id = %request_id, status = %other, "transfer.failed on unexpected status");
            }
        },
        other => {
            tracing::debug!(event = %other, "ignoring unhandled transfer event");
        }
    }

    Ok(())
}

async fn notify_payout_result(
    db: &web::Data<sea_orm::DatabaseConnection>,
    config: &Config,
    email_queue: &EmailQueue,
    user_id: Uuid,
    amount_eur_cents: i32,
    succeeded: bool,
) {
    let user_repo = UserRepository::new(db.get_ref().clone(), config.default_credit);
    match user_repo.find_by_id(user_id).await {
        Ok(Some(user)) => {
            let lang = Lang::parse(&user.language).unwrap_or_default();
            let job = if succeeded {
                EmailJob::CashoutPaid {
                    to_email: user.email.clone(),
                    amount_eur_cents,
                    lang,
                }
            } else {
                EmailJob::CashoutFailed {
                    to_email: user.email.clone(),
                    amount_eur_cents,
                    lang,
                }
            };
            email_queue.enqueue(job);
        }
        Ok(None) => {
            tracing::warn!(user_id = %user_id, "payout result for unknown user; notification skipped");
        }
        Err(e) => {
            tracing::error!(user_id = %user_id, error = %e, "failed to load user for payout notification");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::classify_app_error;
    use super::WebhookError;
    use jambo_core::error::AppError;

    #[test]
    fn classify_app_error_is_transient_for_internal() {
        assert!(matches!(
            classify_app_error(AppError::Internal("boom".to_string())),
            WebhookError::Transient(_)
        ));
    }

    #[test]
    fn classify_app_error_is_permanent_for_not_found() {
        assert!(matches!(
            classify_app_error(AppError::NotFound("payment.profile_not_found")),
            WebhookError::Permanent(_)
        ));
    }
}
