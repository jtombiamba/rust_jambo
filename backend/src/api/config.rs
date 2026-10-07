use actix_web::{get, web, HttpResponse};

use crate::api::dto::config::ClientConfigResponse;
use crate::config::Config;
use crate::game::constants::{BOT_THINKING_DELAY_MS, ROUND_PAUSE_DELAY_MS};

#[utoipa::path(
    get,
    path = "/api/config",
    tag = "config",
    responses((status = 200, description = "Client configuration", body = ClientConfigResponse))
)]
#[get("/config")]
pub async fn client_config(config: web::Data<Config>) -> HttpResponse {
    let response = ClientConfigResponse {
        paypal_donate_url: config.paypal_donate_url.clone(),
        bot_thinking_delay_ms: *BOT_THINKING_DELAY_MS,
        round_pause_delay_ms: *ROUND_PAUSE_DELAY_MS,
        cashout_enabled: config.cashout_enabled,
        cashout_min_credits: config.cashout_min_credits,
        cashout_credits_per_eur: config.cashout_credits_per_eur,
        cashout_max_eur_cents: config.cashout_max_eur_cents,
    };
    HttpResponse::Ok().json(response)
}
