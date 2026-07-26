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

#[cfg(test)]
mod tests {
    use actix_web::body::to_bytes;
    use serde_json::Value;

    use super::*;

    #[actix_web::test]
    async fn test_not_found_handler() {
        let resp = not_found().await;
        assert_eq!(resp.status(), 404);
        let body = to_bytes(resp.into_body()).await.unwrap();
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["success"], false);
        assert_eq!(body["source"], "fallback:not_found");
    }

    #[actix_web::test]
    async fn test_method_not_allowed_handler() {
        let resp = method_not_allowed().await;
        assert_eq!(resp.status(), 405);
        let body = to_bytes(resp.into_body()).await.unwrap();
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["success"], false);
        assert_eq!(body["source"], "fallback:method_not_allowed");
    }
}
