use crate::observability::metrics;
use lapin::options::BasicQosOptions;
use lapin::{
    options::*, types::FieldTable, BasicProperties, Connection, ConnectionProperties, Consumer,
};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

pub mod ai_task;
pub mod events;
pub mod redis;
pub use ai_task::AITask;
pub use redis::RedisClient;

const AI_TASKS_QUEUE: &str = "ai_tasks";
const AI_TASKS_DLX: &str = "ai_tasks_dlx";
const AI_TASKS_DLQ: &str = "ai_tasks_dlq";

#[derive(Debug, Clone)]
pub struct RabbitMQPublishConfig {
    pub max_retries: u32,
    pub initial_retry_delay_ms: u64,
    pub max_retry_delay_ms: u64,
    pub circuit_breaker_failure_threshold: u32,
    pub circuit_breaker_cooldown_secs: u64,
}

impl Default for RabbitMQPublishConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_retry_delay_ms: 100,
            max_retry_delay_ms: 5000,
            circuit_breaker_failure_threshold: 5,
            circuit_breaker_cooldown_secs: 30,
        }
    }
}

impl RabbitMQPublishConfig {
    #[allow(dead_code)]
    pub fn from_env() -> Self {
        use std::env;
        Self {
            max_retries: env::var("RABBITMQ_PUBLISH_MAX_RETRIES")
                .unwrap_or_else(|_| "3".to_string())
                .parse()
                .unwrap_or(3),
            initial_retry_delay_ms: env::var("RABBITMQ_PUBLISH_INITIAL_RETRY_DELAY_MS")
                .unwrap_or_else(|_| "100".to_string())
                .parse()
                .unwrap_or(100),
            max_retry_delay_ms: env::var("RABBITMQ_PUBLISH_MAX_RETRY_DELAY_MS")
                .unwrap_or_else(|_| "5000".to_string())
                .parse()
                .unwrap_or(5000),
            circuit_breaker_failure_threshold: env::var("CIRCUIT_BREAKER_FAILURE_THRESHOLD")
                .unwrap_or_else(|_| "5".to_string())
                .parse()
                .unwrap_or(5),
            circuit_breaker_cooldown_secs: env::var("CIRCUIT_BREAKER_COOLDOWN_SECS")
                .unwrap_or_else(|_| "30".to_string())
                .parse()
                .unwrap_or(30),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

#[derive(Debug)]
struct CircuitBreaker {
    state: tokio::sync::Mutex<CircuitState>,
    consecutive_failures: AtomicU32,
    last_failure_time: tokio::sync::Mutex<Option<std::time::Instant>>,
    cooldown_start: tokio::sync::Mutex<Option<std::time::Instant>>,
    failure_threshold: u32,
    cooldown_duration: Duration,
}

impl CircuitBreaker {
    fn new(failure_threshold: u32, cooldown_secs: u64) -> Self {
        Self {
            state: tokio::sync::Mutex::new(CircuitState::Closed),
            consecutive_failures: AtomicU32::new(0),
            last_failure_time: tokio::sync::Mutex::new(None),
            cooldown_start: tokio::sync::Mutex::new(None),
            failure_threshold,
            cooldown_duration: Duration::from_secs(cooldown_secs),
        }
    }

    async fn record_success(&self) {
        self.consecutive_failures.store(0, Ordering::SeqCst);
        let mut state = self.state.lock().await;
        if *state == CircuitState::HalfOpen {
            *state = CircuitState::Closed;
            metrics::CIRCUIT_BREAKER_STATE.set(0.0);
            info!("Circuit breaker transitioned to Closed");
        }
    }

    async fn record_failure(&self) {
        let failures = self.consecutive_failures.fetch_add(1, Ordering::SeqCst) + 1;
        {
            let mut last_time = self.last_failure_time.lock().await;
            *last_time = Some(std::time::Instant::now());
        }

        if failures >= self.failure_threshold {
            let mut state = self.state.lock().await;
            if *state == CircuitState::Closed {
                *state = CircuitState::Open;
                let mut cooldown = self.cooldown_start.lock().await;
                *cooldown = Some(std::time::Instant::now());
                metrics::CIRCUIT_BREAKER_STATE.set(1.0);
                warn!(
                    "Circuit breaker OPEN after {} consecutive failures",
                    failures
                );
            }
        }
    }

    async fn allow_request(&self) -> bool {
        let state = *self.state.lock().await;
        match state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                let cooldown = self.cooldown_start.lock().await;
                if let Some(start) = *cooldown {
                    if start.elapsed() >= self.cooldown_duration {
                        drop(cooldown);
                        let mut s = self.state.lock().await;
                        *s = CircuitState::HalfOpen;
                        metrics::CIRCUIT_BREAKER_STATE.set(2.0);
                        info!("Circuit breaker transitioned to HalfOpen");
                        return true;
                    }
                }
                false
            }
            CircuitState::HalfOpen => true,
        }
    }
}

/// Connect to RabbitMQ with exponential backoff retry.
/// Shared between the main server and the ai-worker binary.
#[allow(dead_code)]
pub async fn connect_to_rabbitmq_with_retry(
    url: &str,
    max_retries: u32,
    publish_config: RabbitMQPublishConfig,
) -> Result<RabbitMQClient, lapin::Error> {
    let initial_delay_ms = 1000;
    let max_delay_ms = 30000;

    let mut last_error = None;

    for attempt in 0..max_retries {
        match RabbitMQClient::new(url, publish_config.clone()).await {
            Ok(client) => {
                if attempt > 0 {
                    info!(
                        "Successfully connected to RabbitMQ after {} retries",
                        attempt
                    );
                }
                return Ok(client);
            }
            Err(e) => {
                last_error = Some(e);
                if attempt == max_retries - 1 {
                    error!(
                        "Failed to connect to RabbitMQ after {} attempts",
                        max_retries
                    );
                    break;
                }
                let delay_ms = std::cmp::min(initial_delay_ms * 2u64.pow(attempt), max_delay_ms);
                warn!(
                    "RabbitMQ connection attempt {} failed. Retrying in {}ms...",
                    attempt + 1,
                    delay_ms
                );
                sleep(Duration::from_millis(delay_ms)).await;
            }
        }
    }

    Err(last_error.unwrap())
}

/// Metrics for RabbitMQ operations (lock-free with AtomicU64)
#[derive(Debug)]
#[allow(dead_code)]
pub struct RabbitMQMetrics {
    pub publish_success_count: AtomicU64,
    pub publish_failure_count: AtomicU64,
    pub publish_retry_count: AtomicU64,
    pub consume_success_count: AtomicU64,
    pub consume_failure_count: AtomicU64,
    pub connection_error_count: AtomicU64,
}

impl Default for RabbitMQMetrics {
    fn default() -> Self {
        Self {
            publish_success_count: AtomicU64::new(0),
            publish_failure_count: AtomicU64::new(0),
            publish_retry_count: AtomicU64::new(0),
            consume_success_count: AtomicU64::new(0),
            consume_failure_count: AtomicU64::new(0),
            connection_error_count: AtomicU64::new(0),
        }
    }
}

#[derive(Clone)]
pub struct RabbitMQClient {
    connection: std::sync::Arc<Connection>,
    metrics: std::sync::Arc<RabbitMQMetrics>,
    circuit_breaker: std::sync::Arc<CircuitBreaker>,
    publish_config: RabbitMQPublishConfig,
    /// Cached channel reused across publishes to avoid create_channel() per call.
    /// Uses tokio::sync::Mutex since channel operations are async.
    cached_channel: std::sync::Arc<tokio::sync::Mutex<Option<lapin::Channel>>>,
}

impl RabbitMQClient {
    pub async fn new(
        url: &str,
        publish_config: RabbitMQPublishConfig,
    ) -> Result<Self, lapin::Error> {
        let connection = Connection::connect(url, ConnectionProperties::default()).await?;
        info!("Connected to RabbitMQ");
        let cb = CircuitBreaker::new(
            publish_config.circuit_breaker_failure_threshold,
            publish_config.circuit_breaker_cooldown_secs,
        );
        Ok(Self {
            connection: std::sync::Arc::new(connection),
            metrics: std::sync::Arc::new(RabbitMQMetrics::default()),
            circuit_breaker: std::sync::Arc::new(cb),
            publish_config,
            cached_channel: std::sync::Arc::new(tokio::sync::Mutex::new(None)),
        })
    }

