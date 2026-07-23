use serde::{Deserialize, Serialize};
use tracing::error;
use uuid::Uuid;

use crate::messaging::RedisClient;
use crate::observability::metrics::{record_cache_hit, record_cache_miss};

pub mod leaderboard;

const CACHE_TTL_SECS: u64 = 15 * 60;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CachedUser {
    pub pseudo: String,
    pub email: String,
}

pub struct UserCache {
    redis_client: Option<RedisClient>,
}

#[allow(dead_code)]
impl UserCache {
    pub fn new() -> Self {
        Self { redis_client: None }
    }

    pub fn new_with_redis(redis_client: RedisClient) -> Self {
        Self {
            redis_client: Some(redis_client),
        }
    }

    pub async fn get_by_pseudo(&self, pseudo: &str) -> Option<CachedUser> {
        let mut redis = self.redis_client.clone()?;
        let uuid_str = redis
            .get(&format!("user:pseudo:{pseudo}"))
            .await
            .unwrap_or_else(|e| {
                error!("Redis get_by_pseudo error: {}", e);
                None
            });
        match uuid_str {
            Some(val) => {
                let uuid = Uuid::parse_str(&val).ok()?;
                self.get_by_uuid(&uuid).await
            }
            None => {
                record_cache_miss();
                None
            }
        }
    }

    pub async fn get_uuid_by_pseudo(&self, pseudo: &str) -> Option<Uuid> {
        let mut redis = self.redis_client.clone()?;
        let uuid_str = redis
            .get(&format!("user:pseudo:{pseudo}"))
            .await
            .unwrap_or_else(|e| {
                error!("Redis get_uuid_by_pseudo error: {}", e);
                None
            });
        match uuid_str {
            Some(val) => {
                let uuid = Uuid::parse_str(&val).ok();
                if uuid.is_some() {
                    record_cache_hit();
                } else {
                    record_cache_miss();
                }
                uuid
            }
            None => {
                record_cache_miss();
                None
            }
        }
    }

    pub async fn get_by_uuid(&self, uuid: &Uuid) -> Option<CachedUser> {
        let mut redis = self.redis_client.clone()?;
        let data = redis
            .get(&format!("user:uuid:{uuid}"))
            .await
            .unwrap_or_else(|e| {
                error!("Redis get_by_uuid error: {}", e);
                None
            });
        match data {
            Some(raw) => {
                let user: Option<CachedUser> = serde_json::from_str(&raw).ok();
                if user.is_some() {
                    record_cache_hit();
                } else {
                    record_cache_miss();
                }
                user
            }
            None => {
                record_cache_miss();
                None
            }
        }
    }

    pub async fn get_by_uuids(&self, uuids: &[Uuid]) -> Vec<Option<CachedUser>> {
        let mut redis = match self.redis_client.clone() {
            Some(r) => r,
            None => return vec![None; uuids.len()],
        };
        let keys: Vec<String> = uuids.iter().map(|id| format!("user:uuid:{id}")).collect();
        let values = match redis.mget(&keys).await {
            Ok(v) => v,
            Err(e) => {
                error!("Redis mget error: {}", e);
                return vec![None; uuids.len()];
            }
        };
        let results: Vec<Option<CachedUser>> = values
            .into_iter()
            .map(|opt| opt.and_then(|data| serde_json::from_str(&data).ok()))
            .collect();
        for r in &results {
            if r.is_some() {
                record_cache_hit();
            } else {
                record_cache_miss();
            }
        }
        results
    }

    pub async fn put(&self, uuid: Uuid, pseudo: String, email: String) {
        let mut redis = match self.redis_client.clone() {
            Some(r) => r,
            None => return,
        };
        let user_data = match serde_json::to_string(&CachedUser {
            pseudo: pseudo.clone(),
            email,
        }) {
            Ok(d) => d,
            Err(_) => return,
        };
        let _ = redis
            .set_ex(&format!("user:uuid:{uuid}"), &user_data, CACHE_TTL_SECS)
            .await;
        let _ = redis
            .set_ex(
                &format!("user:pseudo:{pseudo}"),
                &uuid.to_string(),
                CACHE_TTL_SECS,
            )
            .await;
    }

    pub async fn populate_bulk(&self, users: &[(Uuid, String, String)]) {
        for (uuid, pseudo, email) in users {
            self.put(*uuid, pseudo.clone(), email.clone()).await;
        }
    }

    pub async fn invalidate(&self, uuid: &Uuid) {
        let mut redis = match self.redis_client.clone() {
            Some(r) => r,
            None => return,
        };
        let pseudo: Option<String> = self.get_by_uuid(uuid).await.map(|u| u.pseudo);
        let _ = redis.del(&format!("user:uuid:{uuid}")).await;
        if let Some(p) = pseudo {
            let _ = redis.del(&format!("user:pseudo:{p}")).await;
        }
    }
}

impl Default for UserCache {
    fn default() -> Self {
        Self::new()
    }
}
