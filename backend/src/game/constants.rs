pub const MAX_PLAYERS_IN_GAME: usize = 4;
pub const CARDS_PER_PLAYER: usize = 5;
pub const TOTAL_CARDS: usize = 32;
pub const SUITS: [&str; 4] = ["Hearts", "Spades", "Diamonds", "Clubs"];
pub const RANK_START: u8 = 3;
#[allow(dead_code)]
pub const RANK_END: u8 = 10;
pub const BOT_THINKING_DELAY_SECS: u64 = 1;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constants() {
        assert_eq!(MAX_PLAYERS_IN_GAME, 4);
        assert_eq!(CARDS_PER_PLAYER, 5);
        assert_eq!(TOTAL_CARDS, 32);
        assert_eq!(SUITS.len(), 4);
    }
}
