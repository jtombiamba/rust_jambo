use crate::api::dto::dashboard::{PaginationParams, PlayerProfileResponse};

fn params() -> PaginationParams {
    PaginationParams {
        page: None,
        per_page: None,
        status: None,
        order_by: None,
        bet_min: None,
        bet_max: None,
    }
}

#[test]
fn to_filter_uses_defaults_when_empty() {
    let filter = params().to_filter();
    assert!(filter.statuses.is_empty());
    assert_eq!(filter.order_by, "date_desc");
    assert_eq!(filter.bet_min, None);
    assert_eq!(filter.bet_max, None);
}

#[test]
fn to_filter_splits_and_normalizes_statuses() {
    let mut p = params();
    p.status = Some(" Active, Finished , PENDING ".to_string());
    let filter = p.to_filter();
    assert_eq!(filter.statuses, vec!["active", "finished", "pending"]);
}

#[test]
fn to_filter_preserves_order_by_and_bet_bounds() {
    let mut p = params();
    p.order_by = Some("bet_asc".to_string());
    p.bet_min = Some(5);
    p.bet_max = Some(50);
    let filter = p.to_filter();
    assert_eq!(filter.order_by, "bet_asc");
    assert_eq!(filter.bet_min, Some(5));
    assert_eq!(filter.bet_max, Some(50));
}

#[test]
fn player_profile_response_skips_none_frozen_until() {
    let response = PlayerProfileResponse {
        credit: 100,
        game_played: 3,
        wins: 1,
        kora_wins: 0,
        frozen_until: None,
        cashout_locked: false,
    };
    let json = serde_json::to_value(&response).unwrap();
    assert!(json.get("frozen_until").is_none());
    assert_eq!(json["credit"], 100);
}
