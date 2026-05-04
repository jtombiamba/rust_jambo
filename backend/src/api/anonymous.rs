use actix_web::{get, HttpResponse, Responder};

use crate::api::dto::responses::AnonymousStatsResponse;

#[get("/anonymous")]
pub async fn get_anonymous_stats() -> impl Responder {
    let stats = AnonymousStatsResponse {
        games_allowed: 10,
        games_played: 0,
        total_wins: 0,
        credits: 500,
    };
    let json_str = serde_json::to_string(&stats).unwrap_or_default();
    tracing::debug!("[DEBUG] AnonymousStatsResponse JSON: {}", json_str);
    HttpResponse::Ok().json(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, App};

    #[actix_web::test]
    async fn test_get_anonymous_stats() {
        let app = test::init_service(
            App::new().service(get_anonymous_stats),
        ).await;
        let req = test::TestRequest::get()
            .uri("/anonymous")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["games_allowed"], 10);
        assert_eq!(body["games_played"], 0);
        assert_eq!(body["total_wins"], 0);
        assert_eq!(body["credits"], 500);
    }
}
