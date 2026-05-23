use actix_web::{get, web, HttpResponse, Responder};

use crate::api::dto::config::ClientConfigResponse;
use crate::config::Config;

#[get("/config")]
pub async fn client_config(config: web::Data<Config>) -> impl Responder {
    let response = ClientConfigResponse {
        paypal_donate_url: config.paypal_donate_url.clone(),
    };
    HttpResponse::Ok().json(response)
}
