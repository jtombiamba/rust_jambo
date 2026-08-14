use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Incoming WebSocket messages from the frontend.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IncomingMessage {
    /// Join a specific game with optional player identification.
    JoinGame {
        game_id: Uuid,
        #[serde(default)]
        player_id: Option<Uuid>,
        #[serde(default)]
        player_position: Option<i32>,
    },
    /// Leave the current game.
    LeaveGame,
    /// Heartbeat to keep connection alive.
    Ping,
}

/// Outgoing WebSocket messages to the frontend.
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OutgoingMessage {
    /// Acknowledgment of a join.
    GameJoined { game_id: Uuid },
    /// Full game state snapshot sent privately to a player upon joining or reconnection.
    GameStateSnapshot {
        game_id: Uuid,
        roll: i32,
        rank: Option<i32>,
        status: String,
        current_winning_card: Option<i32>,
        current_winning_player_position: Option<i32>,
        players: Vec<GameStatePlayer>,
        played_cards: Vec<Option<i32>>, //Vec<GameStateCard>,
        step_by_step: bool,
    },
    /// Error response.
    Error { message: String, source: String },
    /// Pong response to a ping.
    Pong,
}

#[derive(Debug, Clone, Serialize)]
pub struct GameStatePlayer {
    pub id: Uuid,
    pub name: String,
    pub position: i32,
    pub display_position: i32,
    pub player_type: String,
    pub cards_count: i32,
}

// #[warn(dead_code)]
// #[derive(Debug, Clone, Serialize)]
// pub struct GameStateCard {
//     pub player_id: i32,
//     pub card_index: i32,
// }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_incoming_ping_deserialization() {
        let msg: IncomingMessage = serde_json::from_str(r#"{"type":"ping"}"#).unwrap();
        assert!(matches!(msg, IncomingMessage::Ping));
    }

    #[test]
    fn test_incoming_join_game_deserialization() {
        let game_id = Uuid::new_v4();
        let json = format!(r#"{{"type":"join_game","game_id":"{}"}}"#, game_id);
        let msg: IncomingMessage = serde_json::from_str(&json).unwrap();
        match msg {
            IncomingMessage::JoinGame {
                game_id: gid,
                player_id: _,
                player_position: _,
            } => assert_eq!(gid, game_id),
            _ => panic!("Expected JoinGame"),
        }
    }

    #[test]
    fn test_incoming_leave_game_deserialization() {
        let msg: IncomingMessage = serde_json::from_str(r#"{"type":"leave_game"}"#).unwrap();
        assert!(matches!(msg, IncomingMessage::LeaveGame));
    }

    #[test]
    fn test_incoming_unknown_type() {
        let result: Result<IncomingMessage, _> = serde_json::from_str(r#"{"type":"unknown"}"#);
        assert!(result.is_err());
    }

    #[test]
    fn test_outgoing_pong_serialization() {
        let msg = OutgoingMessage::Pong;
        let json = serde_json::to_string(&msg).unwrap();
        assert_eq!(json, r#"{"type":"pong"}"#);
    }

    #[test]
    fn test_outgoing_game_joined_serialization() {
        let game_id = Uuid::new_v4();
        let msg = OutgoingMessage::GameJoined { game_id };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["type"], "game_joined");
        assert_eq!(parsed["game_id"], game_id.to_string());
    }

    #[test]
    fn test_outgoing_error_serialization() {
        let msg = OutgoingMessage::Error {
            message: "something went wrong".into(),
            source: "ws:test".into(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["type"], "error");
        assert_eq!(parsed["message"], "something went wrong");
        assert_eq!(parsed["source"], "ws:test");
    }
}
