use actix_web::{App, HttpServer};
use tracing::info;

mod api;
mod auth;
mod bootstrap;
mod cache;
mod config;
mod database;
mod error;
mod game;
mod i18n;
mod mailer;
mod messaging;
mod observability;
mod payment;
mod room;
mod routes;
mod websocket;

use crate::bootstrap::bootstrap;
use crate::routes::configure;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    crate::observability::init_tracing("jambo-backend");

    crate::observability::metrics_init::init_all();

    // Periodically publish process CPU/memory usage. The Rust prometheus
    // crate does not auto-export process metrics, so we collect them here
    // with sysinfo and expose them as cpu_usage_percent / memory_usage_bytes.
    tokio::spawn(async {
        let mut sys = sysinfo::System::new();
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(15));
        loop {
            interval.tick().await;
            crate::observability::metrics::update_process_metrics(&mut sys, "backend");
        }
    });

    let config = crate::config::Config::from_env().expect("Failed to load configuration");
    let cpu_count = num_cpus::get();
    info!(
        "Starting server on {}:{} -- CPU cores: {}, Actix workers: 2 (production-optimized)",
        config.host, config.port, cpu_count
    );

    let state = bootstrap(&config)
        .await
        .expect("Failed to bootstrap application");

    let cors_config = config.clone();

    HttpServer::new(move || {
        info!("Registering routes");
        App::new()
            .wrap(crate::i18n::middleware::I18nMiddleware)
            .wrap(cors_config.cors_middleware())
            .wrap(crate::observability::middleware::CorrelationIdMiddleware)
            .wrap(crate::api::middleware::ip_forward::ForwardedIpMiddleware)
            .configure(|cfg| configure(cfg, &state))
    })
    .bind((config.host.as_str(), config.port))?
    .workers(num_cpus::get())
    .run()
    .await
}

#[cfg(test)]
mod tests {
    #[actix_web::test]
    async fn test_health_check() {
        let app = actix_web::test::init_service(
            actix_web::App::new().service(crate::api::system::health_check),
        )
        .await;
        let req = actix_web::test::TestRequest::get()
            .uri("/health")
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body = actix_web::test::read_body(resp).await;
        assert_eq!(body, "OK");
    }

    #[actix_web::test]
    async fn test_metrics() {
        let app = actix_web::test::init_service(
            actix_web::App::new().service(crate::api::system::metrics),
        )
        .await;
        let req = actix_web::test::TestRequest::get()
            .uri("/metrics")
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let content_type = resp
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(
            content_type.starts_with("text/plain"),
            "Expected text/plain content-type, got: {content_type}"
        );
    }

    #[actix_web::test]
    async fn test_metrics_contains_default_metrics() {
        crate::observability::metrics_init::init_all();
        let app = actix_web::test::init_service(
            actix_web::App::new().service(crate::api::system::metrics),
        )
        .await;
        let req = actix_web::test::TestRequest::get()
            .uri("/metrics")
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body = actix_web::test::read_body(resp).await;
        let body_str = String::from_utf8(body.to_vec()).unwrap();
        assert!(
            body_str.contains("ws_messages_sent_total"),
            "Expected ws_messages_sent_total in metrics, got: {body_str}"
        );
        assert!(
            body_str.contains("ws_connections_active"),
            "Expected ws_connections_active in metrics, got: {body_str}"
        );
        assert!(
            body_str.contains("rabbitmq_consume_total"),
            "Expected rabbitmq_consume_total in metrics, got: {body_str}"
        );
        assert!(
            body_str.contains("rabbitmq_healthy"),
            "Expected rabbitmq_healthy in metrics, got: {body_str}"
        );
        assert!(
            body_str.contains("rabbitmq_publish_total"),
            "Expected rabbitmq_publish_total in metrics, got: {body_str}"
        );
        assert!(
            body_str.contains("games_finished_total"),
            "Expected games_finished_total in metrics, got: {body_str}"
        );
        assert!(
            body_str.contains("ws_send_failed_total"),
            "Expected ws_send_failed_total in metrics, got: {body_str}"
        );
        assert!(
            body_str.contains("ws_messages_dropped_total"),
            "Expected ws_messages_dropped_total in metrics, got: {body_str}"
        );
        assert!(
            body_str.contains("ws_slow_consumer_disconnects_total"),
            "Expected ws_slow_consumer_disconnects_total in metrics, got: {body_str}"
        );
    }
}
