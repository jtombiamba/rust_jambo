use crate::messaging::events::GameEvent;
use redis::{aio::ConnectionManager, AsyncCommands, Client, RedisResult};
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

    /// Publish a message to a Redis channel.
    pub async fn publish(&mut self, channel: &str, message: &str) -> RedisResult<()> {
        self.connection_manager.publish(channel, message).await
    }

    /// Publish a game event to its appropriate Redis channel.
    pub async fn publish_game_event(&mut self, event: &GameEvent) -> RedisResult<()> {
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

    /// Subscribe to Redis patterns and return a subscription object.
    pub async fn psubscribe(&mut self, patterns: &[&str]) -> RedisResult<redis::aio::PubSub> {
        let mut pubsub: redis::aio::PubSub =
            self.client.get_async_connection().await?.into_pubsub();
        for pattern in patterns {
            pubsub.psubscribe(*pattern).await?;
        }
        Ok(pubsub)
    }
}