    /// Get a snapshot of current metrics
    #[allow(dead_code)]
    pub fn get_metrics_snapshot(&self) -> serde_json::Value {
        serde_json::json!({
            "publish_success_count": self.metrics.publish_success_count.load(Ordering::Relaxed),
            "publish_failure_count": self.metrics.publish_failure_count.load(Ordering::Relaxed),
            "publish_retry_count": self.metrics.publish_retry_count.load(Ordering::Relaxed),
            "consume_success_count": self.metrics.consume_success_count.load(Ordering::Relaxed),
            "consume_failure_count": self.metrics.consume_failure_count.load(Ordering::Relaxed),
            "connection_error_count": self.metrics.connection_error_count.load(Ordering::Relaxed),
        })
    }

    /// Reset metrics (useful for testing)
    #[allow(dead_code)]
    pub fn reset_metrics(&self) {
        self.metrics
            .publish_success_count
            .store(0, Ordering::Relaxed);
        self.metrics
            .publish_failure_count
            .store(0, Ordering::Relaxed);
        self.metrics.publish_retry_count.store(0, Ordering::Relaxed);
        self.metrics
            .consume_success_count
            .store(0, Ordering::Relaxed);
        self.metrics
            .consume_failure_count
            .store(0, Ordering::Relaxed);
        self.metrics
            .connection_error_count
            .store(0, Ordering::Relaxed);
    }

