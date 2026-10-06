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
