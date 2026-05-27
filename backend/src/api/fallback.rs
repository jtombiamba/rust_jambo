use actix_web::HttpResponse;
use serde_json::json;

pub async fn not_found() -> HttpResponse {
    HttpResponse::NotFound().json(json!({
        "success": false,
        "error": "Not found"
    }))
}

pub async fn method_not_allowed() -> HttpResponse {
    HttpResponse::MethodNotAllowed().json(json!({
        "success": false,
        "error": "Method not allowed"
    }))
}
