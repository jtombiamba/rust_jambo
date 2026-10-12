use actix_web::{get, HttpResponse};

use crate::api::dto::responses::AnonymousStatsResponse;

#[utoipa::path(
    get,
    path = "/api/anonymous",
    tag = "anonymous",
    responses((status = 200, description = "Anonymous gameplay stats", body = AnonymousStatsResponse))
)]
#[get("/anonymous")]
pub async fn get_anonymous_stats() -> HttpResponse {
    let stats = AnonymousStatsResponse {
        games_allowed: 10,
        games_played: 0,
        total_wins: 0,
        credits: 100,
    };
    let json_str = serde_json::to_string(&stats).unwrap_or_default();
    // tracing::debug!("[DEBUG] AnonymousStatsResponse JSON: {}", json_str);
    tracing::info!("[DEBUG] AnonymousStatsResponse JSON: {}", json_str);
    HttpResponse::Ok().json(stats)
}
