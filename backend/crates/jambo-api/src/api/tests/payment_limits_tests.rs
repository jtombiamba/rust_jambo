use crate::api::payment_limits::{current_month_start, monthly_limit_exceeded, parse_eur_to_cents};
use chrono::{TimeZone, Utc};

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
