use rand::seq::SliceRandom;
use rand::thread_rng;

/// Strategy choices mirroring the Python `StrategyChoice` enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrategyChoice {
    LongUp = 1,
    LongDown = 2,
    MidUp = 3,
    MidDown = 4,
    ShortUp = 5,
    ShortDown = 6,
}

impl StrategyChoice {
    /// Randomly select a strategy from the allowed set (for high-level strategy).
    pub fn random_high() -> Self {
        use StrategyChoice::*;
        let choices = [LongUp, LongDown, MidUp, MidDown];
        *choices.choose(&mut thread_rng()).unwrap()
    }
}

/// Determine if a card can match the current winning card (same suit).
/// Returns true if the card is of the same colour (suit) as the winning card,
/// or if there is no winning card.
pub fn can_match_card(card: i32, current_winning_card: Option<i32>) -> bool {
    match current_winning_card {
        None => true,
        Some(winning) => winning / 8 == card / 8,
    }
}

/// Pick all unplayed cards that can match the current winning card.
/// If no card directly matches the winning card, return all unplayed cards.
pub fn pick_playable_cards_in_round(
    unplayed_cards: &[i32],
    current_winning_card: Option<i32>,
) -> Vec<i32> {
    if unplayed_cards.is_empty() {
        return vec![];
    }
    let has_match = unplayed_cards
        .iter()
        .any(|&c| can_match_card(c, current_winning_card));
    if has_match {
        unplayed_cards
            .iter()
            .filter(|&&c| can_match_card(c, current_winning_card))
            .copied()
            .collect()
    } else {
        unplayed_cards.to_vec()
    }
}

/// Pick the best card according to the given strategy choice.
/// The playable cards are filtered first (by matching suit).
/// Then the strategy selects a card based on zones (0-2, 3-5, 6-7) within the suit.
/// Zones are determined by `card % 8`:
/// - Zone 1: 0-2 (low rank)
/// - Zone 2: 3-5 (mid rank)
/// - Zone 3: 6-7 (high rank)
pub fn pick_best_card_from_strategy_choice(
    unplayed_cards: &[i32],
    current_winning_card: Option<i32>,
    strategy_choice: StrategyChoice,
) -> i32 {
    let playable = pick_playable_cards_in_round(unplayed_cards, current_winning_card);
    if playable.is_empty() {
        // Fallback: should not happen, but return first unplayed card
        return unplayed_cards[0];
    }

    // Helper to find first card in a given zone range.
    let find_in_zone = |zone_start: i32, zone_end: i32| -> Option<i32> {
        playable
            .iter()
            .find(|&&c| {
                let rem = c % 8;
                rem >= zone_start && rem <= zone_end
            })
            .copied()
    };

    match strategy_choice {
        StrategyChoice::LongUp => {
            // zone 3 -> zone 2 -> zone 1
            if let Some(card) = find_in_zone(6, 7) {
                return card;
            }
            if let Some(card) = find_in_zone(3, 5) {
                return card;
            }
            if let Some(card) = find_in_zone(0, 2) {
                return card;
            }
        }
        StrategyChoice::LongDown => {
            // zone 3 -> zone 1 -> zone 2
            if let Some(card) = find_in_zone(6, 7) {
                return card;
            }
            if let Some(card) = find_in_zone(0, 2) {
                return card;
            }
            if let Some(card) = find_in_zone(3, 5) {
                return card;
            }
        }
        StrategyChoice::MidUp => {
            // zone 2 -> zone 3 -> zone 1
            if let Some(card) = find_in_zone(3, 5) {
                return card;
            }
            if let Some(card) = find_in_zone(6, 7) {
                return card;
            }
            if let Some(card) = find_in_zone(0, 2) {
                return card;
            }
        }
        StrategyChoice::MidDown => {
            // zone 2 -> zone 1 -> zone 3
            if let Some(card) = find_in_zone(3, 5) {
                return card;
            }
            if let Some(card) = find_in_zone(0, 2) {
                return card;
            }
            if let Some(card) = find_in_zone(6, 7) {
                return card;
            }
        }
        StrategyChoice::ShortUp => {
            // zone 1 -> zone 3 -> zone 2
            if let Some(card) = find_in_zone(0, 2) {
                return card;
            }
            if let Some(card) = find_in_zone(6, 7) {
                return card;
            }
            if let Some(card) = find_in_zone(3, 5) {
                return card;
            }
        }
        StrategyChoice::ShortDown => {
            // zone 1 -> zone 2 -> zone 3
            if let Some(card) = find_in_zone(0, 2) {
                return card;
            }
            if let Some(card) = find_in_zone(3, 5) {
                return card;
            }
            if let Some(card) = find_in_zone(6, 7) {
                return card;
            }
        }
    }

    // If no card found in any zone (should not happen), return first playable card.
    playable[0]
}

