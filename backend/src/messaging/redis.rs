use crate::messaging::events::{GameEvent, RoomEvent};
use crate::observability::metrics::REDIS_PUBLISH_DURATION_SECONDS;
use redis::{aio::ConnectionManager, AsyncCommands, Client, RedisResult};
use std::time::Instant;
use tracing::info;

/// Redis client wrapper that manages a connection pool.
#[derive(Clone)]
pub struct RedisClient {
    client: Client,
    connection_manager: ConnectionManager,
}

impl RedisClient {
    /// Create a new Redis client from a URL string.
    /// The URL should be in the format `redis://[:password@]host[:port][/db]`.
    pub async fn new(url: &str) -> RedisResult<Self> {
        let client = Client::open(url)?;
        let connection_manager = client.get_connection_manager().await?;
        info!("Connected to Redis at {}", url);
        Ok(Self {
            client,
            connection_manager,
        })
    }

    /// Get a string value by key.
    pub async fn get(&mut self, key: &str) -> RedisResult<Option<String>> {
        self.connection_manager.get(key).await
    }

    /// Set a string value by key with TTL, only if key doesn't exist.
    /// Returns true if the key was set, false if it already existed.
    pub async fn set_nx_ex(&mut self, key: &str, value: &str, ttl_secs: u64) -> RedisResult<bool> {
        let result: Option<String> = redis::cmd("SET")
            .arg(key)
            .arg(value)
            .arg("NX")
            .arg("EX")
            .arg(ttl_secs)
            .query_async(&mut self.connection_manager)
            .await?;
        Ok(result.is_some())
    }

    /// Set a string value by key (no expiry).
    #[allow(dead_code)]
    pub async fn set(&mut self, key: &str, value: &str) -> RedisResult<()> {
        self.connection_manager.set(key, value).await
    }

    /// Set a string value by key with TTL in seconds.
    pub async fn set_ex(&mut self, key: &str, value: &str, ttl_secs: u64) -> RedisResult<()> {
        self.connection_manager.set_ex(key, value, ttl_secs).await
    }

    /// Atomically increment a counter and return the new value.
    #[allow(dead_code)]
    pub async fn incr(&mut self, key: &str) -> RedisResult<u64> {
        self.connection_manager.incr(key, 1).await
    }

    /// Atomically increment a counter and set TTL if the key was newly created.
    /// Uses a Lua script to avoid the race between SET NX EX + INCR.
    pub async fn incr_with_expire(&mut self, key: &str, ttl_secs: u64) -> RedisResult<u64> {
        let script = redis::Script::new(
            "local current = redis.call('INCR', KEYS[1])\n\
             if current == 1 then redis.call('EXPIRE', KEYS[1], ARGV[1]) end\n\
             return current",
        );
        script
            .key(key)
            .arg(ttl_secs)
            .invoke_async(&mut self.connection_manager)
            .await
    }

    /// Delete one or more keys.
    pub async fn del(&mut self, key: &str) -> RedisResult<()> {
        self.connection_manager.del(key).await
    }

    /// Check if a key exists.
    pub async fn exists(&mut self, key: &str) -> RedisResult<bool> {
        self.connection_manager.exists(key).await
    }

    /// Set expiry on an existing key.
    #[allow(dead_code)]
    pub async fn expire(&mut self, key: &str, ttl_secs: u64) -> RedisResult<()> {
        self.connection_manager.expire(key, ttl_secs as i64).await
    }

    /// Publish a message to a Redis channel.
    pub async fn publish(&mut self, channel: &str, message: &str) -> RedisResult<()> {
        let start = Instant::now();
        let result = self.connection_manager.publish(channel, message).await;
        let duration = start.elapsed().as_secs_f64();
        REDIS_PUBLISH_DURATION_SECONDS
            .with_label_values(&[])
            .observe(duration);
        result
    }

    /// Publish a game event to its appropriate Redis channel.
    pub async fn publish_game_event(&mut self, event: &GameEvent) -> RedisResult<()> {
        let channel = event.channel();
        let message = event.to_json();
        self.publish(&channel, &message).await
    }

    /// Publish a room event to its appropriate Redis channel.
    pub async fn publish_room_event(&mut self, event: &RoomEvent) -> RedisResult<()> {
        let channel = event.channel();
        let message = event.to_json();
        self.publish(&channel, &message).await
    }

    /// Subscribe to a Redis channel and return a subscription object.
    /// This is a simplified subscription that yields messages as they arrive.
    #[allow(dead_code)]
    pub async fn subscribe(&mut self, channels: &[&str]) -> RedisResult<redis::aio::PubSub> {
        let mut pubsub: redis::aio::PubSub =
            self.client.get_async_connection().await?.into_pubsub();
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
        loop {
            let (next_cursor, keys): (u64, Vec<String>) = redis::cmd("SCAN")
                .arg(cursor)
                .arg("MATCH")
                .arg(pattern)
                .arg("COUNT")
                .arg(100)
                .query_async(&mut self.connection_manager)
                .await?;
            if !keys.is_empty() {
                let count: u64 = redis::cmd("DEL")
                    .arg(keys.as_slice())
                    .query_async(&mut self.connection_manager)
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
        let mut pubsub: redis::aio::PubSub =
            self.client.get_async_connection().await?.into_pubsub();
        for pattern in patterns {
            pubsub.psubscribe(*pattern).await?;
        }
        Ok(pubsub)
    }

    #[allow(dead_code)]
    pub async fn zadd(&mut self, key: &str, member: String, score: f64) -> RedisResult<()> {
        self.connection_manager.zadd(key, member, score).await
    }

    pub async fn zrevrange_withscores(
        &mut self,
        key: &str,
        start: isize,
        stop: isize,
    ) -> RedisResult<Vec<(String, f64)>> {
        self.connection_manager
            .zrevrange_withscores(key, start, stop)
            .await
    }

    pub async fn zrevrank(&mut self, key: &str, member: String) -> RedisResult<Option<u64>> {
        self.connection_manager.zrevrank(key, member).await
    }

    pub async fn mget(&mut self, keys: &[String]) -> RedisResult<Vec<Option<String>>> {
        redis::cmd("MGET")
            .arg(keys)
            .query_async(&mut self.connection_manager)
            .await
    }
}
