use lapin::{
    options::*, types::FieldTable, BasicProperties, Connection, ConnectionProperties, Consumer,
};
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

pub mod ai_task;
pub mod events;
pub mod redis;
pub use ai_task::AITask;
pub use redis::RedisClient;

const AI_TASKS_QUEUE: &str = "ai_tasks";
const MAX_RETRIES: u32 = 3;
const INITIAL_RETRY_DELAY_MS: u64 = 100;
const MAX_RETRY_DELAY_MS: u64 = 5000;

/// Connect to RabbitMQ with exponential backoff retry.
/// Shared between the main server and the ai-worker binary.
pub async fn connect_to_rabbitmq_with_retry(
    url: &str,
    max_retries: u32,
) -> Result<RabbitMQClient, lapin::Error> {
    let initial_delay_ms = 1000;
    let max_delay_ms = 30000;

    let mut last_error = None;

    for attempt in 0..max_retries {
        match RabbitMQClient::new(url).await {
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

/// Metrics for RabbitMQ operations
#[derive(Debug, Clone, Default)]
pub struct RabbitMQMetrics {
    pub publish_success_count: u64,
    pub publish_failure_count: u64,
    pub publish_retry_count: u64,
    pub consume_success_count: u64,
    pub consume_failure_count: u64,
    pub connection_error_count: u64,
}

#[derive(Clone)]
pub struct RabbitMQClient {
    connection: std::sync::Arc<Connection>,
    metrics: std::sync::Arc<std::sync::Mutex<RabbitMQMetrics>>,
}

impl RabbitMQClient {
    pub async fn new(url: &str) -> Result<Self, lapin::Error> {
        let connection = Connection::connect(url, ConnectionProperties::default()).await?;
        info!("Connected to RabbitMQ");
        Ok(Self {
            connection: std::sync::Arc::new(connection),
            metrics: std::sync::Arc::new(std::sync::Mutex::new(RabbitMQMetrics::default())),
        })
    }

    /// Get a copy of current metrics
    pub fn get_metrics(&self) -> RabbitMQMetrics {
        self.metrics.lock().unwrap().clone()
    }

    /// Reset metrics (useful for testing)
    #[allow(dead_code)]
    pub fn reset_metrics(&self) {
        let mut metrics = self.metrics.lock().unwrap();
        *metrics = RabbitMQMetrics::default();
    }

    /// Publish with exponential backoff retry
    pub async fn publish_with_retry(
        &self,
        queue: &str,
        message: &[u8],
    ) -> Result<(), lapin::Error> {
        let mut last_error = None;

        for attempt in 0..MAX_RETRIES {
            match self.publish_internal(queue, message).await {
                Ok(_) => {
                    let mut metrics = self.metrics.lock().unwrap();
                    metrics.publish_success_count += 1;
                    if attempt > 0 {
                        metrics.publish_retry_count += 1;
                        info!("Publish succeeded after {} retries", attempt);
                    }
                    return Ok(());
                }
                Err(e) => {
                    last_error = Some(e);

                    if attempt == MAX_RETRIES - 1 {
                        // Last attempt failed
                        let mut metrics = self.metrics.lock().unwrap();
                        metrics.publish_failure_count += 1;
                        break;
                    }

                    // Calculate exponential backoff delay
                    let delay_ms = std::cmp::min(
                        INITIAL_RETRY_DELAY_MS * 2u64.pow(attempt),
                        MAX_RETRY_DELAY_MS,
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

    /// Internal publish without retry
    async fn publish_internal(&self, queue: &str, message: &[u8]) -> Result<(), lapin::Error> {
        let channel = self.connection.create_channel().await?;
        let queue_name: lapin::types::ShortString = queue.into();
        let exchange: lapin::types::ShortString = "".into();

        // Declare queue (idempotent)
        let _ = channel
            .queue_declare(
                queue_name.clone(),
                QueueDeclareOptions::default(),
                FieldTable::default(),
            )
            .await?;

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

        // Declare queue (idempotent)
        let _ = channel
            .queue_declare(
                queue_name.clone(),
                QueueDeclareOptions::default(),
                FieldTable::default(),
            )
            .await?;

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

        let mut metrics = self.metrics.lock().unwrap();
        metrics.consume_success_count += 1;

        Ok(consumer)
    }

    /// Check if connection is still alive
    pub async fn check_health(&self) -> bool {
        match self.connection.status().connected() {
            true => {
                debug!("RabbitMQ connection is healthy");
                true
            }
            false => {
                error!("RabbitMQ connection is not healthy");
                let mut metrics = self.metrics.lock().unwrap();
                metrics.connection_error_count += 1;
                false
            }
        }
    }

    /// Get queue length (approximate)
    pub async fn get_queue_length(&self, queue: &str) -> Result<u32, lapin::Error> {
        let channel = self.connection.create_channel().await?;
        let queue_name: lapin::types::ShortString = queue.into();

        let queue_info = channel
            .queue_declare(
                queue_name,
                QueueDeclareOptions::default(),
                FieldTable::default(),
            )
            .await?;

        Ok(queue_info.message_count())
    }
}
