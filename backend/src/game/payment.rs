pub fn calculate_payment(winner_position: usize, total_players: usize, bet: i32) -> Vec<i32> {
    let mut credits = vec![0; total_players];
    for (i, credit) in credits.iter_mut().enumerate() {
        if i == winner_position {
            *credit = bet.saturating_mul(total_players as i32 - 1);
        } else {
            *credit = -bet;
        }
    }
    credits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_payment() {
        let credits = calculate_payment(0, 4, 10);
        assert_eq!(credits, vec![30, -10, -10, -10]);
    }
}
