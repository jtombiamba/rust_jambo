use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Player information for AI task
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PlayerInfo {
    pub player_id: Uuid,
    pub position: i32,
    pub player_type: String, // "human" or "bot"
    pub credits: i32,
    pub name: String,
}

/// Task message sent to RabbitMQ when a bot's turn arrives.
#[derive(Debug, Serialize, Deserialize)]
pub struct AITask {
    pub game_id: Uuid,
    pub player_id: Uuid,

    /// Correlation ID propagated from the originating HTTP request for end-to-end tracing.
    pub correlation_id: Option<Uuid>,

    // Extended game state to reduce database queries
    pub current_round: i32,
    pub current_roll: i32, // Current round number (1-5)
    pub game_status: String,
    pub current_player_turn: Option<Uuid>,

    // Cards information
    pub played_cards_this_round: Vec<i32>, // Card indices already played in current round
    pub bot_hand_cards: Vec<i32>,          // Bot's unplayed card indices

    // Players information
    pub players: Vec<PlayerInfo>, // All players in the game

    // Current round state
    pub current_winning_card: Option<i32>, // Currently winning card index in the round
    pub winning_player_position: Option<i32>, // Position of player currently winning the round

    // Game configuration
    pub bet: i32,
    pub auto_mode: bool, // Whether the game is in auto mode
}

impl AITask {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        game_id: Uuid,
        player_id: Uuid,
        correlation_id: Option<Uuid>,
        current_round: i32,
        current_roll: i32,
        game_status: String,
        current_player_turn: Option<Uuid>,
        played_cards_this_round: Vec<i32>,
        bot_hand_cards: Vec<i32>,
        players: Vec<PlayerInfo>,
        current_winning_card: Option<i32>,
        winning_player_position: Option<i32>,
        bet: i32,
        auto_mode: bool,
    ) -> Self {
        Self {
            game_id,
            player_id,
            correlation_id,
            current_round,
            current_roll,
            game_status,
            current_player_turn,
            played_cards_this_round,
            bot_hand_cards,
            players,
            current_winning_card,
            winning_player_position,
            bet,
            auto_mode,
        }
    }

    /// Create a minimal AITask (backward compatibility)
    pub fn minimal(game_id: Uuid, player_id: Uuid) -> Self {
        Self {
            game_id,
            player_id,
            correlation_id: None,
            current_round: 0,
            current_roll: 1,
            game_status: "active".to_string(),
            current_player_turn: None,
            played_cards_this_round: Vec::new(),
            bot_hand_cards: Vec::new(),
            players: Vec::new(),
            current_winning_card: None,
            winning_player_position: None,
            bet: 10,
            auto_mode: false,
        }
    }

    /// Serialize the task to JSON bytes.
    pub fn to_json_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("Failed to serialize AITask")
    }

    /// Deserialize from JSON bytes.
    #[allow(dead_code)]
    pub fn from_json_bytes(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }
}
