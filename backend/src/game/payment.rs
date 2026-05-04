pub fn calculate_payment(winner_position: usize, total_players: usize, bet: i32) -> Vec<i32> {
    let mut credits = vec![0; total_players];
    for i in 0..total_players {
        if i == winner_position {
            credits[i] = bet.saturating_mul(total_players as i32 - 1);
        } else {
            credits[i] = -bet;
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