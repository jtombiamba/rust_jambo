use actix_web::{web, HttpResponse};
use serde::Deserialize;
use std::sync::Arc;

use crate::api::dto::responses::ApiErrorResponse;
use crate::i18n::I18n;
use crate::mailer::Mailer;

#[derive(Deserialize)]
pub struct ContactRequest {
    pub name: String,
    pub email: String,
    pub subject: String,
    pub message: String,
}

pub async fn send_contact(
    body: web::Json<ContactRequest>,
    mailer: web::Data<Arc<dyn Mailer>>,
    i18n: I18n,
) -> HttpResponse {
    let body = body.into_inner();

    if body.name.trim().is_empty()
        || body.email.trim().is_empty()
        || body.subject.trim().is_empty()
        || body.message.trim().is_empty()
    {
        return HttpResponse::BadRequest().json(ApiErrorResponse {
            success: false,
            error: i18n.t("contact.all_fields_required"),
            field: None,
            source: "contact:validation".to_string(),
            request_id: crate::observability::CORRELATION_ID
                .try_with(|id| id.to_string())
                .ok(),
        });
    }

    match mailer
        .send_contact_form(
            &body.name,
            &body.email,
            &body.subject,
            &body.message,
            i18n.lang,
        )
        .await
    {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({
            "message": i18n.t("contact.sent")
        })),
        Err(e) => {
            tracing::error!("Failed to send contact form: {}", e);
            let request_id = crate::observability::CORRELATION_ID
                .try_with(|id| id.to_string())
                .ok();
            HttpResponse::InternalServerError().json(ApiErrorResponse {
                success: false,
                error: i18n.t("contact.send_failed"),
                field: None,
                source: "contact:email".to_string(),
                request_id,
            })
        }
    }
}
