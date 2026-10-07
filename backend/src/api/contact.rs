use actix_web::{web, HttpResponse};
use serde::Deserialize;

use crate::api::dto::responses::{ApiErrorResponse, ContactSentResponse};
use crate::i18n::I18n;
use crate::mailer::{EmailJob, EmailQueue};

#[derive(Deserialize, utoipa::ToSchema)]
pub struct ContactRequest {
    pub name: String,
    pub email: String,
    pub subject: String,
    pub message: String,
}

#[utoipa::path(
    post,
    path = "/api/contact",
    tag = "contact",
    request_body = ContactRequest,
    responses(
        (status = 200, description = "Contact message sent", body = ContactSentResponse),
        (status = 400, description = "Validation error", body = ApiErrorResponse),
        (status = 429, description = "Rate limited", body = ApiErrorResponse),
        (status = 500, description = "Email send failure", body = ApiErrorResponse),
    )
)]
pub async fn send_contact(
    body: web::Json<ContactRequest>,
    email_queue: web::Data<EmailQueue>,
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

    email_queue.enqueue(EmailJob::ContactForm {
        name: body.name,
        email: body.email,
        subject: body.subject,
        message: body.message,
        lang: i18n.lang,
    });

    HttpResponse::Ok().json(ContactSentResponse {
        message: i18n.t("contact.sent"),
    })
}
