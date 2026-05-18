mod ai_task;
mod caching;
mod creation;
mod evaluation;
mod events;
mod gameplay;
mod invites;
mod lifecycle;
mod recovery;
#[cfg(test)]
mod tests;
pub mod types;

use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use crate::messaging::RedisClient;
pub use types::{CardPlayResult, GameServiceError, MultiplayerGameOutcome};

pub const fn compute_display_position(
    actual_pos: usize,
    num_players: usize,
    my_pos: usize,
) -> usize {
    (num_players + actual_pos - my_pos) % num_players
}

/// Check if a database error is a PostgreSQL unique constraint violation (code 23505).
fn is_unique_violation(e: &sea_orm::DbErr) -> bool {
    if let sea_orm::DbErr::Exec(exec_err) = e {
        exec_err.to_string().contains("23505")
    } else {
        false
    }
}

pub struct GameService {
    db: sea_orm::DatabaseConnection,
    redis_client: Option<RedisClient>,
    accept_invite_locks: tokio::sync::Mutex<HashMap<Uuid, Arc<tokio::sync::Mutex<()>>>>,
}

impl GameService {
    #[allow(dead_code)]
    pub fn new(db: sea_orm::DatabaseConnection) -> Self {
        Self {
            db,
            redis_client: None,
            accept_invite_locks: tokio::sync::Mutex::new(HashMap::new()),
        }
    }

    pub fn new_with_redis(
        db: sea_orm::DatabaseConnection,
        redis_client: Option<RedisClient>,
    ) -> Self {
        Self {
            db,
            redis_client,
            accept_invite_locks: tokio::sync::Mutex::new(HashMap::new()),
        }
    }

    #[allow(dead_code)]
    pub fn redis_client(&self) -> Option<RedisClient> {
        self.redis_client.clone()
    }
}
