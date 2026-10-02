//! Prometheus `/metrics` HTTP server for the AI worker.

use actix_web::{web, App, HttpServer};
use anyhow::Context;
use prometheus::TextEncoder;

use crate::config::Config;

async fn metrics_handler() -> actix_web::HttpResponse {
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let body = encoder
        .encode_to_string(&metric_families)
        .unwrap_or_default();
    actix_web::HttpResponse::Ok()
        .content_type("text/plain; version=0.0.4")
        .body(body)
}

/// Handle to a running metrics HTTP server, used to stop it gracefully.
pub struct MetricsServer {
    handle: actix_web::dev::ServerHandle,
}

impl MetricsServer {
    /// Binds and spawns the Prometheus metrics HTTP server on `config.host` at
    /// `config.port + port_offset`.
    pub async fn spawn(config: &Config, port_offset: u32) -> anyhow::Result<Self> {
        let metrics_port = (config.port as u32 + port_offset).min(65535) as u16;
        let bind_addr = format!("{}:{}", config.host, metrics_port);

        let server =
            HttpServer::new(move || App::new().route("/metrics", web::get().to(metrics_handler)))
                .bind(&bind_addr)
                .with_context(|| format!("Failed to bind metrics HTTP server to {bind_addr}"))?
                .workers(2)
                .run();

        let handle = server.handle();
        tokio::spawn(server);
        tracing::info!("Metrics server listening on http://{bind_addr}/metrics");

        Ok(Self { handle })
    }

    /// Stops the underlying HTTP server and waits for in-flight requests.
    pub async fn stop(self) {
        self.handle.stop(true).await;
    }
}
