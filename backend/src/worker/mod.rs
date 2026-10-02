//! AI worker: consumes `ai_tasks` from RabbitMQ and processes bot moves.
//!
//! The module is split by responsibility so the entry point stays a thin
//! bootstrap (see [`run`]) rather than a monolithic `main`:
//!
//! - [`metrics_server`]: Prometheus `/metrics` HTTP server.
//! - [`shutdown`]: graceful shutdown signalling.
//! - [`consumer`]: the RabbitMQ consume loop with automatic reconnection.

mod consumer;
mod metrics_server;
mod shutdown;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use sea_orm::DatabaseConnection;
use tokio::sync::{Mutex, RwLock, Semaphore};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::config::Config;
use crate::database;
use crate::game::constants::BOT_THINKING_DELAY_MS;
use crate::messaging::{RabbitMQClient, RabbitMQPublishConfig, RedisClient};
use crate::observability::metrics;

pub use metrics_server::MetricsServer;
pub use shutdown::{ShutdownHandle, ShutdownSignal};

/// Convenience entry point for the `ai-worker` binary.
pub async fn run(config: Config) -> Result<()> {
    Worker::run(config).await
}

/// Runtime parameters for the AI worker, resolved from the environment with
/// sensible defaults. Kept separate from [`Config`] to make the worker's
/// concurrency and reconnection tuning explicit and testable.
#[derive(Debug, Clone)]
pub struct WorkerRuntime {
    pub db_pool_size: u32,
    pub max_concurrent: usize,
    pub reconnect_initial_delay_ms: u64,
    pub reconnect_max_delay_ms: u64,
    pub health_check_interval_secs: u64,
    pub queue_depth_interval_secs: u64,
    pub metrics_report_interval_secs: u64,
    pub db_pool_metrics_interval_secs: u64,
    pub process_metrics_interval_secs: u64,
}

impl WorkerRuntime {
    pub fn from_env(config: &Config) -> Self {
        Self {
            db_pool_size: env_u64_or("AI_WORKER_DB_POOL_SIZE", 100) as u32,
            max_concurrent: env_u64_or("AI_WORKER_MAX_CONCURRENT", 50) as usize,
            reconnect_initial_delay_ms: env_u64_or("AI_WORKER_RECONNECT_INITIAL_DELAY_MS", 1_000),
            reconnect_max_delay_ms: env_u64_or("AI_WORKER_RECONNECT_MAX_DELAY_MS", 30_000),
            health_check_interval_secs: env_u64_or("AI_WORKER_HEALTH_CHECK_INTERVAL_SECS", 15),
            queue_depth_interval_secs: env_u64_or("AI_WORKER_QUEUE_DEPTH_INTERVAL_SECS", 10),
            metrics_report_interval_secs: env_u64_or("AI_WORKER_METRICS_REPORT_INTERVAL_SECS", 30),
            db_pool_metrics_interval_secs: config.db_pool_metrics_interval_secs,
            process_metrics_interval_secs: 15,
        }
    }
}

fn env_u64_or(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

/// Aggregated, lock-free counters for worker telemetry.
#[derive(Debug, Default, Clone)]
pub struct WorkerCounters {
    tasks_processed: Arc<AtomicU64>,
    tasks_failed: Arc<AtomicU64>,
    parse_errors: Arc<AtomicU64>,
    delivery_errors: Arc<AtomicU64>,
}

/// A point-in-time snapshot of [`WorkerCounters`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorkerCountersSnapshot {
    pub tasks_processed: u64,
    pub tasks_failed: u64,
    pub parse_errors: u64,
    pub delivery_errors: u64,
    pub total_tasks: u64,
    pub success_rate: f64,
}

