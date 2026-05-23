use actix_web::{web, HttpResponse};
use serde::Deserialize;
use std::sync::Arc;

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
        return HttpResponse::BadRequest().json(serde_json::json!({
            "error": i18n.t("contact.all_fields_required")
        }));
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
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": i18n.t("contact.send_failed")
            }))
        }
    }
}
