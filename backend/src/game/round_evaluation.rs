use crate::game::card_mapping::Card;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayedCard {
    pub player_position: usize,
    pub card: Card,
}

/// Pure data context for round evaluation — no ORM, no DB dependency.
///
/// Carries the cards played in the round and the leading card
/// (the first card played in this round by the first player).
///
/// The first player of a round is the winner of the previous round.
/// For round 1, the first player is randomly chosen at game creation.
/// The first player's card determines the leading suit for the round.
#[derive(Debug, Clone)]
pub struct RoundContext {
    /// Cards played in this round, one per player.
    pub plays: Vec<PlayedCard>,
    /// The first card played in this round (by the first player).
    /// This card determines the leading suit for this round.
    /// Always `Some` when called from the game service.
    pub leading_card: Option<Card>,
    /// The player position (0-3) who played the first card (the leading card).
    /// This is the winner of the previous round (or randomly chosen for round 1).
    /// Always `Some` when called from the game service.
    pub leading_player_position: Option<usize>,
}

/// Result of a round evaluation — pure data, no ORM dependency.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoundResult {
    /// Player position (0-3) of the round winner.
    pub winner_position: usize,
    /// Whether the winning card is a 3 of a suit (indices 0, 8, 16, 24),
    /// which triggers a Kora finish.
    pub is_kora: bool,
}

