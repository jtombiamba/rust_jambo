use chrono::{Datelike, TimeZone, Utc};

/// Start of the current calendar month in UTC (midnight on the 1st).
pub fn current_month_start(now: chrono::DateTime<Utc>) -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(now.year(), now.month(), 1, 0, 0, 0)
        .single()
        .expect("first day of month is always a valid UTC datetime")
}

/// Parse a decimal euro amount string (e.g. "1.00") into integer euro cents.
///
/// Malformed or empty input yields `0`, mirroring how payment amount strings
/// are validated by PayPal before this point.
pub fn parse_eur_to_cents(amount: &str) -> i32 {
    let trimmed = amount.trim();
    if trimmed.is_empty() {
        return 0;
    }

    let mut parts = trimmed.split('.');
    let euros: i32 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    let cents_str = parts.next().unwrap_or("");
    let cents = if cents_str.is_empty() {
        0
    } else {
        let mut padded = cents_str.to_string();
        padded.truncate(2);
        while padded.len() < 2 {
            padded.push('0');
        }
        padded.parse::<i32>().unwrap_or(0)
    };

    euros.saturating_mul(100).saturating_add(cents)
}

/// True when spending `upcoming_cents` on top of `spent_cents` exceeds the cap.
pub fn monthly_limit_exceeded(spent_cents: i32, upcoming_cents: i32, limit_cents: i32) -> bool {
    spent_cents.saturating_add(upcoming_cents) > limit_cents
}

#[cfg(test)]
mod tests {
    use super::*;

    fn month_start(year: i32, month: u32) -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(year, month, 1, 0, 0, 0)
            .single()
            .unwrap()
    }

    #[test]
    fn month_start_truncates_day_and_time() {
        let now = Utc
            .with_ymd_and_hms(2026, 9, 23, 15, 42, 7)
            .single()
            .unwrap();
        assert_eq!(current_month_start(now), month_start(2026, 9));
    }

    #[test]
    fn month_start_handles_year_boundary() {
        let now = Utc.with_ymd_and_hms(2026, 1, 5, 0, 0, 0).single().unwrap();
        assert_eq!(current_month_start(now), month_start(2026, 1));
    }

    #[test]
    fn parse_whole_euros() {
        assert_eq!(parse_eur_to_cents("1"), 100);
        assert_eq!(parse_eur_to_cents("100"), 10000);
    }

    #[test]
    fn parse_two_decimal_euros() {
        assert_eq!(parse_eur_to_cents("1.00"), 100);
        assert_eq!(parse_eur_to_cents("0.50"), 50);
        assert_eq!(parse_eur_to_cents("2.50"), 250);
    }

    #[test]
    fn parse_single_decimal_euros_pads_to_cents() {
        assert_eq!(parse_eur_to_cents("1.5"), 150);
    }

    #[test]
    fn parse_overlong_decimals_truncates() {
        assert_eq!(parse_eur_to_cents("1.999"), 199);
    }

    #[test]
    fn parse_malformed_inputs_yield_zero() {
        assert_eq!(parse_eur_to_cents(""), 0);
        assert_eq!(parse_eur_to_cents("abc"), 0);
    }

    #[test]
    fn parse_ignores_surrounding_whitespace() {
        assert_eq!(parse_eur_to_cents(" 1.00 "), 100);
    }

    #[test]
    fn limit_not_exceeded_when_below_cap() {
        assert!(!monthly_limit_exceeded(9000, 100, 10000));
        assert!(!monthly_limit_exceeded(9900, 100, 10000));
    }

    #[test]
    fn limit_not_exceeded_when_exactly_at_cap() {
        assert!(!monthly_limit_exceeded(9900, 100, 10000));
        assert!(!monthly_limit_exceeded(10000, 0, 10000));
    }

    #[test]
    fn limit_exceeded_when_above_cap() {
        assert!(monthly_limit_exceeded(10000, 100, 10000));
        assert!(monthly_limit_exceeded(0, 10001, 10000));
    }

    #[test]
    fn limit_saturates_on_overflow() {
        assert!(monthly_limit_exceeded(i32::MAX, 1, 10000));
    }
}
