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
/// A KORA finish doubles the bet for losers (×2). We require
/// `profile.credit >= bet * KORA_CREDIT_MULTIPLIER` before allowing players to join.
pub const KORA_CREDIT_MULTIPLIER: i32 = 2;

/// Multiplier applied to the bet for a Double KORA finish (×4).
/// A Double KORA occurs when the same player wins rounds 4 and 5 both with a 3.
pub const DOUBLE_KORA_CREDIT_MULTIPLIER: i32 = 4;

pub const DEFAULT_BOT_THINKING_DELAY_MS: u64 = 800;
pub const DEFAULT_ROUND_PAUSE_DELAY_MS: u64 = 2500;

/// Resolve a delay from an optional raw environment value, falling back to
/// `default` when the value is absent or unparseable.
fn parse_delay_ms(raw: Option<&str>, default: u64) -> u64 {
    raw.and_then(|v| v.parse().ok()).unwrap_or(default)
}

pub static BOT_THINKING_DELAY_MS: LazyLock<u64> = LazyLock::new(|| {
    parse_delay_ms(
        std::env::var("BOT_THINKING_DELAY_MS").ok().as_deref(),
        DEFAULT_BOT_THINKING_DELAY_MS,
    )
});

pub static ROUND_PAUSE_DELAY_MS: LazyLock<u64> = LazyLock::new(|| {
    parse_delay_ms(
        std::env::var("ROUND_PAUSE_DELAY_MS").ok().as_deref(),
        DEFAULT_ROUND_PAUSE_DELAY_MS,
    )
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
        assert_eq!(KORA_CREDIT_MULTIPLIER, 2);
        assert_eq!(DOUBLE_KORA_CREDIT_MULTIPLIER, 4);
    }

    #[test]
    fn test_default_thinking_delay() {
        assert_eq!(*BOT_THINKING_DELAY_MS, DEFAULT_BOT_THINKING_DELAY_MS);
    }

    #[test]
    fn test_thinking_delay_from_env() {
        assert_eq!(parse_delay_ms(Some("500"), 800), 500);
    }

    #[test]
    fn test_delay_falls_back_to_default_when_unparseable() {
        assert_eq!(parse_delay_ms(Some("not-a-number"), 800), 800);
        assert_eq!(parse_delay_ms(None, 800), 800);
    }
}
