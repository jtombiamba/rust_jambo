use uuid::Uuid;

use crate::messaging::RedisClient;
use crate::room::error::RoomServiceError;

pub const START_GAME_LOCK_TTL_SECS: u64 = 30;

#[async_trait::async_trait]
pub trait StartGameLock: Send + Sync {
    async fn acquire(&self, run_id: Uuid) -> Result<StartGameLockGuard, RoomServiceError>;
}

pub struct StartGameLockGuard {
    pub(crate) redis: Option<RedisClient>,
    pub(crate) key: String,
    pub(crate) token: String,
    pub(crate) released: bool,
}

impl StartGameLockGuard {
    pub async fn release(&mut self) {
        if self.released {
            return;
        }
        self.released = true;
        if let Some(ref mut redis) = self.redis {
            match redis.compare_and_delete(&self.key, &self.token).await {
                Ok(_) => {}
                Err(e) => tracing::error!("Failed to release start-game lock {}: {}", self.key, e),
            }
        }
    }
}

impl Drop for StartGameLockGuard {
    fn drop(&mut self) {
        if !self.released {
            tracing::warn!(
                "StartGameLockGuard dropped without release; relying on TTL for {}",
                self.key
            );
        }
    }
}

pub struct RedisStartGameLock {
    redis_client: Option<RedisClient>,
}

impl RedisStartGameLock {
    pub fn new(redis_client: Option<RedisClient>) -> Self {
        Self { redis_client }
    }
}

#[async_trait::async_trait]
impl StartGameLock for RedisStartGameLock {
    async fn acquire(&self, run_id: Uuid) -> Result<StartGameLockGuard, RoomServiceError> {
        let key = format!("start_game_lock:{}", run_id);
        let token = Uuid::now_v7().to_string();

        let acquired = if let Some(ref mut redis) = self.redis_client.clone() {
            redis
                .set_nx_ex(&key, &token, START_GAME_LOCK_TTL_SECS)
                .await
                .map_err(|e| RoomServiceError::Internal(format!("Redis lock error: {}", e)))?
        } else {
            true
        };

        if !acquired {
            return Err(RoomServiceError::StartAlreadyInProgress);
        }

        Ok(StartGameLockGuard {
            redis: self.redis_client.clone(),
            key,
            token,
            released: false,
        })
    }
}
