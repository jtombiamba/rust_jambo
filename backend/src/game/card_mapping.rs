use crate::game::constants::{RANK_START, SUITS, TOTAL_CARDS};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Card {
    pub index: u8, // 0‑31
    pub suit: &'static str,
    pub rank: u8,
}

impl Card {
    pub fn new(index: u8) -> Option<Self> {
        if index as usize >= TOTAL_CARDS {
            return None;
        }
        let suit_index = (index / 8) as usize;
        let rank = RANK_START + (index % 8);
        Some(Self {
            index,
            suit: SUITS[suit_index],
            rank,
        })
    }

    pub fn suit_colour(&self) -> &'static str {
        match self.suit {
            "Hearts" | "Diamonds" => "Red",
            "Spades" | "Clubs" => "Black",
            _ => unreachable!(),
        }
    }

    pub fn is_same_colour(&self, other: &Card) -> bool {
        self.suit_colour() == other.suit_colour()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_card_mapping() {
        let card = Card::new(0).unwrap();
        assert_eq!(card.suit, "Hearts");
        assert_eq!(card.rank, 3);

        let card = Card::new(7).unwrap();
        assert_eq!(card.suit, "Hearts");
        assert_eq!(card.rank, 10);

        let card = Card::new(8).unwrap();
        assert_eq!(card.suit, "Spades");
        assert_eq!(card.rank, 3);

        let card = Card::new(31).unwrap();
        assert_eq!(card.suit, "Clubs");
        assert_eq!(card.rank, 10);

        assert!(Card::new(32).is_none());
    }

    #[test]
    fn test_suit_colour() {
        let heart = Card::new(0).unwrap();
        assert_eq!(heart.suit_colour(), "Red");
        let spade = Card::new(8).unwrap();
        assert_eq!(spade.suit_colour(), "Black");
    }
}