impl WorkerCounters {
    pub fn inc_processed(&self) {
        self.tasks_processed.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_failed(&self) {
        self.tasks_failed.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_parse_error(&self) {
        self.parse_errors.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_delivery_error(&self) {
        self.delivery_errors.fetch_add(1, Ordering::Relaxed);
    }

    /// Computes a consistent snapshot, including derived `total_tasks` and
    /// `success_rate` (processed / total, or 0.0 when nothing was processed).
    pub fn snapshot(&self) -> WorkerCountersSnapshot {
        let tasks_processed = self.tasks_processed.load(Ordering::Relaxed);
        let tasks_failed = self.tasks_failed.load(Ordering::Relaxed);
        let parse_errors = self.parse_errors.load(Ordering::Relaxed);
        let delivery_errors = self.delivery_errors.load(Ordering::Relaxed);
        let total_tasks = tasks_processed + tasks_failed + parse_errors;
        let success_rate = if total_tasks > 0 {
            tasks_processed as f64 / total_tasks as f64
        } else {
            0.0
        };
        WorkerCountersSnapshot {
            tasks_processed,
            tasks_failed,
            parse_errors,
            delivery_errors,
            total_tasks,
            success_rate,
        }
    }
}

/// The AI worker. Owns shared dependencies and state, and coordinates the
/// background tasks plus the consume/reconnect loop.
pub struct Worker {
    config: Config,
    runtime: WorkerRuntime,
    db: DatabaseConnection,
    redis: Option<RedisClient>,
    publish_config: RabbitMQPublishConfig,
    semaphore: Arc<Semaphore>,
    game_locks: Arc<Mutex<HashMap<Uuid, Arc<Semaphore>>>>,
    counters: WorkerCounters,
    start_time: Instant,
    shutdown: ShutdownHandle,
    /// The currently active RabbitMQ client, if any. Background tasks (health
    /// checks, queue depth) read from here so they always observe the latest
    /// connection after a reconnect.
    current_client: Arc<RwLock<Option<RabbitMQClient>>>,
}

impl Worker {
    /// Entry point: wires up the metrics server, connections, background tasks,
    /// and runs the consumer/reconnect loop until shutdown.
    pub async fn run(config: Config) -> Result<()> {
        let runtime = WorkerRuntime::from_env(&config);

        info!(
            "Starting AI worker — CPU cores: {}, Tokio runtime workers: {}",
            num_cpus::get(),
            std::env::var("TOKIO_WORKER_THREADS")
                .unwrap_or_else(|_| "default (num_cpus)".to_string())
        );

        let metrics_server = MetricsServer::spawn(&config, 2000).await?;

        let db = database::create_connection_with_pool_size(&config, runtime.db_pool_size)
            .await
            .context("Failed to create database connection")?;
        info!(
            "Connected to database (pool size: {})",
            runtime.db_pool_size
        );

        let redis = Self::connect_redis(&config).await;

        let worker = Worker {
            publish_config: Self::publish_config(&config),
            config,
            runtime: runtime.clone(),
            db,
            redis,
            semaphore: Arc::new(Semaphore::new(runtime.max_concurrent)),
            game_locks: Arc::new(Mutex::new(HashMap::new())),
            counters: WorkerCounters::default(),
            start_time: Instant::now(),
            shutdown: ShutdownHandle::new(),
            current_client: Arc::new(RwLock::new(None)),
        };

        worker.spawn_shutdown_handler();
        worker.spawn_background_tasks();

        info!(
            "Waiting for AI tasks (concurrent: {}, frontend thinking delay: {}ms)...",
            runtime.max_concurrent, *BOT_THINKING_DELAY_MS
        );

        worker.run_consumer_loop().await;

        // Drain in-flight tasks by acquiring all semaphore permits: once every
        // permit is back, no spawned task can still be holding one.
        info!("Draining in-flight tasks...");
        let drain_semaphore = worker.semaphore.clone();
        let _drain_permits = drain_semaphore
            .acquire_many(runtime.max_concurrent as u32)
            .await;

        worker.log_final_metrics();

        metrics_server.stop().await;

        Ok(())
    }

    fn publish_config(config: &Config) -> RabbitMQPublishConfig {
        RabbitMQPublishConfig {
            max_retries: config.rabbitmq_publish_max_retries,
            initial_retry_delay_ms: config.rabbitmq_publish_initial_retry_delay_ms,
            max_retry_delay_ms: config.rabbitmq_publish_max_retry_delay_ms,
            circuit_breaker_failure_threshold: config.circuit_breaker_failure_threshold,
            circuit_breaker_cooldown_secs: config.circuit_breaker_cooldown_secs,
        }
    }

    async fn connect_redis(config: &Config) -> Option<RedisClient> {
        match config.redis_url.as_deref() {
            Some(url) => match RedisClient::new(url).await {
                Ok(client) => {
                    info!("Connected to Redis");
                    Some(client)
                }
                Err(e) => {
                    warn!("Failed to connect to Redis: {e}, proceeding without Redis");
                    None
                }
            },
            None => None,
        }
    }

    /// Translates a Ctrl-C into a shutdown signal shared by all tasks.
    fn spawn_shutdown_handler(&self) {
        let shutdown = self.shutdown.clone();
        tokio::spawn(async move {
            tokio::signal::ctrl_c().await.ok();
            info!("Received shutdown signal, draining...");
            shutdown.trigger();
        });
    }

    fn spawn_background_tasks(&self) {
        self.spawn_db_pool_metrics_task();
        self.spawn_process_metrics_task();
        self.spawn_metrics_reporter_task();
        self.spawn_health_monitor_task();
        self.spawn_queue_depth_monitor_task();
    }

    fn spawn_db_pool_metrics_task(&self) {
        let db = self.db.clone();
        let interval = Duration::from_secs(self.runtime.db_pool_metrics_interval_secs);
        let shutdown = self.shutdown.signal();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                tokio::select! {
                    _ = shutdown.wait() => break,
                    _ = ticker.tick() => {
                        metrics::update_db_pool_metrics(&db, "ai_worker");
                    }
                }
            }
        });
    }

