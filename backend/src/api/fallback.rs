use actix_web::HttpResponse;

use crate::api::dto::responses::ApiErrorResponse;

pub async fn not_found() -> HttpResponse {
    let request_id = crate::observability::CORRELATION_ID
        .try_with(|id| id.to_string())
        .ok();
    HttpResponse::NotFound().json(ApiErrorResponse {
        success: false,
        error: "Not found".to_string(),
        field: None,
        source: "fallback:not_found".to_string(),
        request_id,
    })
}

pub async fn method_not_allowed() -> HttpResponse {
    let request_id = crate::observability::CORRELATION_ID
        .try_with(|id| id.to_string())
        .ok();
    HttpResponse::MethodNotAllowed().json(ApiErrorResponse {
        success: false,
        error: "Method not allowed".to_string(),
        field: None,
        source: "fallback:method_not_allowed".to_string(),
        request_id,
    })
}
