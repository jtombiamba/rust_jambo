use crate::messaging::events::{GameEvent, RoomEvent, UserEvent};
use jambo_core::observability::metrics::{
    REDIS_BUFFER_OVERFLOW_TOTAL, REDIS_PUBLISH_DURATION_SECONDS, REDIS_PUBLISH_FAILURES_TOTAL,
    REDIS_PUBLISH_RETRIES_TOTAL,
};
use redis::{AsyncCommands, Client, RedisResult};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tracing::{error, info};

/// Default number of pooled Redis connections created by [`RedisClient::new`].
/// Callers that apply [`RedisClient::with_config`] override this with
/// `Config::redis_pool_size`.
const DEFAULT_POOL_SIZE: usize = 16;

pub enum PublishResult {
    Published,
    RetryExhausted(String),
}

struct QueuedEvent {
    channel: String,
    payload: String,
}

/// Redis client wrapper that manages a connection pool, publish retries, and event buffering.
///
/// All commands are served from a [`deadpool_redis::Pool`] of multiplexed
/// connections instead of a single shared [`redis::aio::ConnectionManager`], so
/// concurrent commands (e.g. a burst of token `EXISTS` checks during a WebSocket
/// handshake storm) are no longer serialized behind one socket.
#[derive(Clone)]
pub struct RedisClient {
    client: Client,
    pool: deadpool_redis::Pool,
    publish_max_retries: u32,
    publish_retry_base_delay_ms: u64,
    publish_retry_max_delay_ms: u64,
    buffer: Arc<Mutex<VecDeque<QueuedEvent>>>,
    buffer_max_size: usize,
    flush_interval_secs: u64,
    flush_started: Arc<AtomicBool>,
}

