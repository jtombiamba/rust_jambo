use std::sync::Arc;

use actix_web::{web, HttpResponse, ResponseError};

use crate::api::dto::requests::CashoutRequest;
use crate::api::dto::responses::{
    ApiErrorResponse, CashoutHistoryResponse, CashoutRequestResponse, PayoutOnboardingResponse,
};
use crate::api::services::cashout_service::CashoutServiceTrait;
use crate::auth::extractors::AuthenticatedUser;
use jambo_core::config::Config;
use jambo_core::error::AppError;
use jambo_core::i18n::Lang;
use jambo_core::payment::{PaymentProviderKind, PaymentServiceTrait};
use jambo_db::models::{ConnectAccountStatus, PayoutMethod};
use jambo_db::repositories::UserRepository;
use jambo_mailer::{EmailJob, EmailQueue};

#[utoipa::path(
    post,
    path = "/api/me/cashout",
    tag = "payments",
    security(("cookie_auth" = [])),
    request_body = CashoutRequest,
    responses(
        (status = 200, description = "Cashout request created", body = CashoutRequestResponse),
        (status = 400, description = "Invalid request", body = ApiErrorResponse),
        (status = 401, description = "Authentication required", body = ApiErrorResponse),
        (status = 403, description = "Cashout disabled", body = ApiErrorResponse),
        (status = 409, description = "Account locked or insufficient credits", body = ApiErrorResponse),
    )
)]
pub async fn request_cashout(
    auth_user: AuthenticatedUser,
    body: web::Json<CashoutRequest>,
    service: web::Data<Arc<dyn CashoutServiceTrait>>,
    db: web::Data<sea_orm::DatabaseConnection>,
    email_queue: web::Data<EmailQueue>,
    config: web::Data<Config>,
) -> HttpResponse {
    let payout_method = match PayoutMethod::parse(&body.payout_method) {
        Some(m) => m,
        None => return AppError::BadRequest("cashout.invalid_payout_method").error_response(),
    };

    let payout_destination = match payout_method {
        PayoutMethod::StripeConnect => {
            let user_repo = UserRepository::new(db.get_ref().clone(), config.default_credit);
            match user_repo.find_by_id(auth_user.user_id).await {
                Ok(Some(user)) => match user.stripe_connect_account_id {
                    Some(account_id) => {
                        if user.stripe_connect_account_status != ConnectAccountStatus::Verified {
                            return AppError::BadRequest("cashout.connect_verification_pending")
                                .error_response();
                        }
                        account_id
                    }
                    None => {
                        return AppError::BadRequest("cashout.connect_bank_first").error_response()
                    }
                },
                Ok(None) => {
                    return AppError::NotFound("payment.profile_not_found").error_response()
                }
                Err(e) => return AppError::Database(e).error_response(),
            }
        }
        PayoutMethod::PaypalEmail => body.payout_destination.trim().to_string(),
    };

    let response = match service
        .request_cashout(
            auth_user.user_id,
            body.credits,
            payout_method,
            &payout_destination,
        )
        .await
    {
        Ok(response) => response,
        Err(e) => return e.error_response(),
    };

    let user_repo = UserRepository::new(db.get_ref().clone(), config.default_credit);
    match user_repo.find_by_id(auth_user.user_id).await {
        Ok(Some(user)) => {
            let lang = Lang::parse(&user.language).unwrap_or_default();
            let credits = response.credits;
            let amount_eur_cents = response.amount_eur_cents;
            let destination = payout_destination.clone();
            let admin_email = config.cashout_admin_email.clone();

            email_queue.enqueue(EmailJob::CashoutRequested {
                to_email: user.email.clone(),
                credits,
                amount_eur_cents,
                payout_destination: destination.clone(),
                lang,
            });

            if !admin_email.is_empty() {
                tracing::warn!(
                "Enqueuing cashout request to admin email: {} for user {} ({}), credits: {}, amount_eur_cents: {}, destination: {}",
                admin_email, user.pseudo, user.email, credits, amount_eur_cents, destination
            );
                email_queue.enqueue(EmailJob::CashoutAdminAlert {
                    to_email: admin_email,
                    pseudo: user.pseudo.clone(),
                    email: user.email.clone(),
                    credits,
                    amount_eur_cents,
                    payout_destination: destination,
                });
            }
        }
        Ok(None) => {
            tracing::warn!(
                user_id = %auth_user.user_id,
                "cashout request created but user not found; notification emails skipped"
            );
        }
        Err(e) => {
            tracing::error!(
                user_id = %auth_user.user_id,
                error = %e,
                "cashout request created but failed to load user for notification emails"
            );
        }
    }

    HttpResponse::Ok().json(response)
}

