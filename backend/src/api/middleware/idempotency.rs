#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const IDEMPOTENCY_KEY_PREFIX: &str = "idem";
pub const IDEMPOTENCY_TTL_SECS: u64 = 300;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedPlayOutcome {
    pub success: bool,
    pub card_id: Uuid,
    pub next_turn: Option<Uuid>,
    pub game_ended: bool,
    pub round_completed: bool,
    pub current_round: i32,
}

pub fn build_redis_key(player_id: Uuid, idempotency_key: &str) -> String {
    format!(
        "{}:{}:{}",
        IDEMPOTENCY_KEY_PREFIX, player_id, idempotency_key
    )
}