impl RedisClient {
    pub async fn new(url: &str) -> RedisResult<Self> {
        let client = Client::open(url)?;
        let mut config = deadpool_redis::Config::from_url(url);
        config.pool = Some(deadpool_redis::PoolConfig {
            max_size: DEFAULT_POOL_SIZE,
            ..Default::default()
        });
        let pool = config
            .create_pool(Some(deadpool_redis::Runtime::Tokio1))
            .map_err(|e| {
                redis::RedisError::from((redis::ErrorKind::Io, "redis pool", e.to_string()))
            })?;
        info!("Connected to Redis at {}", url);
        Ok(Self {
            client,
            pool,
            publish_max_retries: 3,
            publish_retry_base_delay_ms: 100,
            publish_retry_max_delay_ms: 1000,
            buffer: Arc::new(Mutex::new(VecDeque::new())),
            buffer_max_size: 1000,
            flush_interval_secs: 5,
            flush_started: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn with_config(mut self, config: &jambo_core::config::Config) -> Self {
        self.publish_max_retries = config.redis_publish_max_retries;
        self.publish_retry_base_delay_ms = config.redis_publish_retry_base_delay_ms;
        self.publish_retry_max_delay_ms = config.redis_publish_retry_max_delay_ms;
        self.buffer_max_size = config.redis_event_buffer_max_size;
        self.flush_interval_secs = config.redis_event_buffer_flush_interval_secs;
        self.pool.resize(config.redis_pool_size.max(1));
        self
    }

    /// Borrow a pooled connection for a single command.
    async fn connection(&self) -> RedisResult<deadpool_redis::Connection> {
        self.pool.get().await.map_err(|e| {
            redis::RedisError::from((redis::ErrorKind::Io, "redis pool", e.to_string()))
        })
    }

    /// Get a string value by key.
    pub async fn get(&mut self, key: &str) -> RedisResult<Option<String>> {
        let mut conn = self.connection().await?;
        conn.get(key).await
    }

    /// Set a string value by key with TTL, only if key doesn't exist.
    /// Returns true if the key was set, false if it already existed.
    pub async fn set_nx_ex(&mut self, key: &str, value: &str, ttl_secs: u64) -> RedisResult<bool> {
        let mut conn = self.connection().await?;
        let result: Option<String> = redis::cmd("SET")
            .arg(key)
            .arg(value)
            .arg("NX")
            .arg("EX")
            .arg(ttl_secs)
            .query_async(&mut conn)
            .await?;
        Ok(result.is_some())
    }

    pub async fn compare_and_delete(&mut self, key: &str, expected: &str) -> RedisResult<bool> {
        let script = redis::Script::new(
            "if redis.call('get', KEYS[1]) == ARGV[1] then\n\
                 return redis.call('del', KEYS[1])\n\
             else\n\
                 return 0\n\
             end",
        );
        let mut conn = self.connection().await?;
        let result: i64 = script
            .key(key)
            .arg(expected)
            .invoke_async(&mut conn)
            .await?;
        Ok(result == 1)
    }

    /// Set a string value by key (no expiry).
    #[allow(dead_code)]
    pub async fn set(&mut self, key: &str, value: &str) -> RedisResult<()> {
        let mut conn = self.connection().await?;
        conn.set(key, value).await
    }

    /// Set a string value by key with TTL in seconds.
    pub async fn set_ex(&mut self, key: &str, value: &str, ttl_secs: u64) -> RedisResult<()> {
        let mut conn = self.connection().await?;
        conn.set_ex(key, value, ttl_secs).await
    }

    /// Atomically increment a counter and return the new value.
    #[allow(dead_code)]
    pub async fn incr(&mut self, key: &str) -> RedisResult<u64> {
        let mut conn = self.connection().await?;
        conn.incr(key, 1).await
    }

    /// Atomically increment a counter and set TTL if the key was newly created.
    /// Uses a Lua script to avoid the race between SET NX EX + INCR.
    pub async fn incr_with_expire(&mut self, key: &str, ttl_secs: u64) -> RedisResult<u64> {
        let script = redis::Script::new(
            "local current = redis.call('INCR', KEYS[1])\n\
             if current == 1 then redis.call('EXPIRE', KEYS[1], ARGV[1]) end\n\
             return current",
        );
        let mut conn = self.connection().await?;
        script.key(key).arg(ttl_secs).invoke_async(&mut conn).await
    }

    /// Delete one or more keys.
    pub async fn del(&mut self, key: &str) -> RedisResult<()> {
        let mut conn = self.connection().await?;
        conn.del(key).await
    }

    /// Check if a key exists.
    pub async fn exists(&mut self, key: &str) -> RedisResult<bool> {
        let mut conn = self.connection().await?;
        conn.exists(key).await
    }

    /// Set expiry on an existing key.
    #[allow(dead_code)]
    pub async fn expire(&mut self, key: &str, ttl_secs: u64) -> RedisResult<()> {
        let mut conn = self.connection().await?;
        conn.expire(key, ttl_secs as i64).await
    }

    /// Publish a message to a Redis channel.
    pub async fn publish(&mut self, channel: &str, message: &str) -> RedisResult<()> {
        let start = Instant::now();
        let mut conn = self.connection().await?;
        let result = conn.publish(channel, message).await;
        let duration = start.elapsed().as_secs_f64();
        REDIS_PUBLISH_DURATION_SECONDS
            .with_label_values(&[])
            .observe(duration);
        result
    }

    /// Publish a game event to its appropriate Redis channel.
    #[allow(dead_code)]
    pub async fn publish_game_event(&mut self, event: &GameEvent) -> RedisResult<()> {
        let channel = event.channel();
        let message = event.to_json();
        self.publish(&channel, &message).await
    }

    /// Publish a room event to its appropriate Redis channel.
    #[allow(dead_code)]
    pub async fn publish_room_event(&mut self, event: &RoomEvent) -> RedisResult<()> {
        let channel = event.channel();
        let message = event.to_json();
        self.publish(&channel, &message).await
    }

    pub async fn publish_with_retry(&mut self, channel: &str, message: &str) -> PublishResult {
        let mut attempt = 0u32;
        loop {
            match self.publish(channel, message).await {
                Ok(()) => return PublishResult::Published,
                Err(e) => {
                    attempt += 1;
                    if attempt >= self.publish_max_retries {
                        REDIS_PUBLISH_FAILURES_TOTAL
                            .with_label_values(&[channel.split(':').next().unwrap_or("unknown")])
                            .inc();
                        let err_msg = format!("{e}");
                        self.enqueue_event(channel, message);
                        return PublishResult::RetryExhausted(err_msg);
                    }
                    REDIS_PUBLISH_RETRIES_TOTAL
                        .with_label_values(&[channel.split(':').next().unwrap_or("unknown")])
                        .inc();
                    let delay = (self.publish_retry_base_delay_ms * 2u64.pow(attempt - 1))
                        .min(self.publish_retry_max_delay_ms);
                    tokio::time::sleep(Duration::from_millis(delay)).await;
                }
            }
        }
    }

    pub async fn publish_game_event_with_retry(&mut self, event: &GameEvent) -> PublishResult {
        let channel = event.channel();
        let message = event.to_json();
        self.publish_with_retry(&channel, &message).await
    }

    pub async fn publish_room_event_with_retry(&mut self, event: &RoomEvent) -> PublishResult {
        let channel = event.channel();
        let message = event.to_json();
        self.publish_with_retry(&channel, &message).await
    }

    pub async fn publish_user_event_with_retry(&mut self, event: &UserEvent) -> PublishResult {
        let channel = event.channel();
        let message = event.to_json();
        self.publish_with_retry(&channel, &message).await
    }

    fn enqueue_event(&mut self, channel: &str, payload: &str) {
        let mut buffer = self.buffer.lock().unwrap();
        if buffer.len() >= self.buffer_max_size {
            REDIS_BUFFER_OVERFLOW_TOTAL.inc();
            error!(
                "Redis event buffer overflow ({} events), dropping oldest event",
                self.buffer_max_size
            );
            buffer.pop_front();
        }
        buffer.push_back(QueuedEvent {
            channel: channel.to_string(),
            payload: payload.to_string(),
        });
        self.ensure_flush_task();
    }

    fn ensure_flush_task(&self) {
        if self.flush_started.swap(true, Ordering::SeqCst) {
            return;
        }

        let buffer = self.buffer.clone();
        let client = self.client.clone();
        let flush_interval = Duration::from_secs(self.flush_interval_secs);
        let publish_max_retries = self.publish_max_retries;
        let publish_retry_base_delay_ms = self.publish_retry_base_delay_ms;
        let publish_retry_max_delay_ms = self.publish_retry_max_delay_ms;

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(flush_interval);
            loop {
                interval.tick().await;
                let events: Vec<QueuedEvent> = {
                    let mut buf = buffer.lock().unwrap();
                    buf.drain(..).collect()
                };
                if events.is_empty() {
                    continue;
                }
                info!("Flushing {} buffered Redis events", events.len());

                let mut conn = match client.get_connection_manager().await {
                    Ok(conn) => conn,
                    Err(e) => {
                        error!("Failed to get Redis connection for flush: {}", e);
                        let mut buf = buffer.lock().unwrap();
                        for event in events {
                            if buf.len() >= 1000 {
                                buf.pop_front();
                            }
                            buf.push_back(event);
                        }
                        continue;
                    }
                };

                for event in events {
                    let mut attempt = 0u32;
                    loop {
                        match conn.publish(&event.channel, &event.payload).await {
                            Ok(()) => break,
                            Err(e) => {
                                attempt += 1;
                                if attempt >= publish_max_retries {
                                    REDIS_PUBLISH_FAILURES_TOTAL
                                        .with_label_values(&["flush"])
                                        .inc();
                                    error!(
                                        "Failed to flush event to channel {} after {} retries: {}",
                                        event.channel, publish_max_retries, e
                                    );
                                    let mut buf = buffer.lock().unwrap();
                                    if buf.len() >= 1000 {
                                        buf.pop_front();
                                    }
                                    buf.push_back(event);
                                    break;
                                }
                                let delay = (publish_retry_base_delay_ms * 2u64.pow(attempt - 1))
                                    .min(publish_retry_max_delay_ms);
                                tokio::time::sleep(Duration::from_millis(delay)).await;
                            }
                        }
                    }
                }
            }
        });
    }

    /// Subscribe to a Redis channel and return a subscription object.
    /// This is a simplified subscription that yields messages as they arrive.
    #[allow(dead_code)]
    pub async fn subscribe(&mut self, channels: &[&str]) -> RedisResult<redis::aio::PubSub> {
        let mut pubsub: redis::aio::PubSub = self.client.get_async_pubsub().await?;
        for channel in channels {
            pubsub.subscribe(*channel).await?;
        }
        Ok(pubsub)
    }

    /// Delete all keys matching a glob pattern using SCAN + DEL.
    /// Returns the number of keys deleted.
    pub async fn del_pattern(&mut self, pattern: &str) -> RedisResult<u64> {
        let mut cursor = 0u64;
        let mut deleted = 0u64;
        let mut conn = self.connection().await?;
        loop {
            let (next_cursor, keys): (u64, Vec<String>) = redis::cmd("SCAN")
                .arg(cursor)
                .arg("MATCH")
                .arg(pattern)
                .arg("COUNT")
                .arg(100)
                .query_async(&mut conn)
                .await?;
            if !keys.is_empty() {
                let count: u64 = redis::cmd("DEL")
                    .arg(keys.as_slice())
                    .query_async(&mut conn)
                    .await?;
                deleted += count;
            }
            cursor = next_cursor;
            if cursor == 0 {
                break;
            }
        }
        Ok(deleted)
    }

    /// Subscribe to Redis patterns and return a subscription object.
    pub async fn psubscribe(&mut self, patterns: &[&str]) -> RedisResult<redis::aio::PubSub> {
        let mut pubsub: redis::aio::PubSub = self.client.get_async_pubsub().await?;
        for pattern in patterns {
            pubsub.psubscribe(*pattern).await?;
        }
        Ok(pubsub)
    }

    #[allow(dead_code)]
    pub async fn zadd(&mut self, key: &str, member: String, score: f64) -> RedisResult<()> {
        let mut conn = self.connection().await?;
        conn.zadd(key, member, score).await
    }

    pub async fn zrevrange_withscores(
        &mut self,
        key: &str,
        start: isize,
        stop: isize,
    ) -> RedisResult<Vec<(String, f64)>> {
        let mut conn = self.connection().await?;
        conn.zrevrange_withscores(key, start, stop).await
    }

    pub async fn zrevrank(&mut self, key: &str, member: String) -> RedisResult<Option<u64>> {
        let mut conn = self.connection().await?;
        conn.zrevrank(key, member).await
    }

    pub async fn mget(&mut self, keys: &[String]) -> RedisResult<Vec<Option<String>>> {
        let mut conn = self.connection().await?;
        redis::cmd("MGET").arg(keys).query_async(&mut conn).await
    }
}
