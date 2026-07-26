use crate::database::models::player_profile;

pub(crate) struct CreditCalculator {
    freeze_duration_secs: u64,
    unfreeze_credit_no_payment: i32,
}

pub(crate) struct CreditResult {
    pub final_credit: i32,
    pub frozen_until: Option<chrono::DateTime<chrono::Utc>>,
}

impl CreditCalculator {
    pub(crate) fn new(freeze_duration_secs: u64, unfreeze_credit_no_payment: i32) -> Self {
        Self {
            freeze_duration_secs,
            unfreeze_credit_no_payment,
        }
    }

    pub(crate) fn compute_joining_credit(
        &self,
        profile: &player_profile::Model,
        bet: i32,
        now: chrono::DateTime<chrono::Utc>,
    ) -> CreditResult {
        let new_credit = profile.credit - bet;
        let freeze_duration = chrono::Duration::seconds(self.freeze_duration_secs as i64);
        let was_previously_frozen = profile.frozen_until.is_some();

        let (final_credit, frozen_until) = if new_credit <= 0 {
            (new_credit, Some(now + freeze_duration))
        } else if was_previously_frozen {
            let auto_unfreeze_credit = if new_credit < self.unfreeze_credit_no_payment {
                self.unfreeze_credit_no_payment
            } else {
                new_credit
            };
            (auto_unfreeze_credit, None)
        } else {
            (new_credit, profile.frozen_until)
        };

        CreditResult {
            final_credit,
            frozen_until,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_profile(
        credit: i32,
        frozen_until: Option<chrono::DateTime<Utc>>,
    ) -> player_profile::Model {
        player_profile::Model {
            id: uuid::Uuid::now_v7(),
            user_id: uuid::Uuid::now_v7(),
            player_type: crate::database::models::PlayerType::Human,
            credit,
            game_played: 0,
            wins: 0,
            kora_wins: 0,
            winning_streak: 0,
            latitude: None,
            longitude: None,
            country_code: None,
            city: None,
            frozen_until,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn test_normal_join_sufficient_credit() {
        let calc = CreditCalculator::new(86400, 250);
        let profile = make_profile(500, None);
        let result = calc.compute_joining_credit(&profile, 100, Utc::now());
        assert_eq!(result.final_credit, 400);
        assert!(result.frozen_until.is_none());
    }

    #[test]
    fn test_join_drains_credit_to_zero() {
        let calc = CreditCalculator::new(86400, 250);
        let profile = make_profile(100, None);
        let result = calc.compute_joining_credit(&profile, 100, Utc::now());
        assert_eq!(result.final_credit, 0);
        assert!(result.frozen_until.is_some());
    }

    #[test]
    fn test_join_leaves_negative_credit_and_freezes() {
        let calc = CreditCalculator::new(86400, 250);
        let profile = make_profile(50, None);
        let result = calc.compute_joining_credit(&profile, 100, Utc::now());
        assert_eq!(result.final_credit, -50);
        assert!(result.frozen_until.is_some());
    }

    #[test]
    fn test_previously_frozen_auto_unfreeze_below_threshold() {
        let calc = CreditCalculator::new(86400, 250);
        let profile = make_profile(300, Some(Utc::now()));
        let result = calc.compute_joining_credit(&profile, 100, Utc::now());
        assert_eq!(result.final_credit, 250);
        assert!(result.frozen_until.is_none());
    }

    #[test]
    fn test_previously_frozen_auto_unfreeze_above_threshold() {
        let calc = CreditCalculator::new(86400, 250);
        let profile = make_profile(500, Some(Utc::now()));
        let result = calc.compute_joining_credit(&profile, 100, Utc::now());
        assert_eq!(result.final_credit, 400);
        assert!(result.frozen_until.is_none());
    }

    #[test]
    fn test_keep_existing_freeze_if_credit_positive_and_not_previously_frozen() {
        let calc = CreditCalculator::new(86400, 250);
        let profile = make_profile(500, None);
        let result = calc.compute_joining_credit(&profile, 100, Utc::now());
        assert_eq!(result.final_credit, 400);
        assert!(result.frozen_until.is_none());
    }
}
