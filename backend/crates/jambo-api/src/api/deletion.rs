use actix_web::{web, HttpResponse, ResponseError};

use crate::api::dto::responses::{ApiErrorResponse, DeletionRequestResponse};
use crate::auth::extractors::AuthenticatedUser;
use jambo_core::config::Config;
use jambo_core::error::AppError;
use jambo_core::i18n::I18n;
use jambo_db::repositories::UserRepository;

/// Record a GDPR data-deletion request for the authenticated user. The account
/// stays fully usable until a DPO executes the soft-delete via `admin-cli`.
#[utoipa::path(
    post,
    path = "/api/me/deletion-request",
    tag = "account",
    security(("cookie_auth" = [])),
    responses(
        (status = 200, description = "Deletion request recorded", body = DeletionRequestResponse),
        (status = 401, description = "Authentication required", body = ApiErrorResponse),
        (status = 500, description = "Database error", body = ApiErrorResponse),
    )
)]
pub async fn request_account_deletion(
    auth_user: AuthenticatedUser,
    db: web::Data<sea_orm::DatabaseConnection>,
    config: web::Data<Config>,
    i18n: I18n,
) -> HttpResponse {
    let repo = UserRepository::new(db.get_ref().clone(), config.default_credit);
    match repo.request_deletion(auth_user.user_id).await {
        Ok(user) => HttpResponse::Ok().json(DeletionRequestResponse {
            success: true,
            requested_at: user.deletion_requested_at.map(|t| t.to_rfc3339()),
            message: i18n.t("auth.deletion_requested"),
        }),
        Err(e) => {
            tracing::error!(
                user_id = %auth_user.user_id,
                error = %e,
                "failed to record deletion request"
            );
            AppError::Database(e).error_response()
        }
    }
}