    fn spawn_process_metrics_task(&self) {
        let interval = Duration::from_secs(self.runtime.process_metrics_interval_secs);
        let shutdown = self.shutdown.signal();
        tokio::spawn(async move {
            let mut sys = sysinfo::System::new();
            let mut ticker = tokio::time::interval(interval);
            loop {
                tokio::select! {
                    _ = shutdown.wait() => break,
                    _ = ticker.tick() => {
                        metrics::update_process_metrics(&mut sys, "ai_worker");
                    }
                }
            }
        });
    }

    fn spawn_metrics_reporter_task(&self) {
        let counters = self.counters.clone();
        let interval = Duration::from_secs(self.runtime.metrics_report_interval_secs);
        let start_time = self.start_time;
        let shutdown = self.shutdown.signal();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                tokio::select! {
                    _ = shutdown.wait() => break,
                    _ = ticker.tick() => {
                        let snapshot = counters.snapshot();
                        info!(
                            tasks_processed = snapshot.tasks_processed,
                            tasks_failed = snapshot.tasks_failed,
                            parse_errors = snapshot.parse_errors,
                            delivery_errors = snapshot.delivery_errors,
                            total_tasks = snapshot.total_tasks,
                            success_rate = snapshot.success_rate,
                            uptime_seconds = start_time.elapsed().as_secs(),
                            "Periodic metrics"
                        );
                    }
                }
            }
        });
    }

    /// Periodically calls [`RabbitMQClient::check_health`] on the current
    /// connection so the `rabbitmq_healthy` gauge reflects reality, even while
    /// the consumer loop is blocked waiting for deliveries.
    fn spawn_health_monitor_task(&self) {
        let current_client = self.current_client.clone();
        let interval = Duration::from_secs(self.runtime.health_check_interval_secs);
        let shutdown = self.shutdown.signal();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                tokio::select! {
                    _ = shutdown.wait() => break,
                    _ = ticker.tick() => {
                        match current_client.read().await.clone() {
                            Some(client) => {
                                client.check_health().await;
                            }
                            None => {
                                // Not connected (yet) — mark unhealthy.
                                metrics::RABBITMQ_HEALTHY.set(0.0);
                            }
                        }
                    }
                }
            }
        });
    }

    fn spawn_queue_depth_monitor_task(&self) {
        let current_client = self.current_client.clone();
        let interval = Duration::from_secs(self.runtime.queue_depth_interval_secs);
        let shutdown = self.shutdown.signal();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                tokio::select! {
                    _ = shutdown.wait() => break,
                    _ = ticker.tick() => {
                        let Some(client) = current_client.read().await.clone() else {
                            continue;
                        };
                        match client.get_queue_length("ai_tasks").await {
                            Ok(depth) => {
                                metrics::RABBITMQ_QUEUE_LENGTH.set(depth as f64);
                                if depth > 1000 {
                                    warn!("AI tasks queue depth critical: {}", depth);
                                } else if depth > 500 {
                                    warn!("AI tasks queue depth high: {}", depth);
                                }
                            }
                            Err(e) => {
                                error!("Failed to get queue depth: {}", e);
                            }
                        }
                    }
                }
            }
        });
    }

    fn log_final_metrics(&self) {
        let snapshot = self.counters.snapshot();
        info!(
            tasks_processed = snapshot.tasks_processed,
            tasks_failed = snapshot.tasks_failed,
            parse_errors = snapshot.parse_errors,
            delivery_errors = snapshot.delivery_errors,
            total_tasks = snapshot.total_tasks,
            success_rate = snapshot.success_rate,
            uptime_seconds = self.start_time.elapsed().as_secs(),
            "AI worker shutting down - final metrics"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_defaults_match_documented_values() {
        let config = Config::default();
        let runtime = WorkerRuntime::from_env(&config);

        assert_eq!(runtime.db_pool_size, 100);
        assert_eq!(runtime.max_concurrent, 50);
        assert_eq!(runtime.reconnect_initial_delay_ms, 1_000);
        assert_eq!(runtime.reconnect_max_delay_ms, 30_000);
        assert_eq!(runtime.health_check_interval_secs, 15);
        assert_eq!(runtime.queue_depth_interval_secs, 10);
        assert_eq!(runtime.metrics_report_interval_secs, 30);
    }

    #[test]
    fn counters_start_empty_with_zero_success_rate() {
        let counters = WorkerCounters::default();
        let snapshot = counters.snapshot();

        assert_eq!(snapshot.tasks_processed, 0);
        assert_eq!(snapshot.tasks_failed, 0);
        assert_eq!(snapshot.parse_errors, 0);
        assert_eq!(snapshot.delivery_errors, 0);
        assert_eq!(snapshot.total_tasks, 0);
        assert_eq!(snapshot.success_rate, 0.0);
    }

    #[test]
    fn counters_track_individual_metrics() {
        let counters = WorkerCounters::default();

        counters.inc_processed();
        counters.inc_processed();
        counters.inc_failed();
        counters.inc_parse_error();
        counters.inc_delivery_error();
        counters.inc_delivery_error();

        let snapshot = counters.snapshot();
        assert_eq!(snapshot.tasks_processed, 2);
        assert_eq!(snapshot.tasks_failed, 1);
        assert_eq!(snapshot.parse_errors, 1);
        assert_eq!(snapshot.delivery_errors, 2);
        // delivery_errors is not part of total_tasks.
        assert_eq!(snapshot.total_tasks, 4);
        assert!((snapshot.success_rate - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn counters_success_rate_is_full_when_no_failures() {
        let counters = WorkerCounters::default();
        for _ in 0..10 {
            counters.inc_processed();
        }

        let snapshot = counters.snapshot();
        assert_eq!(snapshot.total_tasks, 10);
        assert!((snapshot.success_rate - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn publish_config_maps_all_config_values() {
        let config = Config::default();
        let publish_config = Worker::publish_config(&config);

        assert_eq!(
            publish_config.max_retries,
            config.rabbitmq_publish_max_retries
        );
        assert_eq!(
            publish_config.initial_retry_delay_ms,
            config.rabbitmq_publish_initial_retry_delay_ms
        );
        assert_eq!(
            publish_config.max_retry_delay_ms,
            config.rabbitmq_publish_max_retry_delay_ms
        );
        assert_eq!(
            publish_config.circuit_breaker_failure_threshold,
            config.circuit_breaker_failure_threshold
        );
        assert_eq!(
            publish_config.circuit_breaker_cooldown_secs,
            config.circuit_breaker_cooldown_secs
        );
    }
}