    /// Publish with exponential backoff retry
    pub async fn publish_with_retry(
        &self,
        queue: &str,
        message: &[u8],
    ) -> Result<(), lapin::Error> {
        if !self.circuit_breaker.allow_request().await {
            warn!(
                "Circuit breaker is open, refusing publish to queue '{}'",
                queue
            );
            return Err(lapin::ErrorKind::InvalidChannelState(
                lapin::ChannelState::Error,
                "circuit breaker is open",
            )
            .into());
        }

        let mut last_error = None;

        for attempt in 0..self.publish_config.max_retries {
            match self.publish_internal(queue, message).await {
                Ok(_) => {
                    self.circuit_breaker.record_success().await;
                    self.metrics
                        .publish_success_count
                        .fetch_add(1, Ordering::Relaxed);
                    if attempt > 0 {
                        self.metrics
                            .publish_retry_count
                            .fetch_add(1, Ordering::Relaxed);
                        info!("Publish succeeded after {} retries", attempt);
                    }
                    metrics::RABBITMQ_PUBLISH_TOTAL
                        .with_label_values(&[queue])
                        .inc();
                    return Ok(());
                }
                Err(e) => {
                    last_error = Some(e);

                    if attempt == self.publish_config.max_retries - 1 {
                        self.circuit_breaker.record_failure().await;
                        self.metrics
                            .publish_failure_count
                            .fetch_add(1, Ordering::Relaxed);
                        metrics::RABBITMQ_PUBLISH_ERRORS_TOTAL
                            .with_label_values(&[queue])
                            .inc();
                        break;
                    }

                    // Calculate exponential backoff delay
                    let delay_ms = std::cmp::min(
                        self.publish_config.initial_retry_delay_ms * 2u64.pow(attempt),
                        self.publish_config.max_retry_delay_ms,
                    );

                    warn!(
                        "Publish attempt {} failed: {:?}. Retrying in {}ms",
                        attempt + 1,
                        last_error,
                        delay_ms
                    );
                    sleep(Duration::from_millis(delay_ms)).await;
                }
            }
        }

        Err(last_error.unwrap())
    }

    /// Internal publish without retry.
    /// Uses a cached channel to avoid creating a new channel per publish,
    /// which reduces network round-trips and worker blocking.
    async fn publish_internal(&self, queue: &str, message: &[u8]) -> Result<(), lapin::Error> {
        let queue_name: lapin::types::ShortString = queue.into();
        let exchange: lapin::types::ShortString = "".into();

        // Get or create the cached channel
        let channel = {
            let mut guard = self.cached_channel.lock().await;
            match guard.as_ref() {
                Some(ch) if ch.status().connected() => ch.clone(),
                _ => {
                    let ch = self.connection.create_channel().await?;
                    // Declare the queue once on channel creation so it exists
                    let queue_args = if queue == AI_TASKS_QUEUE {
                        let mut args = FieldTable::default();
                        args.insert(
                            "x-dead-letter-exchange".into(),
                            lapin::types::AMQPValue::LongString(AI_TASKS_DLX.into()),
                        );
                        args.insert(
                            "x-dead-letter-routing-key".into(),
                            lapin::types::AMQPValue::LongString(AI_TASKS_DLQ.into()),
                        );
                        args
                    } else {
                        FieldTable::default()
                    };
                    let _ = ch
                        .queue_declare(
                            queue_name.clone(),
                            QueueDeclareOptions::default(),
                            queue_args,
                        )
                        .await?;
                    *guard = Some(ch.clone());
                    ch
                }
            }
        };

        // Publish message
        let start_time = std::time::Instant::now();
        let result = channel
            .basic_publish(
                exchange,
                queue_name,
                BasicPublishOptions::default(),
                message,
                BasicProperties::default(),
            )
            .await;

        let duration = start_time.elapsed();
        debug!("Publish to queue '{}' took {:?}", queue, duration);

        result.map(|_| ())
    }

