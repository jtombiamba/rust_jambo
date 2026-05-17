use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use futures::StreamExt;
use lapin::{
    options::{BasicAckOptions, BasicNackOptions},
    Consumer,
};
use sea_orm::DatabaseConnection;
use tokio::sync::{Mutex, Semaphore};
use tracing::{error, info, warn};
use uuid::Uuid;

use jambo_backend::config::Config;
use jambo_backend::database;
use jambo_backend::game::constants::BOT_THINKING_DELAY_MS;
use jambo_backend::game::worker_core::process_bot_move;
use jambo_backend::messaging::{self, AITask, RabbitMQPublishConfig, RedisClient};
use jambo_backend::observability::metrics;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("ai_worker=info".parse().unwrap()),
        )
        .json()
        .init();

    let config = Config::from_env().context("Failed to load configuration")?;
    let cpu_count = num_cpus::get();
    info!(
        "Starting AI worker — CPU cores: {}, Tokio runtime workers: {}",
        cpu_count,
        std::env::var("TOKIO_WORKER_THREADS").unwrap_or_else(|_| "default (num_cpus)".to_string())
    );

    let pool_size = std::env::var("AI_WORKER_DB_POOL_SIZE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100);
    let db_connection: DatabaseConnection =
        database::create_connection_with_pool_size(&config, pool_size)
            .await
            .context("Failed to create database connection")?;
    info!("Connected to database (pool size: {})", pool_size);

    let redis_client = match config.redis_url {
        Some(url) => match RedisClient::new(&url).await {
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

    let publish_config = RabbitMQPublishConfig {
        max_retries: config.rabbitmq_publish_max_retries,
        initial_retry_delay_ms: config.rabbitmq_publish_initial_retry_delay_ms,
        max_retry_delay_ms: config.rabbitmq_publish_max_retry_delay_ms,
        circuit_breaker_failure_threshold: config.circuit_breaker_failure_threshold,
        circuit_breaker_cooldown_secs: config.circuit_breaker_cooldown_secs,
    };

    let rabbitmq_client = messaging::connect_to_rabbitmq_with_retry(
        &config.rabbitmq_url,
        config.max_rabbitmq_connection_retries,
        publish_config,
    )
    .await
    .context("Failed to connect to RabbitMQ after retries")?;
    info!("Connected to RabbitMQ");

    let db_for_pool_metrics = db_connection.clone();
    let pool_metrics_interval = Duration::from_secs(config.db_pool_metrics_interval_secs);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(pool_metrics_interval);
        loop {
            interval.tick().await;
            metrics::update_db_pool_metrics(&db_for_pool_metrics);
        }
    });

    let max_concurrent = std::env::var("AI_WORKER_MAX_CONCURRENT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(50);
    let semaphore = Arc::new(Semaphore::new(max_concurrent));

    // Per-game mutex to ensure only one bot move per game is processed at a time.
    // This prevents race conditions when multiple bots in the same game have their
    // AI tasks processed concurrently.
    let game_locks: Arc<Mutex<HashMap<Uuid, Arc<Semaphore>>>> =
        Arc::new(Mutex::new(HashMap::new()));

    let mut consumer: Consumer = rabbitmq_client
        .consume_ai_tasks()
        .await
        .context("Failed to start consuming AI tasks")?;
    info!(
        "Waiting for AI tasks (concurrent: {}, delay: {}ms)...",
        max_concurrent, *BOT_THINKING_DELAY_MS
    );

    let tasks_processed = Arc::new(AtomicU64::new(0));
    let tasks_failed = Arc::new(AtomicU64::new(0));
    let parse_errors = Arc::new(AtomicU64::new(0));
    let delivery_errors = Arc::new(AtomicU64::new(0));
    let start_time = std::time::Instant::now();

    // Graceful shutdown
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::mpsc::channel::<()>(1);
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        info!("Received shutdown signal, draining...");
        let _ = shutdown_tx.send(()).await;
    });

    // Periodic metrics reporter
    {
        let tp = tasks_processed.clone();
        let tf = tasks_failed.clone();
        let pe = parse_errors.clone();
        let de = delivery_errors.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30));
            loop {
                interval.tick().await;
                let proc = tp.load(Ordering::Relaxed);
                let fail = tf.load(Ordering::Relaxed);
                let p_err = pe.load(Ordering::Relaxed);
                let d_err = de.load(Ordering::Relaxed);
                let total = proc + fail + p_err;
                info!(
                    tasks_processed = proc,
                    tasks_failed = fail,
                    parse_errors = p_err,
                    delivery_errors = d_err,
                    total_tasks = total,
                    success_rate = if total > 0 {
                        proc as f64 / total as f64
                    } else {
                        0.0
                    },
                    uptime_seconds = start_time.elapsed().as_secs(),
                    "Periodic metrics"
                );
            }
        });
    }

    // Queue depth monitoring
    let rmq_for_depth = rabbitmq_client.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(10));
        loop {
            interval.tick().await;
            match rmq_for_depth.get_queue_length("ai_tasks").await {
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
    });

    // Main processing loop with semaphore-based concurrency
    loop {
        tokio::select! {
            delivery_result = consumer.next() => {
                let delivery = match delivery_result {
                    Some(Ok(d)) => d,
                    Some(Err(e)) => {
                        error!("Error receiving delivery: {}", e);
                        delivery_errors.fetch_add(1, Ordering::Relaxed);
                        continue;
                    }
                    None => break,
                };
                let task = match AITask::from_json_bytes(&delivery.data) {
                    Ok(t) => t,
                    Err(e) => {
                        error!("Failed to parse AI task: {}", e);
                        parse_errors.fetch_add(1, Ordering::Relaxed);
                        let _ = delivery
                            .nack(BasicNackOptions {
                                multiple: false,
                                requeue: false,
                            })
                            .await;
                        continue;
                    }
                };
                let game_id = task.game_id;
                let player_id = task.player_id;
                info!("Processing AI task: game={}, player={}", game_id, player_id);

                let permit = semaphore.clone().acquire_owned().await;
                let db = db_connection.clone();
                let redis = redis_client.clone();
                let rmq = rabbitmq_client.clone();
                let tp = tasks_processed.clone();
                let tf = tasks_failed.clone();
                let gl = game_locks.clone();

                tokio::spawn(async move {
                    let _permit = permit;
                    let task_start_time = std::time::Instant::now();
                    metrics::AI_TASKS_IN_FLIGHT.inc();

                    // Acquire per-game lock to serialize bot moves within the same game
                    let game_permit = {
                        let mut locks = gl.lock().await;
                        let game_sem = locks
                            .entry(game_id)
                            .or_insert_with(|| Arc::new(Semaphore::new(1)))
                            .clone();
                        game_sem.acquire_owned().await
                    };
                    let _game_permit = game_permit;

                    let result = process_bot_move(task, db, redis, Some(rmq)).await;

                    metrics::AI_TASKS_IN_FLIGHT.dec();
                    let duration = task_start_time.elapsed();
                    metrics::AI_TASK_DURATION_SECONDS
                        .with_label_values(&["ai_task"])
                        .observe(duration.as_secs_f64());

                    match result {
                        Ok(()) => {
                            tp.fetch_add(1, Ordering::Relaxed);
                            info!(
                                "Successfully processed bot move for game {}, player {} in {:?}",
                                game_id, player_id, duration
                            );
                            let _ = delivery.ack(BasicAckOptions::default()).await;
                        }
                        Err(e) => {
                            tf.fetch_add(1, Ordering::Relaxed);
                            error!("Failed to process bot move: {}", e);
                            let _ = delivery
                                .nack(BasicNackOptions {
                                    multiple: false,
                                    requeue: false,
                                })
                                .await;
                        }
                    }
                    // game_permit is dropped here, releasing the per-game lock
                });
            }
            _ = shutdown_rx.recv() => {
                info!("Shutdown signal received, stopping message consumption...");
                break;
            }
        }
    }

    // Drain in-flight tasks by acquiring all semaphore permits
    info!("Draining in-flight tasks...");
    let _drain_permits = semaphore.acquire_many(max_concurrent as u32).await;

    let uptime = start_time.elapsed();
    let proc = tasks_processed.load(Ordering::Relaxed);
    let fail = tasks_failed.load(Ordering::Relaxed);
    let p_err = parse_errors.load(Ordering::Relaxed);
    let d_err = delivery_errors.load(Ordering::Relaxed);
    let total = proc + fail + p_err;
    info!(
        tasks_processed = proc,
        tasks_failed = fail,
        parse_errors = p_err,
        delivery_errors = d_err,
        total_tasks = total,
        success_rate = if total > 0 {
            proc as f64 / total as f64
        } else {
            0.0
        },
        uptime_seconds = uptime.as_secs(),
        "AI worker shutting down - final metrics"
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use jambo_backend::game::constants::BOT_THINKING_DELAY_MS;

    #[test]
    fn test_bot_thinking_delay_default() {
        assert_eq!(*BOT_THINKING_DELAY_MS, 800);
    }

    #[test]
    fn test_default_db_pool_size() {
        let pool_size: u64 = std::env::var("AI_WORKER_DB_POOL_SIZE")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(100);
        assert_eq!(pool_size, 100);
    }

    #[test]
    fn test_default_max_concurrent() {
        let max_concurrent: usize = std::env::var("AI_WORKER_MAX_CONCURRENT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(50);
        assert_eq!(max_concurrent, 50);
    }

    #[test]
    fn test_semaphore_creation() {
        let max_concurrent = 10;
        let semaphore = Arc::new(Semaphore::new(max_concurrent));
        assert_eq!(semaphore.available_permits(), max_concurrent);
    }

    #[tokio::test]
    async fn test_semaphore_acquire_release() {
        let semaphore = Arc::new(Semaphore::new(2));
        let permit1 = semaphore.clone().acquire_owned().await;
        let permit2 = semaphore.clone().acquire_owned().await;
        assert_eq!(semaphore.available_permits(), 0);
        drop(permit1);
        assert_eq!(semaphore.available_permits(), 1);
        drop(permit2);
        assert_eq!(semaphore.available_permits(), 2);
    }

    #[test]
    fn test_atomic_counter_operations() {
        let counter = Arc::new(AtomicU64::new(0));
        counter.fetch_add(5, Ordering::Relaxed);
        assert_eq!(counter.load(Ordering::Relaxed), 5);
        counter.fetch_add(3, Ordering::Relaxed);
        assert_eq!(counter.load(Ordering::Relaxed), 8);
        counter.store(0, Ordering::Relaxed);
        assert_eq!(counter.load(Ordering::Relaxed), 0);
    }
}
