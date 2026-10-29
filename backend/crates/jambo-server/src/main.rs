use actix_web::{App, HttpServer};
use tracing::info;

mod bootstrap;
mod routes;

use crate::bootstrap::bootstrap;
use crate::routes::configure;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    jambo_core::observability::init_tracing("jambo-backend");

    jambo_core::observability::metrics_init::init_all();

    let config = jambo_core::config::Config::from_env().expect("Failed to load configuration");

    // Periodically publish process CPU/memory usage. The Rust prometheus
    // crate does not auto-export process metrics, so we collect them here
    // with sysinfo and expose them as cpu_usage_percent / memory_usage_bytes.
    let process_metrics_interval =
        std::time::Duration::from_secs(config.process_metrics_interval_secs);
    tokio::spawn(async move {
        let mut sys = sysinfo::System::new();
        let mut interval = tokio::time::interval(process_metrics_interval);
        loop {
            interval.tick().await;
            jambo_core::observability::metrics::update_process_metrics(&mut sys, "backend");
        }
    });

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
            .wrap(jambo_core::i18n::middleware::I18nMiddleware)
            .wrap(cors_config.cors_middleware())
            .wrap(jambo_core::observability::middleware::CorrelationIdMiddleware)
            .wrap(jambo_api::api::middleware::ip_forward::ForwardedIpMiddleware)
            .configure(|cfg| configure(cfg, &state))
    })
    .bind((config.host.as_str(), config.port))?
    .workers(num_cpus::get())
    .run()
    .await
}
