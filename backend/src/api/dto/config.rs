use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ClientConfigResponse {
    pub paypal_donate_url: String,
    pub bot_thinking_delay_ms: u64,
    pub round_pause_delay_ms: u64,
}
