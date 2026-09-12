use rand::seq::SliceRandom;
use sea_orm::{ActiveValue, Set};
use uuid::Uuid;

use crate::database::models::{game_card, Player};
use crate::game::constants::{CARDS_PER_PLAYER, TOTAL_CARDS};

/// Builds the `game_card` rows for a freshly started game.
///
/// Shuffles a full deck (`0..TOTAL_CARDS`) and deals `CARDS_PER_PLAYER`
/// consecutive cards to each player, in the order the players are provided.
/// The caller is expected to pass players ordered by their table position so
/// that card slices map deterministically to positions.
///
/// Returns the built `ActiveModel`s for bulk insertion plus the shuffled deck,
/// so callers that need to publish per-player `CardsDealt` events can re-derive
/// each player's hand from the returned deck.
pub fn build_game_cards(
    game_id: Uuid,
    players: &[Player],
) -> (Vec<game_card::ActiveModel>, Vec<i32>) {
    let mut cards: Vec<i32> = (0..TOTAL_CARDS as i32).collect();
    cards.shuffle(&mut rand::rng());

    let now = chrono::Utc::now();

    let models = players
        .iter()
        .enumerate()
        .flat_map(|(i, p)| {
            let start = i * CARDS_PER_PLAYER;
            let end = start + CARDS_PER_PLAYER;
            cards[start..end]
                .iter()
                .map(move |&card_index| game_card::ActiveModel {
                    id: Set(Uuid::now_v7()),
                    game_id: Set(game_id),
                    player_id: Set(Some(p.id)),
                    card_index: Set(card_index),
                    played: Set(false),
                    played_at: ActiveValue::NotSet,
                    round: ActiveValue::NotSet,
                    created_at: Set(now),
                })
        })
        .collect();

    (models, cards)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::models::{Player, PlayerType};

    fn make_player(id: Uuid) -> Player {
        Player {
            id,
            game_id: Uuid::nil(),
            player_type: PlayerType::Human,
            name: "p".to_string(),
            position: 0,
            credits: 0,
            created_at: chrono::Utc::now(),
            user_id: None,
            kicked: false,
            kicked_at: None,
        }
    }

    #[test]
    fn deals_all_cards_across_players() {
        let game_id = Uuid::now_v7();
        let players: Vec<Player> = (0..4).map(|_| make_player(Uuid::now_v7())).collect();

        let (models, deck) = build_game_cards(game_id, &players);

        assert_eq!(deck.len(), TOTAL_CARDS);
        assert_eq!(models.len(), players.len() * CARDS_PER_PLAYER);

        // The dealt cards are exactly the first N cards of the shuffled deck,
        // each dealt exactly once.
        let mut seen = models
            .iter()
            .map(|m| m.card_index.clone().unwrap())
            .collect::<Vec<i32>>();
        seen.sort_unstable();

        let mut expected = deck[..models.len()].to_vec();
        expected.sort_unstable();
        assert_eq!(seen, expected);
    }

    #[test]
    fn assigns_cards_per_player_in_position_order() {
        let game_id = Uuid::now_v7();
        let players: Vec<Player> = (0..4).map(|_| make_player(Uuid::now_v7())).collect();
        let player_ids: Vec<Uuid> = players.iter().map(|p| p.id).collect();

        let (models, deck) = build_game_cards(game_id, &players);

        for (i, pid) in player_ids.iter().enumerate() {
            let start = i * CARDS_PER_PLAYER;
            let end = start + CARDS_PER_PLAYER;
            let expected: Vec<i32> = deck[start..end].to_vec();

            let mut actual: Vec<i32> = models
                .iter()
                .filter(|m| m.player_id.clone().unwrap() == Some(*pid))
                .map(|m| m.card_index.clone().unwrap())
                .collect();
            actual.sort_unstable();
            let mut expected_sorted = expected.clone();
            expected_sorted.sort_unstable();

            assert_eq!(actual, expected_sorted);
        }
    }
}
