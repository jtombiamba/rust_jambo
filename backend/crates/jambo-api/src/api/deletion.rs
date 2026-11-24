use actix_web::{web, HttpResponse};
use serde::Deserialize;

use jambo_core::config::Config;
use jambo_core::deletion_token;
use jambo_db::repositories::UserRepository;

#[derive(Deserialize, utoipa::IntoParams)]
pub struct DeletionConfirmQuery {
    pub token: String,
}

/// Confirm an account-deactivation request from the emailed link.
///
/// The link is sent by a DPO through `admin-cli` and carries a signed JWT. On a
/// valid token the user's `deletion_requested_at` is set, which surfaces the
/// request in `admin-cli deletion-list`. Returns a small HTML page either way.
#[utoipa::path(
    get,
    path = "/api/deletion/confirm",
    tag = "account",
    params(DeletionConfirmQuery),
    responses(
        (status = 200, description = "Deletion request recorded", content_type = "text/html"),
        (status = 400, description = "Invalid or expired token", content_type = "text/html"),
        (status = 500, description = "Database error", content_type = "text/html"),
    )
)]
pub async fn confirm_account_deletion(
    query: web::Query<DeletionConfirmQuery>,
    db: web::Data<sea_orm::DatabaseConnection>,
    config: web::Data<Config>,
) -> HttpResponse {
    let claims = match deletion_token::validate_deletion_token(&query.token, &config.jwt_secret) {
        Ok(claims) => claims,
        Err(e) => {
            tracing::info!(error = %e, "invalid deletion confirmation token");
            return deletion_page(
                actix_web::http::StatusCode::BAD_REQUEST,
                "Link invalid or expired",
                "This link is invalid or has expired.",
            );
        }
    };

    let repo = UserRepository::new(db.get_ref().clone(), config.default_credit);
    match repo.request_deletion(claims.sub).await {
        Ok(user) => {
            tracing::info!(user_id = %user.id, "recorded deletion request via confirmation link");
            deletion_page(
                actix_web::http::StatusCode::OK,
                "Request received",
                "Your data deletion request has been recorded. Our team will process it shortly.",
            )
        }
        Err(e) => {
            tracing::error!(
                user_id = %claims.sub,
                error = %e,
                "failed to record deletion request from confirmation link"
            );
            deletion_page(
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                "Something went wrong",
                "We could not record your request. Please contact support.",
            )
        }
    }
}

fn deletion_page(status: actix_web::http::StatusCode, title: &str, message: &str) -> HttpResponse {
    let html = format!(
        "<!DOCTYPE html><html><head><meta charset=\"utf-8\"><title>{title}</title></head>\
         <body style=\"font-family:Arial,sans-serif;background:#f3f4f6;margin:0;padding:48px 16px;text-align:center;\">\
         <div style=\"max-width:480px;margin:0 auto;background:#fff;border-radius:12px;padding:32px;box-shadow:0 2px 8px rgba(0,0,0,0.06);\">\
         <h1 style=\"color:#111827;font-size:20px;margin:0 0 12px;\">{title}</h1>\
         <p style=\"color:#4b5563;font-size:14px;line-height:1.6;margin:0;\">{message}</p>\
         </div></body></html>"
    );
    HttpResponse::build(status)
        .content_type("text/html; charset=utf-8")
        .body(html)
}
