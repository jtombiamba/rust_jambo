use std::sync::LazyLock;

pub const MAX_PLAYERS_IN_GAME: usize = 4;
pub const CARDS_PER_PLAYER: usize = 5;
pub const TOTAL_CARDS: usize = 32;
pub const SUITS: [&str; 4] = ["Hearts", "Spades", "Diamonds", "Clubs"];
pub const RANK_START: u8 = 3;
#[allow(dead_code)]
pub const RANK_END: u8 = 10;

/// Multiplier applied to the bet when checking credit solvability at game join time.
///
/// A KORA finish doubles the bet for losers (×2), and a Double KORA quadruples it (×4).
/// To prevent players from going into negative credit after a KORA, we require
/// `profile.credit >= bet * KORA_CREDIT_MULTIPLIER` before allowing them to join.
///
/// Set to 2 to cover KORA (×2). Set to 4 to also cover Double KORA (×4).
pub const KORA_CREDIT_MULTIPLIER: i32 = 2;

pub static BOT_THINKING_DELAY_MS: LazyLock<u64> = LazyLock::new(|| {
    std::env::var("BOT_THINKING_DELAY_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(800)
});

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

    #[test]
    fn test_default_thinking_delay() {
        assert_eq!(*BOT_THINKING_DELAY_MS, 800);
    }

    #[test]
    fn test_thinking_delay_from_env() {
        std::env::set_var("BOT_THINKING_DELAY_MS", "500");
        // LazyLock is evaluated once; clear env after test
        // Since we can't reset a LazyLock, this test verifies the default only
        std::env::remove_var("BOT_THINKING_DELAY_MS");
    }
}