/// Simple strategy: if no winning card, pick random.
/// Otherwise use MID_UP strategy (which will filter by suit).
#[allow(dead_code)]
fn compute_simple(
    unplayed_cards: &[i32],
    _round_played_cards: &[i32],
    current_winning_card: Option<i32>,
) -> i32 {
    if unplayed_cards.is_empty() {
        // Safety: no cards to play — should not happen in normal flow
        return -1;
    }
    if current_winning_card.is_none() {
        // random choice when no suit to follow
        *unplayed_cards.choose(&mut thread_rng()).unwrap()
    } else {
        pick_best_card_from_strategy_choice(
            unplayed_cards,
            current_winning_card,
            StrategyChoice::MidUp,
        )
    }
}

/// High strategy: if no winning card, pick random.
/// Otherwise pick a random strategy among LONG_UP, LONG_DOWN, MID_UP, MID_DOWN.
fn compute_high(
    unplayed_cards: &[i32],
    _round_played_cards: &[i32],
    current_winning_card: Option<i32>,
) -> i32 {
    if unplayed_cards.is_empty() {
        // Safety: no cards to play — should not happen in normal flow
        return -1;
    }
    if current_winning_card.is_none() {
        *unplayed_cards.choose(&mut thread_rng()).unwrap()
    } else {
        let strategy = StrategyChoice::random_high();
        pick_best_card_from_strategy_choice(unplayed_cards, current_winning_card, strategy)
    }
}

/// Main entry point for bot strategy.
/// Currently uses the high strategy (as per Python default).
/// Returns -1 if no cards are available to play (caller should handle this).
pub fn compute_strategy(
    unplayed_cards: &[i32],
    round_played_cards: &[i32],
    current_winning_card: Option<i32>,
) -> i32 {
    if unplayed_cards.is_empty() {
        return -1;
    }
    compute_high(unplayed_cards, round_played_cards, current_winning_card)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_can_match_card() {
        assert!(can_match_card(0, None));
        assert!(can_match_card(0, Some(0)));
        assert!(can_match_card(0, Some(7)));
        assert!(!can_match_card(0, Some(8)));
        assert!(can_match_card(8, Some(8)));
        assert!(can_match_card(15, Some(8)));
        assert!(!can_match_card(16, Some(8)));
    }

    #[test]
    fn test_pick_playable_cards_in_round() {
        let cards = vec![0, 8, 16, 24];
        let playable = pick_playable_cards_in_round(&cards, Some(0));
        assert_eq!(playable, vec![0]); // only same suit (0-7)
        let playable2 = pick_playable_cards_in_round(&cards, Some(10));
        // card 8 matches suit 1 (cards 8-15), returns [8]
        assert_eq!(playable2, vec![8]);

        // Test with no matching suit
        let cards2 = vec![0, 16, 24]; // no suit 1 cards
        let playable3 = pick_playable_cards_in_round(&cards2, Some(10));
        // no matching suit, returns all cards
        assert_eq!(playable3, cards2);
    }

    #[test]
    fn test_pick_best_card_from_strategy_choice() {
        let cards = vec![0, 1, 2, 3, 4, 5, 6, 7]; // all hearts
                                                  // For LongUp, should pick zone 3 (6-7) first
        let card = pick_best_card_from_strategy_choice(&cards, Some(0), StrategyChoice::LongUp);
        assert!(card == 6 || card == 7);
        // For ShortDown, should pick zone 1 (0-2) first
        let card = pick_best_card_from_strategy_choice(&cards, Some(0), StrategyChoice::ShortDown);
        assert!((0..=2).contains(&card));
    }

    #[test]
    fn test_compute_strategy() {
        let unplayed = vec![0, 1, 2, 3, 4, 5, 6, 7];
        let round_played = vec![];
        let card = compute_strategy(&unplayed, &round_played, None);
        assert!(unplayed.contains(&card));
        // With winning card, should pick according to high strategy (random among four)
        let round_played = vec![0];
        let card2 = compute_strategy(&unplayed, &round_played, Some(0));
        assert!(unplayed.contains(&card2));
    }
}
