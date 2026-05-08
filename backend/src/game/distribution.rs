use rand::seq::SliceRandom;
use rand::thread_rng;
use uuid::Uuid;

use crate::game::constants::{CARDS_PER_PLAYER, TOTAL_CARDS};

/// Distributes cards randomly among the given players.
///
/// # Arguments
/// * `player_ids` - Slice of player UUIDs. The number of players must be exactly `MAX_PLAYERS_IN_GAME` (4).
///
/// # Returns
/// A vector of `(player_id, card_index)` pairs where each player receives `CARDS_PER_PLAYER` (5) cards,
/// and each card index is a unique integer in `0..TOTAL_CARDS` (32).
///
/// # Panics
/// Panics if `player_ids.len() != MAX_PLAYERS_IN_GAME`.
pub fn distribute_cards(player_ids: &[Uuid]) -> Vec<(Uuid, i32)> {
    assert_eq!(
        player_ids.len(),
        crate::game::constants::MAX_PLAYERS_IN_GAME,
        "Exactly {} players required",
        crate::game::constants::MAX_PLAYERS_IN_GAME
    );

    // Generate a random permutation of all card indices (0..TOTAL_CARDS)
    let mut rng = thread_rng();
    let mut cards: Vec<i32> = (0..TOTAL_CARDS as i32).collect();
    cards.shuffle(&mut rng);

    // Chunk the cards into equal slices per player
    let chunk_size = CARDS_PER_PLAYER;
    let mut result = Vec::with_capacity(TOTAL_CARDS);

    for (i, player_id) in player_ids.iter().enumerate() {
        let start = i * chunk_size;
        let end = start + chunk_size;
        for &card in &cards[start..end] {
            result.push((*player_id, card));
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::constants::MAX_PLAYERS_IN_GAME;
    use uuid::Uuid;

    #[test]
    fn test_distribute_cards() {
        let player_ids = [
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
        ];

        let distribution = distribute_cards(&player_ids);

        // Total cards equals MAX_PLAYERS_IN_GAME * CARDS_PER_PLAYER
        assert_eq!(distribution.len(), MAX_PLAYERS_IN_GAME * CARDS_PER_PLAYER);

        // Each player gets exactly CARDS_PER_PLAYER cards
        for &player_id in &player_ids {
            let count = distribution
                .iter()
                .filter(|(id, _)| *id == player_id)
                .count();
            assert_eq!(count, CARDS_PER_PLAYER);
        }

        // All card indices are unique and within range
        let mut card_indices: Vec<i32> = distribution.iter().map(|&(_, card)| card).collect();
        card_indices.sort_unstable();
        let expected_count = (MAX_PLAYERS_IN_GAME * CARDS_PER_PLAYER) as i32;
        assert_eq!(card_indices.len(), expected_count as usize);
        for &card in &card_indices {
            assert!(card >= 0 && card < TOTAL_CARDS as i32);
        }
    }

    #[test]
    #[should_panic(expected = "Exactly 4 players required")]
    fn test_wrong_player_count() {
        let player_ids = [Uuid::new_v4()]; // only one player
        distribute_cards(&player_ids);
    }
}
