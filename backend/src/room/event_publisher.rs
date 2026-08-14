use crate::messaging::events::RoomEvent;
use crate::messaging::redis::PublishResult;
use crate::messaging::RedisClient;

#[async_trait::async_trait]
pub trait RoomEventPublisher: Send + Sync {
    async fn publish(&self, event: &RoomEvent);
}

pub struct RedisRoomEventPublisher {
    redis_client: Option<RedisClient>,
}

impl RedisRoomEventPublisher {
    pub fn new(redis_client: Option<RedisClient>) -> Self {
        Self { redis_client }
    }
}

#[async_trait::async_trait]
impl RoomEventPublisher for RedisRoomEventPublisher {
    async fn publish(&self, event: &RoomEvent) {
        if let Some(mut redis) = self.redis_client.clone() {
            match redis.publish_room_event_with_retry(event).await {
                PublishResult::Published => {}
                PublishResult::RetryExhausted(e) => {
                    tracing::error!("Failed to publish room event after retries: {}", e);
                }
            }
        }
    }
}
