use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::game::constants::RANK_START;

/// The three "special card" combinations that can be detected in a 5-card hand.
///
/// Values are the card ranks (3..=10), derived from a card index with
/// `rank = RANK_START + (index % 8)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SpecialCards {
    /// At least three 7s in hand.
    pub check_triple_seven: bool,
    /// Sum of the five card ranks strictly below 21.
    pub check_sum_value_under_21: bool,
    /// Four cards of the same rank.
    pub check_a_square: bool,
}

impl SpecialCards {
    /// Whether the hand holds at least one of the three special combinations.
    pub fn has_any(&self) -> bool {
        self.check_triple_seven || self.check_sum_value_under_21 || self.check_a_square
    }

    /// The value of the strongest special combination held, used to rank hands:
    /// `a_square` (2) > `sum_under_21` (1) > `triple_seven` (0). `None` when the
    /// hand holds none.
    pub fn best_rank(&self) -> Option<u8> {
        if self.check_a_square {
            Some(2)
        } else if self.check_sum_value_under_21 {
            Some(1)
        } else if self.check_triple_seven {
            Some(0)
        } else {
            None
        }
    }
}

/// Compute the special-card combinations for a hand of card indices.
///
/// Card indices are 0..32 (`TOTAL_CARDS`); a card's rank is
/// `RANK_START + (index % 8)`.
pub fn compute_special_cards(cards: &[i32]) -> SpecialCards {
    let ranks: Vec<u8> = cards
        .iter()
        .map(|&idx| RANK_START + ((idx.rem_euclid(8)) as u8))
        .collect();

    let mut rank_counts = [0u8; 8]; // ranks 3..=10 -> index (rank - 3)
    for &rank in &ranks {
        if (3..=10).contains(&rank) {
            rank_counts[(rank - 3) as usize] += 1;
        }
    }

    let check_triple_seven = rank_counts[(7 - 3) as usize] >= 3;
    let check_a_square = rank_counts.iter().any(|&count| count >= 4);
    let sum: u32 = ranks.iter().map(|&r| u32::from(r)).sum();
    let check_sum_value_under_21 = sum < 21;

    SpecialCards {
        check_triple_seven,
        check_sum_value_under_21,
        check_a_square,
    }
}

/// Select the single player who is offered the special-card claim.
///
/// The askee is the unique holder of the highest-ranked special combination.
/// When no one holds a combination, or two players tie at the top rank, nobody
/// is asked (`None`).
pub fn select_askee(hands: &[(Uuid, SpecialCards)]) -> Option<Uuid> {
    let mut best_rank: Option<u8> = None;
    let mut askee: Option<Uuid> = None;

    for &(player_id, specials) in hands {
        let Some(rank) = specials.best_rank() else {
            continue;
        };
        match best_rank {
            None => {
                best_rank = Some(rank);
                askee = Some(player_id);
            }
            Some(current) if rank > current => {
                best_rank = Some(rank);
                askee = Some(player_id);
            }
            Some(current) if rank == current => {
                askee = None;
                let _ = current;
            }
            Some(_) => {}
        }
    }

    askee
}

#[cfg(test)]
mod tests {
    use super::*;

    fn specials(cards: &[i32]) -> SpecialCards {
        compute_special_cards(cards)
    }

    #[test]
    fn no_special_combination() {
        // 3,4,5,6,7 of Hearts (indices 0..4) -> no triple seven, sum = 25, no square
        let result = specials(&[0, 1, 2, 3, 4]);
        assert!(!result.check_triple_seven);
        assert!(!result.check_sum_value_under_21);
        assert!(!result.check_a_square);
        assert!(!result.has_any());
        assert_eq!(result.best_rank(), None);
    }

    #[test]
    fn triple_seven_detected() {
        // Three sevens: 7H, 7S, 7D are indices 4, 12, 20 (rank 7 = index % 8 == 4)
        let result = specials(&[4, 12, 20, 0, 1]);
        assert!(result.check_triple_seven);
        assert!(!result.check_a_square);
        // sum = 7+7+7+3+4 = 28, not under 21
        assert!(!result.check_sum_value_under_21);
        assert_eq!(result.best_rank(), Some(0));
    }

    #[test]
    fn four_of_a_kind_detected() {
        // Four 3s: 3H,3S,3D,3C are indices 0,8,16,24 + one extra
        let result = specials(&[0, 8, 16, 24, 5]);
        assert!(result.check_a_square);
        assert!(!result.check_triple_seven);
        assert_eq!(result.best_rank(), Some(2));
    }

    #[test]
    fn sum_under_21_detected() {
        // 3,3,3,4,4 (indices 0,8,16,1,9) -> sum = 17
        let result = specials(&[0, 8, 16, 1, 9]);
        assert!(result.check_sum_value_under_21);
        assert!(!result.check_a_square);
        assert!(!result.check_triple_seven);
        assert_eq!(result.best_rank(), Some(1));
    }

    #[test]
    fn sum_exactly_21_is_not_under_21() {
        // 3,3,3,6,6 -> sum = 21 (indices 0,8,16,3,11)
        let result = specials(&[0, 8, 16, 3, 11]);
        assert!(!result.check_sum_value_under_21);
    }

    #[test]
    fn rank_precedence_square_beats_sum() {
        // A square also makes sum low only if ranks are low; use a square of 4s
        // (indices 1,9,17,25) plus a 3 (index 0) -> sum = 4*4+3 = 19 < 21 AND square
        let result = specials(&[1, 9, 17, 25, 0]);
        assert!(result.check_a_square);
        assert_eq!(result.best_rank(), Some(2));
    }

    #[test]
    fn select_askee_unique_top_holder() {
        let a = Uuid::now_v7();
        let b = Uuid::now_v7();
        let hands = vec![
            (a, specials(&[4, 12, 20, 0, 1])), // triple seven (rank 0)
            (b, specials(&[0, 8, 16, 24, 5])), // square (rank 2)
        ];
        assert_eq!(select_askee(&hands), Some(b));
    }

    #[test]
    fn select_askee_tie_at_top_returns_none() {
        let a = Uuid::now_v7();
        let b = Uuid::now_v7();
        let hands = vec![
            (a, specials(&[0, 8, 16, 24, 5])), // square
            (b, specials(&[1, 9, 17, 25, 5])), // square
        ];
        assert_eq!(select_askee(&hands), None);
    }

    #[test]
    fn select_askee_no_holders_returns_none() {
        let a = Uuid::now_v7();
        let b = Uuid::now_v7();
        let hands = vec![
            (a, specials(&[0, 1, 2, 3, 4])),
            (b, specials(&[8, 9, 10, 11, 12])),
        ];
        assert_eq!(select_askee(&hands), None);
    }

    #[test]
    fn select_askee_lower_rank_loses_to_higher() {
        let a = Uuid::now_v7();
        let b = Uuid::now_v7();
        let c = Uuid::now_v7();
        let hands = vec![
            (a, specials(&[4, 12, 20, 0, 1])), // triple seven (0)
            (b, specials(&[0, 8, 16, 1, 9])),  // sum under 21 (1)
            (c, specials(&[0, 8, 16, 24, 5])), // square (2)
        ];
        assert_eq!(select_askee(&hands), Some(c));
    }
}
