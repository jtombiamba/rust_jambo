use std::sync::Arc;

use actix_web::{web, HttpResponse, ResponseError};

use crate::api::dto::requests::CashoutRequest;
use crate::api::dto::responses::{
    ApiErrorResponse, CashoutHistoryResponse, CashoutRequestResponse,
};
use crate::api::services::cashout_service::CashoutServiceTrait;
use crate::auth::extractors::AuthenticatedUser;
use crate::config::Config;
use crate::database::repositories::UserRepository;
use crate::i18n::Lang;
use crate::mailer::{EmailJob, EmailQueue};

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
    let response = match service
        .request_cashout(auth_user.user_id, body.credits, &body.paypal_email)
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
            let paypal_email = body.paypal_email.clone();
            let admin_email = config.cashout_admin_email.clone();

            email_queue.enqueue(EmailJob::CashoutRequested {
                to_email: user.email.clone(),
                credits,
                amount_eur_cents,
                paypal_email: paypal_email.clone(),
                lang,
            });

            if !admin_email.is_empty() {
                email_queue.enqueue(EmailJob::CashoutAdminAlert {
                    to_email: admin_email,
                    pseudo: user.pseudo.clone(),
                    email: user.email.clone(),
                    credits,
                    amount_eur_cents,
                    paypal_email,
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
