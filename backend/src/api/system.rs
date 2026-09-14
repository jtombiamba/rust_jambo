use actix_web::{get, HttpResponse, Responder};
use prometheus::Encoder;

#[utoipa::path(
    get,
    path = "/health",
    tag = "system",
    responses((status = 200, description = "Service is healthy", body = String))
)]
#[get("/health")]
pub async fn health_check() -> impl Responder {
    HttpResponse::Ok().body("OK")
}

#[utoipa::path(
    get,
    path = "/metrics",
    tag = "system",
    responses((status = 200, description = "Prometheus metrics", content_type = "text/plain"))
)]
#[get("/metrics")]
pub async fn metrics() -> HttpResponse {
    let encoder = prometheus::TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = vec![];
    encoder.encode(&metric_families, &mut buffer).unwrap();
    HttpResponse::Ok()
        .content_type("text/plain; version=0.0.4")
        .body(buffer)
}
