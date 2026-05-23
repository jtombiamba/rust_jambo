use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ClientConfigResponse {
    pub paypal_donate_url: String,
}
