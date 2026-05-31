use std::sync::Arc;
use std::time::Duration;

use actix_web::{web, App, HttpServer};
use anyhow::{Context, Result};
use prometheus::TextEncoder;
use sea_orm::DatabaseConnection;
use tracing::{error, info, warn};

use jambo_backend::cache::UserCache;
use jambo_backend::config::Config;
use jambo_backend::database;
use jambo_backend::mailer::{self, MailerConfig};
use jambo_backend::messaging::RedisClient;
use jambo_backend::observability::metrics;
use jambo_backend::scheduler::Scheduler;

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

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("scheduler_worker=info".parse().unwrap()),
        )
        .json()
        .init();

    metrics::init_all();

    let config = Config::from_env().context("Failed to load configuration")?;
    info!("Starting scheduler worker");

    let metrics_port = (config.port as u32 + 1000).min(65535) as u16;
    let bind_addr = format!("{}:{}", config.host, metrics_port);
    let metrics_server =
        HttpServer::new(move || App::new().route("/metrics", web::get().to(metrics_handler)))
            .bind(&bind_addr)
            .context("Failed to bind metrics HTTP server")?
            .workers(2)
            .run();
    let metrics_handle = metrics_server.handle();
    tokio::spawn(metrics_server);
    info!("Metrics server listening on http://{}/metrics", bind_addr);

    let db_connection: DatabaseConnection = database::create_connection(&config)
        .await
        .context("Failed to create database connection")?;
    info!(
        "Connected to database (pool max: {})",
        config.db_pool_max_connections
    );

    let redis_client = match &config.redis_url {
        Some(url) => match RedisClient::new(url).await {
            Ok(client) => {
                info!("Connected to Redis");
                Some(client)
            }
            Err(e) => {
                warn!(
                    "Failed to connect to Redis: {}, proceeding without Redis",
                    e
                );
                None
            }
        },
        None => None,
    };

    let mailer_config = MailerConfig::from_env();
    let mailer = mailer::create_mailer(mailer_config)
        .map_err(|e| anyhow::anyhow!("Failed to create mailer: {}", e))?;

    let user_cache = match redis_client.clone() {
        Some(rc) => Arc::new(UserCache::new_with_redis(rc)),
        None => Arc::new(UserCache::new()),
    };

    let scheduler = Scheduler::new(db_connection, redis_client, mailer, user_cache, config);

    let (mut tasks, shutdown_tx) = scheduler.run_all();
    let start_time = std::time::Instant::now();
    info!("All scheduler tasks started");

    tokio::select! {
        result = tasks.join_next() => {
            match result {
                Some(Ok(())) => {
                    error!("A scheduler task exited — restarting worker");
                    eprintln!("FATAL: A scheduler task exited unexpectedly");
                    let _ = shutdown_tx.send(true);
                    std::process::exit(1);
                }
                Some(Err(e)) => {
                    error!(
                        panic = %e,
                        "A scheduler task panicked, initiating shutdown"
                    );
                    eprintln!("FATAL: A scheduler task panicked: {}", e);
                    let _ = shutdown_tx.send(true);
                    std::process::exit(1);
                }
                None => {
                    info!("All scheduler tasks completed normally");
                }
            }
        }
        _ = tokio::signal::ctrl_c() => {
            info!("Received shutdown signal, draining...");
            let _ = shutdown_tx.send(true);
            tokio::time::sleep(Duration::from_secs(5)).await;
            tasks.shutdown().await;
        }
    }

    metrics_handle.stop(true).await;

    let uptime = start_time.elapsed();
    info!(
        uptime_seconds = uptime.as_secs(),
        "Scheduler worker shutting down"
    );

    Ok(())
}
