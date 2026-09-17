use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::game::special_cards::SpecialCards;

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
    PlayerJoined {
        game_id: Uuid,
        player_id: Uuid,
        user_id: Uuid,
        pseudo: String,
        position: i32,
        player_count: i32,
        max_players: i32,
    },
    GameCancelled {
        game_id: Uuid,
        reason: String,
    },
    GameReady {
        game_id: Uuid,
        correlation_id: Option<Uuid>,
    },
    CardsDealt {
        game_id: Uuid,
        player_id: Uuid,
        cards: Vec<i32>,
        special_cards: SpecialCards,
    },
    GameStarted {
        game_id: Uuid,
        players: Vec<GameStartedPlayer>,
        current_turn: Uuid,
        game_mode: String,
        correlation_id: Option<Uuid>,
    },
    PlayerDisconnected {
        game_id: Uuid,
        player_id: Uuid,
        player_position: i32,
        disconnected_at: Option<String>,
    },
    PlayerReconnected {
        game_id: Uuid,
        player_id: Uuid,
        player_position: i32,
        reconnected_at: Option<String>,
    },
    StalenessWarning {
        game_id: Uuid,
        player_id: Uuid,
        player_name: String,
        kicked_after_seconds: i64,
    },
    PlayerKicked {
        game_id: Uuid,
        player_id: Uuid,
        player_name: String,
    },
    GameReshuffled {
        game_id: Uuid,
        remaining_players: u32,
    },
    PlayerForfeitWin {
        game_id: Uuid,
        winner_id: Uuid,
        winner_name: String,
    },
    ClaimPending {
        game_id: Uuid,
    },
    ClaimOffered {
        game_id: Uuid,
        player_id: Uuid,
        special_cards: SpecialCards,
    },
    ClaimResolved {
        game_id: Uuid,
    },
    SpecialClaim {
        game_id: Uuid,
        player_id: Uuid,
        cards: Vec<i32>,
        winner_position: i32,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RoomEvent {
    MemberJoined {
        room_id: Uuid,
        user_id: Uuid,
        pseudo: String,
    },
    MemberLeft {
        room_id: Uuid,
        user_id: Uuid,
        pseudo: String,
    },
    RunCreated {
        room_id: Uuid,
        run_id: Uuid,
        num_games: i32,
        bet_per_game: i32,
    },
    GameStarted {
        room_id: Uuid,
        run_id: Uuid,
        game_id: Uuid,
        game_index: i32,
        total_games: i32,
    },
    RunCompleted {
        room_id: Uuid,
        run_id: Uuid,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameStartedPlayer {
    pub id: Uuid,
    pub name: String,
    pub position: i32,
    pub display_position: i32,
    pub cards_count: i32,
    pub player_type: String, // "human" or "bot"
}

/// Events scoped to a single user (rather than a game or room). These are
/// pushed to the user's own WebSocket connection so the client can react in
/// real time to things like incoming game invitations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UserEvent {
    /// A new game invitation was created for the target user.
    InviteReceived {
        user_id: Uuid,
        invite_id: Uuid,
        game_id: Uuid,
        creator_pseudo: String,
        bet: i32,
        player_count: i64,
        max_players: i32,
        created_at: String,
        expires_at: Option<String>,
    },
    /// An invitation is no longer valid (accepted, declined, or game cancelled).
    InviteRemoved { user_id: Uuid, game_id: Uuid },
}

impl UserEvent {
    pub fn channel(&self) -> String {
        match self {
            UserEvent::InviteReceived { user_id, .. }
            | UserEvent::InviteRemoved { user_id, .. } => format!("user:{}", user_id),
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("Failed to serialize UserEvent")
    }

    #[allow(dead_code)]
    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }
}

impl RoomEvent {
    pub fn channel(&self) -> String {
        match self {
            RoomEvent::MemberJoined { room_id, .. }
            | RoomEvent::MemberLeft { room_id, .. }
            | RoomEvent::RunCreated { room_id, .. }
            | RoomEvent::GameStarted { room_id, .. }
            | RoomEvent::RunCompleted { room_id, .. } => format!("room:{}", room_id),
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("Failed to serialize RoomEvent")
    }
}

impl GameEvent {
    /// Returns the Redis channel name where this event should be published.
    pub fn channel(&self) -> String {
        match self {
            GameEvent::CardPlayed { game_id, .. }
            | GameEvent::RoundCompleted { game_id, .. }
            | GameEvent::GameFinished { game_id, .. }
            | GameEvent::TurnChanged { game_id, .. }
            | GameEvent::PlayerJoined { game_id, .. }
            | GameEvent::GameCancelled { game_id, .. }
            | GameEvent::GameReady { game_id, .. }
            | GameEvent::CardsDealt { game_id, .. }
            | GameEvent::GameStarted { game_id, .. }
            | GameEvent::PlayerDisconnected { game_id, .. }
            | GameEvent::PlayerReconnected { game_id, .. }
            | GameEvent::StalenessWarning { game_id, .. }
            | GameEvent::PlayerKicked { game_id, .. }
            | GameEvent::GameReshuffled { game_id, .. }
            | GameEvent::PlayerForfeitWin { game_id, .. } => format!("game:{}", game_id),
            GameEvent::ClaimPending { game_id }
            | GameEvent::ClaimResolved { game_id }
            | GameEvent::SpecialClaim { game_id, .. }
            | GameEvent::ClaimOffered { game_id, .. } => format!("game:{}", game_id),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_event_invite_received_channel_and_serialization() {
        let user_id = Uuid::new_v4();
        let event = UserEvent::InviteReceived {
            user_id,
            invite_id: Uuid::new_v4(),
            game_id: Uuid::new_v4(),
            creator_pseudo: "alice".to_string(),
            bet: 20,
            player_count: 1,
            max_players: 4,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            expires_at: None,
        };

        assert_eq!(event.channel(), format!("user:{}", user_id));

        let json = event.to_json();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["type"], "invite_received");
        assert_eq!(parsed["creator_pseudo"], "alice");
        assert_eq!(parsed["bet"], 20);
    }

    #[test]
    fn user_event_invite_removed_channel_and_serialization() {
        let user_id = Uuid::new_v4();
        let game_id = Uuid::new_v4();
        let event = UserEvent::InviteRemoved { user_id, game_id };

        assert_eq!(event.channel(), format!("user:{}", user_id));

        let json = event.to_json();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["type"], "invite_removed");
        assert_eq!(parsed["game_id"], game_id.to_string());
    }

    #[test]
    fn user_event_roundtrips_through_json() {
        let event = UserEvent::InviteRemoved {
            user_id: Uuid::new_v4(),
            game_id: Uuid::new_v4(),
        };
        let roundtripped = UserEvent::from_json(&event.to_json()).unwrap();
        assert!(matches!(roundtripped, UserEvent::InviteRemoved { .. }));
    }
}
