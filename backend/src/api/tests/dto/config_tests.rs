use crate::api::dto::config::ClientConfigResponse;

#[test]
fn client_config_response_serializes() {
    let response = ClientConfigResponse {
        paypal_donate_url: "https://paypal.me/x".into(),
        bot_thinking_delay_ms: 1000,
        round_pause_delay_ms: 2000,
        cashout_enabled: true,
        cashout_min_credits: 250,
        cashout_credits_per_eur: 250,
        cashout_max_eur_cents: 2000,
    };
    let json = serde_json::to_value(&response).unwrap();
    assert_eq!(json["bot_thinking_delay_ms"], 1000);
    assert_eq!(json["cashout_enabled"], true);
    assert_eq!(json["cashout_min_credits"], 250);
}
