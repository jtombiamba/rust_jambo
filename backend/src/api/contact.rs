use actix_web::{web, HttpResponse};
use serde::Deserialize;
use std::sync::Arc;

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
) -> HttpResponse {
    let body = body.into_inner();

    if body.name.trim().is_empty()
        || body.email.trim().is_empty()
        || body.subject.trim().is_empty()
        || body.message.trim().is_empty()
    {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "error": "All fields are required"
        }));
    }

    match mailer
        .send_contact_form(&body.name, &body.email, &body.subject, &body.message)
        .await
    {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({
            "message": "Message sent successfully"
        })),
        Err(e) => {
            tracing::error!("Failed to send contact form: {}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({
                "error": "Failed to send message. Please try again later."
            }))
        }
    }
}
