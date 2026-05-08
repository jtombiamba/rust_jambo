use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Events that can be published to Redis and forwarded to WebSocket clients.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GameEvent {
    CardPlayed {
        game_id: Uuid,
        player_id: Uuid,
        card_index: i32,
        next_turn: Option<Uuid>,
        correlation_id: Option<Uuid>,
    },
    RoundCompleted {
        game_id: Uuid,
        round_number: i32,
        winner_id: Uuid,
        winner_position: i32,
        win_type: Option<String>, // "normal", "kora", "doubleKora"
        deck_slots: Vec<Option<i32>>,
        correlation_id: Option<Uuid>,
    },
    GameFinished {
        game_id: Uuid,
        winner_id: Option<Uuid>,
        winner_name: Option<String>,
        winner_position: Option<i32>,
        status: String, // "finished", "kora", "doubleKora"
        final_score: Option<i32>,
        rounds_played: i32,
        correlation_id: Option<Uuid>,
    },
    TurnChanged {
        game_id: Uuid,
        current_turn: Uuid,
        correlation_id: Option<Uuid>,
    },
}

impl GameEvent {
    /// Returns the Redis channel name where this event should be published.
    pub fn channel(&self) -> String {
        match self {
            GameEvent::CardPlayed { game_id, .. }
            | GameEvent::RoundCompleted { game_id, .. }
            | GameEvent::GameFinished { game_id, .. }
            | GameEvent::TurnChanged { game_id, .. } => format!("game:{}", game_id),
        }
    }

    /// Serialize the event to JSON string.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("Failed to serialize GameEvent")
    }

    /// Deserialize from JSON string.
    #[allow(dead_code)]
    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }
}
