use serde::{de::DeserializeOwned, Serialize};
use tracing;

use crate::error::GameError;
use crate::messaging::RedisClient;

#[allow(dead_code)]
pub(crate) struct IdempotencyGuard {
    redis: RedisClient,
    key: String,
    acquired: bool,
}

#[allow(dead_code)]
impl IdempotencyGuard {
    pub(crate) fn new(redis: RedisClient, key: String) -> Self {
        Self {
            redis,
            key,
            acquired: false,
        }
    }

    pub(crate) async fn acquire<T: DeserializeOwned>(&mut self) -> Result<Option<T>, GameError> {
        match self.redis.set_nx_ex(&self.key, "pending", 300).await {
            Ok(true) => {
                self.acquired = true;
                Ok(None)
            }
            Ok(false) => match self.redis.get(&self.key).await {
                Ok(Some(val)) if val != "pending" => match serde_json::from_str::<T>(&val) {
                    Ok(outcome) => Ok(Some(outcome)),
                    Err(_) => Err(GameError::IdempotencyConflict),
                },
                _ => Err(GameError::IdempotencyConflict),
            },
            Err(e) => {
                tracing::warn!(
                    "Redis error on idempotency check: {}, proceeding without",
                    e
                );
                Ok(None)
            }
        }
    }

    pub(crate) async fn complete<T: Serialize>(&mut self, outcome: &T) {
        if self.acquired {
            if let Ok(outcome_json) = serde_json::to_string(outcome) {
                let _ = self.redis.set_ex(&self.key, &outcome_json, 300).await;
            }
        }
    }

    pub(crate) async fn release(&mut self) {
        if self.acquired {
            let _ = self.redis.del(&self.key).await;
        }
    }
}
