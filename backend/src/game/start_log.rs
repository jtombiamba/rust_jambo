use serde_json::{json, Value};
use uuid::Uuid;

use crate::database::models::{Player, PlayerType};
use crate::game::special_cards::SpecialCards;

fn player_type_str(player_type: PlayerType) -> &'static str {
    match player_type {
        PlayerType::Human => "human",
        PlayerType::Bot => "bot",
    }
}

/// Build the per-player details array for the game-start log.
pub fn player_details_json(players: &[Player]) -> Vec<Value> {
    players
        .iter()
        .map(|p| {
            json!({
                "player_id": p.id,
                "user_id": p.user_id,
                "name": p.name,
                "position": p.position,
                "player_type": player_type_str(p.player_type),
                "credits": p.credits,
            })
        })
        .collect()
}

/// Build the per-player special-card flags array for the game-start log.
pub fn special_cards_json(hands: &[(Uuid, SpecialCards)]) -> Vec<Value> {
    hands
        .iter()
        .map(|(player_id, s)| {
            json!({
                "player_id": player_id,
                "check_triple_seven": s.check_triple_seven,
                "check_sum_value_under_21": s.check_sum_value_under_21,
                "check_a_square": s.check_a_square,
                "has_any": s.has_any(),
                "best_rank": s.best_rank(),
            })
        })
        .collect()
}

/// Build the single JSON payload logged when a game starts: the players that
/// take part and the special cards computed for each of their hands.
pub fn build_start_details(players: &[Player], hands: &[(Uuid, SpecialCards)]) -> Value {
    json!({
        "players": player_details_json(players),
        "special_cards": special_cards_json(hands),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_player(id: Uuid, name: &str, position: i32, player_type: PlayerType) -> Player {
        Player {
            id,
            game_id: Uuid::nil(),
            player_type,
            name: name.to_string(),
            position,
            credits: 100,
            created_at: chrono::Utc::now(),
            user_id: Some(Uuid::now_v7()),
            kicked: false,
            kicked_at: None,
        }
    }

    #[test]
    fn player_details_includes_identity_and_position() {
        let id = Uuid::now_v7();
        let players = vec![make_player(id, "alice", 2, PlayerType::Human)];

        let details = player_details_json(&players);
        assert_eq!(details.len(), 1);
        assert_eq!(details[0]["player_id"], id.to_string());
        assert_eq!(details[0]["name"], "alice");
        assert_eq!(details[0]["position"], 2);
        assert_eq!(details[0]["player_type"], "human");
        assert_eq!(details[0]["credits"], 100);
    }

    #[test]
    fn special_cards_flags_are_serialized() {
        let id = Uuid::now_v7();
        let hands = vec![(
            id,
            SpecialCards {
                check_triple_seven: true,
                check_sum_value_under_21: false,
                check_a_square: false,
            },
        )];

        let details = special_cards_json(&hands);
        assert_eq!(details.len(), 1);
        assert_eq!(details[0]["player_id"], id.to_string());
        assert_eq!(details[0]["check_triple_seven"], true);
        assert_eq!(details[0]["check_sum_value_under_21"], false);
        assert_eq!(details[0]["check_a_square"], false);
        assert_eq!(details[0]["has_any"], true);
        assert_eq!(details[0]["best_rank"], 0);
    }

    #[test]
    fn build_start_details_combines_players_and_specials() {
        let id = Uuid::now_v7();
        let players = vec![make_player(id, "bob", 0, PlayerType::Bot)];
        let hands = vec![(
            id,
            SpecialCards {
                check_triple_seven: false,
                check_sum_value_under_21: true,
                check_a_square: false,
            },
        )];

        let details = build_start_details(&players, &hands);
        assert_eq!(details["players"].as_array().unwrap().len(), 1);
        assert_eq!(details["special_cards"].as_array().unwrap().len(), 1);
    }
}
