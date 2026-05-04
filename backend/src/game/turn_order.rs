pub fn next_player(current: usize, total_players: usize) -> usize {
    (current + 1) % total_players
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_next_player() {
        assert_eq!(next_player(0, 4), 1);
        assert_eq!(next_player(3, 4), 0);
        assert_eq!(next_player(2, 4), 3);
    }
}