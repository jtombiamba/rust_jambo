use actix_web::{get, web, HttpResponse, Responder};

use crate::api::dto::config::ClientConfigResponse;
use crate::config::Config;
use crate::game::constants::{BOT_THINKING_DELAY_MS, ROUND_PAUSE_DELAY_MS};

#[get("/config")]
pub async fn client_config(config: web::Data<Config>) -> impl Responder {
    let response = ClientConfigResponse {
        paypal_donate_url: config.paypal_donate_url.clone(),
        bot_thinking_delay_ms: *BOT_THINKING_DELAY_MS,
        round_pause_delay_ms: *ROUND_PAUSE_DELAY_MS,
    };
    HttpResponse::Ok().json(response)
}