#[utoipa::path(
    post,
    path = "/api/me/payout/onboarding",
    tag = "payments",
    security(("cookie_auth" = [])),
    responses(
        (status = 200, description = "Payout onboarding link created", body = PayoutOnboardingResponse),
        (status = 400, description = "Stripe Connect not configured", body = ApiErrorResponse),
        (status = 401, description = "Authentication required", body = ApiErrorResponse),
    )
)]
pub async fn payout_onboarding(
    auth_user: AuthenticatedUser,
    payment_service: web::Data<Arc<dyn PaymentServiceTrait>>,
    db: web::Data<sea_orm::DatabaseConnection>,
    config: web::Data<Config>,
) -> HttpResponse {
    if payment_service.provider() != PaymentProviderKind::Stripe || !payment_service.is_configured()
    {
        return AppError::BadRequest("cashout.stripe_connect_unavailable").error_response();
    }

    let user_repo = UserRepository::new(db.get_ref().clone(), config.default_credit);
    let profile_repo = jambo_db::repositories::PlayerProfileRepository::new(db.get_ref().clone());

    let user = match user_repo.find_by_id(auth_user.user_id).await {
        Ok(Some(user)) => user,
        Ok(None) => return AppError::NotFound("payment.profile_not_found").error_response(),
        Err(e) => return AppError::Database(e).error_response(),
    };

    let country = match profile_repo.find_by_user_id(auth_user.user_id).await {
        Ok(Some(profile)) => profile
            .country_code
            .filter(|c| !c.is_empty())
            .unwrap_or_else(|| config.stripe_connect_account_country.clone()),
        Ok(None) => config.stripe_connect_account_country.clone(),
        Err(_) => config.stripe_connect_account_country.clone(),
    };

    let account_id = match user.stripe_connect_account_id {
        Some(existing) if !existing.is_empty() => existing,
        _ => {
            let params = jambo_core::payment::ConnectAccountParams {
                email: user.email.clone(),
                country,
                reference: auth_user.user_id.to_string(),
            };
            let created = match payment_service.create_connected_account(&params).await {
                Ok(id) => id,
                Err(e) => {
                    tracing::error!(
                        user_id = %auth_user.user_id,
                        error = %e,
                        "failed to create stripe connected account"
                    );
                    return AppError::Internal("Failed to create payout account".into())
                        .error_response();
                }
            };
            if let Err(e) = user_repo
                .set_stripe_connect_account_id(auth_user.user_id, &created)
                .await
            {
                tracing::error!(
                    user_id = %auth_user.user_id,
                    error = %e,
                    "failed to persist stripe connected account id"
                );
                return AppError::Database(e).error_response();
            }
            created
        }
    };

    let base = config.frontend_url.trim_end_matches('/');
    let return_url = format!("{base}/profile?payout=connected");
    let refresh_url = format!("{base}/profile?payout=connect");

    let url = match payment_service
        .create_account_link(&account_id, &return_url, &refresh_url)
        .await
    {
        Ok(u) => u,
        Err(e) => {
            tracing::error!(
                user_id = %auth_user.user_id,
                account_id = %account_id,
                error = %e,
                "failed to create stripe account link"
            );
            return AppError::Internal("Failed to start payout onboarding".into()).error_response();
        }
    };

    // Record that onboarding has started so the status machine can track
    // verification via the account.updated webhook.
    if user.stripe_connect_account_status == ConnectAccountStatus::NotCreated {
        if let Err(e) = user_repo
            .set_stripe_connect_account_status(
                auth_user.user_id,
                ConnectAccountStatus::OnboardingStarted,
            )
            .await
        {
            tracing::warn!(
                user_id = %auth_user.user_id,
                error = %e,
                "failed to mark stripe connect account as onboarding started"
            );
        }
    }

    HttpResponse::Ok().json(PayoutOnboardingResponse { url })
}

#[utoipa::path(
    get,
    path = "/api/me/cashout",
    tag = "payments",
    security(("cookie_auth" = [])),
    params(
        ("page" = Option<u64>, Query, description = "Page number (1-based)"),
        ("per_page" = Option<u64>, Query, description = "Items per page"),
    ),
    responses(
        (status = 200, description = "Paginated cashout history", body = CashoutHistoryResponse),
        (status = 401, description = "Authentication required", body = ApiErrorResponse),
    )
)]
pub async fn list_cashouts(
    auth_user: AuthenticatedUser,
    query: web::Query<CashoutPagination>,
    service: web::Data<Arc<dyn CashoutServiceTrait>>,
) -> HttpResponse {
    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(10).clamp(1, 100);
    match service
        .list_cashouts(auth_user.user_id, page, per_page)
        .await
    {
        Ok(response) => HttpResponse::Ok().json(response),
        Err(e) => e.error_response(),
    }
}

#[derive(serde::Deserialize, utoipa::IntoParams)]
pub struct CashoutPagination {
    pub page: Option<u64>,
    pub per_page: Option<u64>,
}
