use crate::api::fallback::{method_not_allowed, not_found};
use actix_web::body::to_bytes;
use serde_json::Value;

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
