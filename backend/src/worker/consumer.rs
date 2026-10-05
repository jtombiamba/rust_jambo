//! RabbitMQ consumer loop with automatic reconnection.
//!
//! The outer loop ([`Worker::run_consumer_loop`]) reconnects whenever the
//! connection drops, while the inner loop ([`Worker::consume`]) processes
//! deliveries until the connection is lost or a shutdown is requested.

use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use lapin::message::Delivery;
use lapin::options::{BasicAckOptions, BasicNackOptions};
use lapin::Consumer;
use tokio::sync::Semaphore;
use tracing::{error, info, warn};

use crate::game::worker_core::process_bot_move;
use crate::messaging::{AITask, RabbitMQClient};
use crate::observability::{metrics, propagation};

use super::Worker;

/// Why the inner consume loop stopped.
enum ConsumeOutcome {
    /// A graceful shutdown was requested.
    Shutdown,
    /// The RabbitMQ connection was lost and the consumer must be recreated.
    ConnectionLost(String),
}

impl Worker {
    /// Runs the outer reconnect loop: connect, consume, and reconnect whenever
    /// the connection drops — until shutdown is requested. A single exponential
    /// backoff is shared across failed connects and lost connections.
    pub(super) async fn run_consumer_loop(&self) {
        let initial_delay = Duration::from_millis(self.runtime.reconnect_initial_delay_ms);
        let max_delay = Duration::from_millis(self.runtime.reconnect_max_delay_ms);
        let mut delay = initial_delay;

        loop {
            if self.shutdown.signal().is_shutdown() {
                info!("Shutdown requested before connecting; stopping consumer loop.");
                return;
            }

            match RabbitMQClient::new(&self.config.rabbitmq_url, self.publish_config.clone()).await
            {
                Ok(client) => {
                    // A successful connection resets the backoff.
                    delay = initial_delay;
                    client.check_health().await;
                    *self.current_client.write().await = Some(client.clone());

                    match client.consume_ai_tasks().await {
                        Ok(consumer) => {
                            info!("Consumer ready; waiting for AI tasks...");
                            match self.consume(client.clone(), consumer).await {
                                ConsumeOutcome::Shutdown => return,
                                ConsumeOutcome::ConnectionLost(reason) => {
                                    warn!(reason, "RabbitMQ consumer loop ended; reconnecting...");
                                    self.mark_unhealthy().await;
                                    delay = self.wait_before_reconnect(delay, max_delay).await;
                                }
                            }
                        }
                        Err(e) => {
                            error!("Failed to start consuming AI tasks: {e}; reconnecting...");
                            self.mark_unhealthy().await;
                            delay = self.wait_before_reconnect(delay, max_delay).await;
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to connect to RabbitMQ: {e}; retrying in {delay:?}...");
                    if self.sleep_until_shutdown(delay).await {
                        return;
                    }
                    delay = (delay * 2).min(max_delay);
                }
            }
        }
    }

    /// Consumes deliveries until the connection drops or shutdown is requested.
    async fn consume(&self, client: RabbitMQClient, mut consumer: Consumer) -> ConsumeOutcome {
        let shutdown = self.shutdown.signal();
        loop {
            tokio::select! {
                _ = shutdown.wait() => {
                    return ConsumeOutcome::Shutdown;
                }
                delivery = consumer.next() => {
                    match delivery {
                        Some(Ok(delivery)) => {
                            self.dispatch_delivery(client.clone(), delivery).await;
                        }
                        Some(Err(e)) => {
                            error!("Error receiving delivery: {e}");
                            self.counters.inc_delivery_error();
                            // If the connection itself is dead, stop consuming so the
                            // outer loop can reconnect. Otherwise keep going.
                            if !client.check_health().await {
                                return ConsumeOutcome::ConnectionLost(format!("{e}"));
                            }
                        }
                        None => {
                            // The consumer stream ends when the channel/connection is
                            // closed — reconnect.
                            return ConsumeOutcome::ConnectionLost(
                                "consumer stream ended".to_string(),
                            );
                        }
                    }
                }
            }
        }
    }

    /// Parses a delivery and spawns a task to process it, using the concurrency
    /// semaphore as backpressure.
    async fn dispatch_delivery(&self, client: RabbitMQClient, delivery: Delivery) {
        let task = match AITask::from_json_bytes(&delivery.data) {
            Ok(task) => task,
            Err(e) => {
                error!("Failed to parse AI task: {e}");
                self.counters.inc_parse_error();
                let _ = delivery
                    .nack(BasicNackOptions {
                        multiple: false,
                        requeue: false,
                    })
                    .await;
                return;
            }
        };

        let game_id = task.game_id;
        let player_id = task.player_id;
        info!("Processing AI task: game={game_id}, player={player_id}");

        // Acquire a permit *before* spawning so the consume loop applies
        // backpressure and never spawns unbounded tasks.
        let permit = self.semaphore.clone().acquire_owned().await;

        let db = self.db.clone();
        let redis = self.redis.clone();
        let rmq = client.clone();
        let counters = self.counters.clone();
        let game_locks = self.game_locks.clone();

        tokio::spawn(async move {
            let _permit = permit;
            let task_start_time = std::time::Instant::now();
            metrics::AI_TASKS_IN_FLIGHT.inc();

            // Serialize bot moves within the same game.
            let game_permit = {
                let mut locks = game_locks.lock().await;
                let game_sem = locks
                    .entry(game_id)
                    .or_insert_with(|| Arc::new(Semaphore::new(1)))
                    .clone();
                game_sem.acquire_owned().await
            };
            let _game_permit = game_permit;

            let parent_context = propagation::extract_context(delivery.properties.headers());

            let result = process_bot_move(task, db, redis, Some(rmq), parent_context).await;

            metrics::AI_TASKS_IN_FLIGHT.dec();
            let duration = task_start_time.elapsed();
            metrics::AI_TASK_DURATION_SECONDS
                .with_label_values(&["ai_task"])
                .observe(duration.as_secs_f64());

            match result {
                Ok(()) => {
                    counters.inc_processed();
                    info!(
                        "Successfully processed bot move for game {game_id}, player {player_id} in {duration:?}"
                    );
                    let _ = delivery.ack(BasicAckOptions::default()).await;
                }
                Err(e) => {
                    counters.inc_failed();
                    error!("Failed to process bot move: {e}");
                    let _ = delivery
                        .nack(BasicNackOptions {
                            multiple: false,
                            requeue: false,
                        })
                        .await;
                }
            }
        });
    }

    /// Marks the connection unhealthy and clears the shared client slot.
    async fn mark_unhealthy(&self) {
        *self.current_client.write().await = None;
        metrics::RABBITMQ_HEALTHY.set(0.0);
    }

    /// Sleeps before the next reconnect attempt, doubling the delay up to
    /// `max_delay`. On shutdown it returns the delay unchanged (the caller
    /// re-checks shutdown at the top of the loop).
    async fn wait_before_reconnect(&self, delay: Duration, max_delay: Duration) -> Duration {
        if self.sleep_until_shutdown(delay).await {
            return delay;
        }
        (delay * 2).min(max_delay)
    }

    /// Sleeps for `duration`, but wakes early (returning `true`) if shutdown is
    /// requested. Returns `false` if the sleep completed normally.
    async fn sleep_until_shutdown(&self, duration: Duration) -> bool {
        let shutdown = self.shutdown.signal();
        tokio::select! {
            _ = tokio::time::sleep(duration) => false,
            _ = shutdown.wait() => true,
        }
    }
}