    /// Publish message (with retry)
    #[allow(dead_code)]
    pub async fn publish(&self, queue: &str, message: &[u8]) -> Result<(), lapin::Error> {
        self.publish_with_retry(queue, message).await
    }

    /// Publish AI task (with retry)
    pub async fn publish_ai_task(&self, task: &AITask) -> Result<(), lapin::Error> {
        let span = tracing::info_span!(
            "publish_ai_task",
            correlation_id = %task.correlation_id.map(|id| id.to_string()).unwrap_or_default(),
            game_id = %task.game_id,
            player_id = %task.player_id,
        );
        let _guard = span.enter();
        self.publish_with_retry(AI_TASKS_QUEUE, &task.to_json_bytes())
            .await
    }

    /// Consume AI tasks from queue
    #[allow(dead_code)]
    pub async fn consume_ai_tasks(&self) -> Result<Consumer, lapin::Error> {
        let start_time = std::time::Instant::now();

        let channel = self.connection.create_channel().await?;
        let queue_name: lapin::types::ShortString = AI_TASKS_QUEUE.into();

        let queue_args = {
            let mut args = FieldTable::default();
            args.insert(
                "x-dead-letter-exchange".into(),
                lapin::types::AMQPValue::LongString(AI_TASKS_DLX.into()),
            );
            args.insert(
                "x-dead-letter-routing-key".into(),
                lapin::types::AMQPValue::LongString(AI_TASKS_DLQ.into()),
            );
            args
        };

        let _ = channel
            .queue_declare(
                queue_name.clone(),
                QueueDeclareOptions::default(),
                queue_args,
            )
            .await?;

        channel.basic_qos(50, BasicQosOptions::default()).await?;

        // Create consumer
        let consumer = channel
            .basic_consume(
                queue_name,
                "ai_worker".into(),
                BasicConsumeOptions::default(),
                FieldTable::default(),
            )
            .await?;

        let duration = start_time.elapsed();
        debug!("Consumer setup took {:?}", duration);

        self.metrics
            .consume_success_count
            .fetch_add(1, Ordering::Relaxed);

        metrics::RABBITMQ_CONSUME_TOTAL.inc();

        Ok(consumer)
    }

    /// Check if connection is still alive
    #[allow(dead_code)]
    pub async fn check_health(&self) -> bool {
        match self.connection.status().connected() {
            true => {
                debug!("RabbitMQ connection is healthy");
                metrics::RABBITMQ_HEALTHY.set(1.0);
                true
            }
            false => {
                error!("RabbitMQ connection is not healthy");
                self.metrics
                    .connection_error_count
                    .fetch_add(1, Ordering::Relaxed);
                metrics::RABBITMQ_HEALTHY.set(0.0);
                false
            }
        }
    }

    /// Get queue length (approximate)
    #[allow(dead_code)]
    pub async fn get_queue_length(&self, queue: &str) -> Result<u32, lapin::Error> {
        let channel = self.connection.create_channel().await?;
        let queue_name: lapin::types::ShortString = queue.into();

        // Must pass matching arguments for queues that have special configuration
        // (e.g., x-dead-letter-exchange for ai_tasks) to avoid precondition_failed errors
        let queue_args = if queue == AI_TASKS_QUEUE {
            let mut args = FieldTable::default();
            args.insert(
                "x-dead-letter-exchange".into(),
                lapin::types::AMQPValue::LongString(AI_TASKS_DLX.into()),
            );
            args.insert(
                "x-dead-letter-routing-key".into(),
                lapin::types::AMQPValue::LongString(AI_TASKS_DLQ.into()),
            );
            args
        } else {
            FieldTable::default()
        };

        let queue_info = channel
            .queue_declare(queue_name, QueueDeclareOptions::default(), queue_args)
            .await?;

        let count = queue_info.message_count();
        metrics::RABBITMQ_QUEUE_LENGTH.set(count as f64);

        Ok(count)
    }
}
