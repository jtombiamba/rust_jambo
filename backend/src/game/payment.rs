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
    fn test_calculate_payment_4_players_winner_0() {
        let credits = calculate_payment(0, 4, 10);
        assert_eq!(credits, vec![30, -10, -10, -10]);
    }

    #[test]
    fn test_calculate_payment_4_players_winner_2() {
        let credits = calculate_payment(2, 4, 10);
        assert_eq!(credits, vec![-10, -10, 30, -10]);
    }

    #[test]
    fn test_calculate_payment_4_players_winner_3() {
        let credits = calculate_payment(3, 4, 10);
        assert_eq!(credits, vec![-10, -10, -10, 30]);
    }

    #[test]
    fn test_calculate_payment_3_players_winner_0() {
        let credits = calculate_payment(0, 3, 10);
        assert_eq!(credits, vec![20, -10, -10]);
    }

    #[test]
    fn test_calculate_payment_3_players_winner_2() {
        let credits = calculate_payment(2, 3, 10);
        assert_eq!(credits, vec![-10, -10, 20]);
    }

    #[test]
    fn test_calculate_payment_2_players_winner_0() {
        let credits = calculate_payment(0, 2, 10);
        assert_eq!(credits, vec![10, -10]);
    }

    #[test]
    fn test_calculate_payment_2_players_winner_1() {
        let credits = calculate_payment(1, 2, 10);
        assert_eq!(credits, vec![-10, 10]);
    }

    #[test]
    fn test_calculate_payment_kora_multiplier() {
        let credits = calculate_payment(0, 4, 20);
        assert_eq!(credits, vec![60, -20, -20, -20]);
    }

    #[test]
    fn test_net_outcome_2_players_normal() {
        let bet = 10;
        let total_players = 2;
        let kora_mult = 1;
        let multiplied_bet = bet * kora_mult;
        let initial_credit: Vec<i32> = vec![500, 500];
        // After upfront deduction: each player pays bet
        let after_deduction: Vec<i32> = initial_credit.iter().map(|c| c - bet).collect();
        assert_eq!(after_deduction, vec![490, 490]);
        // calculate_payment returns winner+(N-1)*bet*kora, losers -bet*kora
        let payments = calculate_payment(0, total_players, multiplied_bet);
        assert_eq!(payments, vec![10, -10]);
        // process_payment_in_txn: new_credits = player.credits + original_bet + payment
        let final_credits: Vec<i32> = after_deduction
            .iter()
            .zip(payments.iter())
            .map(|(c, p)| c + bet + p)
            .collect();
        assert_eq!(final_credits, vec![510, 490]);
    }

    #[test]
    fn test_net_outcome_3_players_normal() {
        let bet = 10;
        let total_players = 3;
        let kora_mult = 1;
        let multiplied_bet = bet * kora_mult;
        let initial_credit: Vec<i32> = vec![500, 500, 500];
        let after_deduction: Vec<i32> = initial_credit.iter().map(|c| c - bet).collect();
        let payments = calculate_payment(0, total_players, multiplied_bet);
        assert_eq!(payments, vec![20, -10, -10]);
        let final_credits: Vec<i32> = after_deduction
            .iter()
            .zip(payments.iter())
            .map(|(c, p)| c + bet + p)
            .collect();
        assert_eq!(final_credits, vec![520, 490, 490]);
    }

    #[test]
    fn test_net_outcome_4_players_normal() {
        let bet = 10;
        let total_players = 4;
        let kora_mult = 1;
        let multiplied_bet = bet * kora_mult;
        let initial_credit: Vec<i32> = vec![500, 500, 500, 500];
        let after_deduction: Vec<i32> = initial_credit.iter().map(|c| c - bet).collect();
        let payments = calculate_payment(0, total_players, multiplied_bet);
        assert_eq!(payments, vec![30, -10, -10, -10]);
        let final_credits: Vec<i32> = after_deduction
            .iter()
            .zip(payments.iter())
            .map(|(c, p)| c + bet + p)
            .collect();
        assert_eq!(final_credits, vec![530, 490, 490, 490]);
    }

    #[test]
    fn test_net_outcome_4_players_kora() {
        let bet = 10;
        let total_players = 4;
        let kora_mult = 2;
        let multiplied_bet = bet * kora_mult;
        let initial_credit: Vec<i32> = vec![500, 500, 500, 500];
        let after_deduction: Vec<i32> = initial_credit.iter().map(|c| c - bet).collect();
        let payments = calculate_payment(0, total_players, multiplied_bet);
        assert_eq!(payments, vec![60, -20, -20, -20]);
        let final_credits: Vec<i32> = after_deduction
            .iter()
            .zip(payments.iter())
            .map(|(c, p)| c + bet + p)
            .collect();
        assert_eq!(final_credits, vec![560, 480, 480, 480]);
    }

    #[test]
    fn test_net_outcome_4_players_double_kora() {
        let bet = 10;
        let total_players = 4;
        let kora_mult = 4;
        let multiplied_bet = bet * kora_mult;
        let initial_credit: Vec<i32> = vec![500, 500, 500, 500];
        let after_deduction: Vec<i32> = initial_credit.iter().map(|c| c - bet).collect();
        let payments = calculate_payment(0, total_players, multiplied_bet);
        assert_eq!(payments, vec![120, -40, -40, -40]);
        let final_credits: Vec<i32> = after_deduction
            .iter()
            .zip(payments.iter())
            .map(|(c, p)| c + bet + p)
            .collect();
        assert_eq!(final_credits, vec![620, 460, 460, 460]);
    }

    #[test]
    fn test_net_outcome_2_players_kora() {
        let bet = 20;
        let total_players = 2;
        let kora_mult = 2;
        let multiplied_bet = bet * kora_mult;
        let initial_credit: Vec<i32> = vec![1000, 1000];
        let after_deduction: Vec<i32> = initial_credit.iter().map(|c| c - bet).collect();
        let payments = calculate_payment(1, total_players, multiplied_bet);
        assert_eq!(payments, vec![-40, 40]);
        let final_credits: Vec<i32> = after_deduction
            .iter()
            .zip(payments.iter())
            .map(|(c, p)| c + bet + p)
            .collect();
        assert_eq!(final_credits, vec![960, 1040]);
    }

    #[test]
    fn test_net_outcome_winner_position_3() {
        let bet = 10;
        let total_players = 4;
        let multiplied_bet = bet;
        let initial_credit: Vec<i32> = vec![500, 500, 500, 500];
        let after_deduction: Vec<i32> = initial_credit.iter().map(|c| c - bet).collect();
        let payments = calculate_payment(3, total_players, multiplied_bet);
        assert_eq!(payments, vec![-10, -10, -10, 30]);
        let final_credits: Vec<i32> = after_deduction
            .iter()
            .zip(payments.iter())
            .map(|(c, p)| c + bet + p)
            .collect();
        assert_eq!(final_credits, vec![490, 490, 490, 530]);
    }
}