/// Evaluate a completed round and determine the winner.
///
/// The leading suit is determined by `ctx.leading_card`:
/// - If `Some(card)`: the suit of that card is the leading suit
///   (this is the first card played in the current round).
/// - If `None` (should not happen in practice): falls back to the
///   first player's card in `plays`.
///
/// Only cards of the leading suit are considered; the highest rank among them wins.
/// Returns `None` if there are no plays.
pub fn evaluate_round(ctx: &RoundContext) -> Option<RoundResult> {
    if ctx.plays.is_empty() {
        return None;
    }

    // Determine the leading suit from the first card played in this round.
    let leading_card = ctx.leading_card.unwrap_or(ctx.plays[0].card);
    let leading_suit = leading_card.index / 8;

    // Filter cards of the same suit as the leading card
    let same_suit: Vec<_> = ctx
        .plays
        .iter()
        .filter(|p| p.card.index / 8 == leading_suit)
        .collect();

    if same_suit.is_empty() {
        // No one followed the leading suit.
        // The player who set the leading suit (first player) wins.
        // This shouldn't happen in practice since the first player's card
        // sets the suit and they are always in same_suit, but we handle it
        // defensively for edge cases.
        let winner_position = ctx
            .leading_player_position
            .unwrap_or(ctx.plays[0].player_position);
        let is_kora = leading_card.index % 8 == 0;
        return Some(RoundResult {
            winner_position,
            is_kora,
        });
    }

    // Find the highest rank among same-suit cards
    same_suit
        .iter()
        .max_by_key(|p| p.card.rank)
        .map(|winner| {
            let is_kora = winner.card.index % 8 == 0;
            RoundResult {
                winner_position: winner.player_position,
                is_kora,
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_card(index: u8) -> Card {
        Card::new(index).unwrap()
    }

    fn make_play(position: usize, card_index: u8) -> PlayedCard {
        PlayedCard {
            player_position: position,
            card: make_card(card_index),
        }
    }

    #[test]
    fn test_evaluate_round_with_leading_card() {
        // Scenario: previous round winner played 3♠ (index 8, Spades),
        // so Spades is the leading suit.
        let ctx = RoundContext {
            leading_card: Some(make_card(8)), // 3♠ (Spades)
            leading_player_position: Some(2), // previous winner was player 2
            plays: vec![
                make_play(0, 0),  // 3♥ (Hearts, different suit)
                make_play(1, 8),  // 3♠ (Spades, same suit)
                make_play(2, 9),  // 4♠ (Spades, same suit, higher)
                make_play(3, 16), // 3♦ (Diamonds, different suit)
            ],
        };
        // Leading suit is Spades (suit 1), highest rank among Spades is 4♠ (player 2)
        let result = evaluate_round(&ctx);
        assert_eq!(result, Some(RoundResult {
            winner_position: 2,
            is_kora: false,
        }));
    }

    #[test]
    fn test_evaluate_round_first_round_no_leading_card() {
        // First round: no leading card, first player's card determines suit
        let ctx = RoundContext {
            leading_card: None,
            leading_player_position: None,
            plays: vec![
                make_play(0, 0),  // 3♥ (Hearts, leading suit)
                make_play(1, 8),  // 3♠ (Spades, different suit)
                make_play(2, 1),  // 4♥ (Hearts, same suit)
                make_play(3, 2),  // 5♥ (Hearts, same suit, highest)
            ],
        };
        // Leading suit is Hearts (suit 0), highest rank among Hearts is 5♥ (player 3)
        let result = evaluate_round(&ctx);
        assert_eq!(result, Some(RoundResult {
            winner_position: 3,
            is_kora: false,
        }));
    }

    #[test]
    fn test_all_different_suit() {
        // If all cards are different suits, only the leading card's suit matters
        let ctx = RoundContext {
            leading_card: Some(make_card(0)), // 3♥ (Hearts)
            leading_player_position: Some(0),
            plays: vec![
                make_play(0, 0),  // 3♥ (Hearts, leading suit)
                make_play(1, 8),  // 3♠ (Spades)
                make_play(2, 16), // 3♦ (Diamonds)
                make_play(3, 24), // 3♣ (Clubs)
            ],
        };
        // Only card 0 is of the leading suit (Hearts), so player 0 wins
        let result = evaluate_round(&ctx);
        assert_eq!(result, Some(RoundResult {
            winner_position: 0,
            is_kora: true, // index 0 is a 3 of Hearts -> Kora
        }));
    }

    #[test]
    fn test_same_suit_different_ranks() {
        // All cards same suit, highest rank wins
        let ctx = RoundContext {
            leading_card: Some(make_card(0)), // 3♥ (Hearts)
            leading_player_position: Some(0),
            plays: vec![
                make_play(0, 0), // 3♥
                make_play(1, 6), // 10♥ (higher rank)
                make_play(2, 3), // 6♥
            ],
        };
        // Highest rank is 10♥ (card 6), player 1 wins
        let result = evaluate_round(&ctx);
        assert_eq!(result, Some(RoundResult {
            winner_position: 1,
            is_kora: false,
        }));
    }

    #[test]
    fn test_empty_plays() {
        let ctx = RoundContext {
            leading_card: None,
            leading_player_position: None,
            plays: vec![],
        };
        assert_eq!(evaluate_round(&ctx), None);
    }

    #[test]
    fn test_kora_detection() {
        // Winning card is index 0 (3♥) -> Kora
        let ctx = RoundContext {
            leading_card: Some(make_card(0)), // 3♥
            leading_player_position: Some(0),
            plays: vec![
                make_play(0, 0), // 3♥ (wins, Kora)
                make_play(1, 1), // 4♥ (same suit, higher rank but irrelevant)
            ],
        };
        let result = evaluate_round(&ctx);
        assert_eq!(result, Some(RoundResult {
            winner_position: 1, // player 1 has 4♥ which beats 3♥
            is_kora: false,     // 4♥ is not a Kora card
        }));

        // Kora card wins (index 24 = 3♣)
        let ctx = RoundContext {
            leading_card: Some(make_card(24)), // 3♣
            leading_player_position: Some(0),
            plays: vec![
                make_play(0, 24), // 3♣ (wins, Kora)
                make_play(1, 16), // 3♦ (different suit)
            ],
        };
        let result = evaluate_round(&ctx);
        assert_eq!(result, Some(RoundResult {
            winner_position: 0,
            is_kora: true, // 3♣ is a Kora card
        }));
    }

    #[test]
    fn test_leading_card_matches_first_player() {
        // The leading card is the first card played (by player 0).
        // This tests the new behavior: leading_card = plays[0].card.
        let ctx = RoundContext {
            leading_card: Some(make_card(0)), // 3♥ (Hearts) - first player's card
            leading_player_position: Some(0),
            plays: vec![
                make_play(0, 0),  // 3♥ (Hearts, leading suit)
                make_play(1, 8),  // 3♠ (Spades, NOT leading suit)
                make_play(2, 16), // 3♦ (Diamonds, NOT leading suit)
                make_play(3, 17), // 4♦ (Diamonds, NOT leading suit)
            ],
        };
        // Leading suit is Hearts (suit 0), only player 0 has Hearts -> player 0 wins
        let result = evaluate_round(&ctx);
        assert_eq!(result, Some(RoundResult {
            winner_position: 0,
            is_kora: true, // index 0 is 3♥ -> Kora
        }));
    }

    #[test]
    fn test_no_one_follows_suit() {
        // Leading card is Clubs (suit 3), but no one plays Clubs.
        // The first player (player 0, who set the leading suit) should win.
        let ctx = RoundContext {
            leading_card: Some(make_card(28)), // 4♣ (Clubs) - first player's card
            leading_player_position: Some(0),
            plays: vec![
                make_play(0, 28), // 4♣ (Clubs, leading suit)
                make_play(1, 8),  // 3♠ (Spades)
                make_play(2, 16), // 3♦ (Diamonds)
                make_play(3, 1),  // 4♥ (Hearts)
            ],
        };
        // No Clubs played (only player 0 has Clubs, but they set the suit),
        // so player 0 (who set the leading suit) wins
        let result = evaluate_round(&ctx);
        assert_eq!(result, Some(RoundResult {
            winner_position: 0,
            is_kora: false, // 28 is 4♣, not a 3
        }));
    }

    #[test]
    fn test_no_one_follows_suit_kora() {
        // Leading card is a 3 (Kora card), no one follows suit.
        // The first player should win with Kora.
        let ctx = RoundContext {
            leading_card: Some(make_card(24)), // 3♣ (Clubs, Kora card) - first player's card
            leading_player_position: Some(0),
            plays: vec![
                make_play(0, 24), // 3♣ (Clubs, Kora card, leading suit)
                make_play(1, 8),  // 3♠ (Spades)
                make_play(2, 16), // 3♦ (Diamonds)
                make_play(3, 1),  // 4♥ (Hearts)
            ],
        };
        // No Clubs played (only player 0 has Clubs), player 0 wins with Kora
        let result = evaluate_round(&ctx);
        assert_eq!(result, Some(RoundResult {
            winner_position: 0,
            is_kora: true, // 24 is 3♣ -> Kora
        }));
    }
}
