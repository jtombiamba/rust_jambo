use serde::Serialize;

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ClientConfigResponse {
    pub paypal_donate_url: String,
    pub bot_thinking_delay_ms: u64,
    pub round_pause_delay_ms: u64,
    pub cashout_enabled: bool,
    pub cashout_min_credits: i32,
    pub cashout_credits_per_eur: i32,
    pub cashout_max_eur_cents: i32,
}
