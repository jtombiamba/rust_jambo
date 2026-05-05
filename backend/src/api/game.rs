use std::sync::Arc;

use actix_web::{get, post, web, HttpMessage, HttpRequest, HttpResponse, Responder, ResponseError};
use serde_json::json;
use uuid::Uuid;

use crate::api::dto::requests::PlayCardRequest;
use crate::api::dto::responses::{GameListItem, PlayCardResponse};
use crate::error::AppError;
use crate::game::orchestrator::GameOrchestratorTrait;
use crate::observability::CorrelationId;

#[get("/games")]
pub async fn list_games() -> impl Responder {
    let games = vec![GameListItem {
        id: Uuid::new_v4(),
        status: "active".to_string(),
        bet: 10,
    }];
    HttpResponse::Ok().json(games)
}

#[get("/games/{id}/me")]
pub async fn get_my_cards(_id: web::Path<Uuid>) -> impl Responder {
    let cards = vec![1, 5, 9, 13, 17];
    HttpResponse::Ok().json(cards)
}

#[post("/game/{id}/play")]
pub async fn play_card(
    req: HttpRequest,
    orchestrator: web::Data<Arc<dyn GameOrchestratorTrait>>,
    id: web::Path<Uuid>,
    payload: web::Json<PlayCardRequest>,
) -> impl Responder {
    let game_id = id.into_inner();
    let correlation_id = req.extensions().get::<CorrelationId>().copied();

    if let Err(e) = payload.validate() {
        return AppError::from(e).error_response();
    }

    match orchestrator
        .play_card(
            game_id,
            payload.player_id,
            payload.card_index,
            correlation_id,
        )
        .await
    {
        Ok(outcome) => {
            let response: PlayCardResponse = outcome.into();
            HttpResponse::Ok().json(response)
        }
        Err(e) => AppError::from(e).error_response(),
    }
}

#[post("/games/{id}/start")]
pub async fn start_game(_id: web::Path<Uuid>) -> impl Responder {
    HttpResponse::Ok().json(json!({ "status": "started" }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::GameError;
    use crate::game::orchestrator::mock::MockGameOrchestrator;
    use crate::game::orchestrator::QuickGameOutcome;
    use actix_web::{test, web, App};
    use std::sync::Arc;

    async fn make_app(
        mock: Arc<dyn GameOrchestratorTrait>,
    ) -> impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    > {
        test::init_service(App::new().app_data(web::Data::new(mock)).service(play_card)).await
    }

    // ── validation tests ──

    #[actix_web::test]
    async fn play_card_valid_payload() {
        let mock = Arc::new(MockGameOrchestrator::ok());
        let app = make_app(mock).await;
        let game_id = Uuid::new_v4();
        let player_id = Uuid::new_v4();
        let payload = serde_json::json!({ "player_id": player_id, "card_index": 15 });

        let req = test::TestRequest::post()
            .uri(&format!("/game/{}/play", game_id))
            .set_json(&payload)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["success"], true);
        assert_eq!(body["message"], "Card played successfully");
    }

    #[actix_web::test]
    async fn play_card_negative_index() {
        let mock = Arc::new(MockGameOrchestrator::ok());
        let app = make_app(mock).await;
        let game_id = Uuid::new_v4();
        let player_id = Uuid::new_v4();
        let payload = serde_json::json!({ "player_id": player_id, "card_index": -1 });

        let req = test::TestRequest::post()
            .uri(&format!("/game/{}/play", game_id))
            .set_json(&payload)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400);
        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["success"], false);
        assert!(body["error"]
            .as_str()
            .unwrap()
            .contains("out of valid range"));
    }

    #[actix_web::test]
    async fn play_card_index_out_of_range() {
        let mock = Arc::new(MockGameOrchestrator::ok());
        let app = make_app(mock).await;
        let game_id = Uuid::new_v4();
        let player_id = Uuid::new_v4();
        let payload = serde_json::json!({ "player_id": player_id, "card_index": 32 });

        let req = test::TestRequest::post()
            .uri(&format!("/game/{}/play", game_id))
            .set_json(&payload)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400);
        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["success"], false);
    }

    #[actix_web::test]
    async fn play_card_missing_player_id() {
        let mock = Arc::new(MockGameOrchestrator::ok());
        let app = make_app(mock).await;
        let game_id = Uuid::new_v4();
        let payload = serde_json::json!({ "card_index": 0 });

        let req = test::TestRequest::post()
            .uri(&format!("/game/{}/play", game_id))
            .set_json(&payload)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400);
    }

    #[actix_web::test]
    async fn play_card_missing_card_index() {
        let mock = Arc::new(MockGameOrchestrator::ok());
        let app = make_app(mock).await;
        let game_id = Uuid::new_v4();
        let player_id = Uuid::new_v4();
        let payload = serde_json::json!({ "player_id": player_id });

        let req = test::TestRequest::post()
            .uri(&format!("/game/{}/play", game_id))
            .set_json(&payload)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400);
    }

    #[actix_web::test]
    async fn play_card_invalid_player_id_uuid() {
        let mock = Arc::new(MockGameOrchestrator::ok());
        let app = make_app(mock).await;
        let game_id = Uuid::new_v4();
        let payload = serde_json::json!({ "player_id": "not-a-uuid", "card_index": 0 });

        let req = test::TestRequest::post()
            .uri(&format!("/game/{}/play", game_id))
            .set_json(&payload)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400);
    }

    #[actix_web::test]
    async fn play_card_non_uuid_path() {
        let mock = Arc::new(MockGameOrchestrator::ok());
        let app = make_app(mock).await;
        let player_id = Uuid::new_v4();
        let payload = serde_json::json!({ "player_id": player_id, "card_index": 0 });

        let req = test::TestRequest::post()
            .uri("/game/not-a-uuid/play")
            .set_json(&payload)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 404);
    }

    // ── error mapping tests ──

    #[actix_web::test]
    async fn play_card_game_not_found() {
        let mock = Arc::new(MockGameOrchestrator::new(
            Err(GameError::GameNotFound),
            Ok(QuickGameOutcome {
                game_id: Uuid::new_v4(),
                players: vec![],
                status: "active".into(),
                current_turn: 0,
                bet: 10,
            }),
        ));
        let app = make_app(mock).await;
        let game_id = Uuid::new_v4();
        let player_id = Uuid::new_v4();
        let req = test::TestRequest::post()
            .uri(&format!("/game/{}/play", game_id))
            .set_json(serde_json::json!({ "player_id": player_id, "card_index": 0 }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 404);
    }

    #[actix_web::test]
    async fn play_card_player_not_found() {
        let mock = Arc::new(MockGameOrchestrator::new(
            Err(GameError::PlayerNotFound),
            Ok(QuickGameOutcome {
                game_id: Uuid::new_v4(),
                players: vec![],
                status: "active".into(),
                current_turn: 0,
                bet: 10,
            }),
        ));
        let app = make_app(mock).await;
        let game_id = Uuid::new_v4();
        let player_id = Uuid::new_v4();
        let req = test::TestRequest::post()
            .uri(&format!("/game/{}/play", game_id))
            .set_json(serde_json::json!({ "player_id": player_id, "card_index": 0 }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 404);
    }

    #[actix_web::test]
    async fn play_card_card_not_found() {
        let mock = Arc::new(MockGameOrchestrator::new(
            Err(GameError::CardNotFound),
            Ok(QuickGameOutcome {
                game_id: Uuid::new_v4(),
                players: vec![],
                status: "active".into(),
                current_turn: 0,
                bet: 10,
            }),
        ));
        let app = make_app(mock).await;
        let game_id = Uuid::new_v4();
        let player_id = Uuid::new_v4();
        let req = test::TestRequest::post()
            .uri(&format!("/game/{}/play", game_id))
            .set_json(serde_json::json!({ "player_id": player_id, "card_index": 0 }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 404);
    }

    #[actix_web::test]
    async fn play_card_not_your_turn() {
        let mock = Arc::new(MockGameOrchestrator::new(
            Err(GameError::NotYourTurn),
            Ok(QuickGameOutcome {
                game_id: Uuid::new_v4(),
                players: vec![],
                status: "active".into(),
                current_turn: 0,
                bet: 10,
            }),
        ));
        let app = make_app(mock).await;
        let game_id = Uuid::new_v4();
        let player_id = Uuid::new_v4();
        let req = test::TestRequest::post()
            .uri(&format!("/game/{}/play", game_id))
            .set_json(serde_json::json!({ "player_id": player_id, "card_index": 0 }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 403);
    }

    #[actix_web::test]
    async fn play_card_invalid_card() {
        let mock = Arc::new(MockGameOrchestrator::new(
            Err(GameError::InvalidCard),
            Ok(QuickGameOutcome {
                game_id: Uuid::new_v4(),
                players: vec![],
                status: "active".into(),
                current_turn: 0,
                bet: 10,
            }),
        ));
        let app = make_app(mock).await;
        let game_id = Uuid::new_v4();
        let player_id = Uuid::new_v4();
        let req = test::TestRequest::post()
            .uri(&format!("/game/{}/play", game_id))
            .set_json(serde_json::json!({ "player_id": player_id, "card_index": 0 }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 403);
    }

    #[actix_web::test]
    async fn play_card_game_finished() {
        let mock = Arc::new(MockGameOrchestrator::new(
            Err(GameError::GameFinished),
            Ok(QuickGameOutcome {
                game_id: Uuid::new_v4(),
                players: vec![],
                status: "active".into(),
                current_turn: 0,
                bet: 10,
            }),
        ));
        let app = make_app(mock).await;
        let game_id = Uuid::new_v4();
        let player_id = Uuid::new_v4();
        let req = test::TestRequest::post()
            .uri(&format!("/game/{}/play", game_id))
            .set_json(serde_json::json!({ "player_id": player_id, "card_index": 0 }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 409);
    }

    #[actix_web::test]
    async fn play_card_round_not_complete() {
        let mock = Arc::new(MockGameOrchestrator::new(
            Err(GameError::RoundNotComplete),
            Ok(QuickGameOutcome {
                game_id: Uuid::new_v4(),
                players: vec![],
                status: "active".into(),
                current_turn: 0,
                bet: 10,
            }),
        ));
        let app = make_app(mock).await;
        let game_id = Uuid::new_v4();
        let player_id = Uuid::new_v4();
        let req = test::TestRequest::post()
            .uri(&format!("/game/{}/play", game_id))
            .set_json(serde_json::json!({ "player_id": player_id, "card_index": 0 }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400);
    }

    #[actix_web::test]
    async fn play_card_database_error() {
        let mock = Arc::new(MockGameOrchestrator::new(
            Err(GameError::Database(sea_orm::DbErr::Custom(
                "db down".into(),
            ))),
            Ok(QuickGameOutcome {
                game_id: Uuid::new_v4(),
                players: vec![],
                status: "active".into(),
                current_turn: 0,
                bet: 10,
            }),
        ));
        let app = make_app(mock).await;
        let game_id = Uuid::new_v4();
        let player_id = Uuid::new_v4();
        let req = test::TestRequest::post()
            .uri(&format!("/game/{}/play", game_id))
            .set_json(serde_json::json!({ "player_id": player_id, "card_index": 0 }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 500);
    }

    #[actix_web::test]
    async fn play_card_internal_error() {
        let mock = Arc::new(MockGameOrchestrator::new(
            Err(GameError::Internal(Box::new(std::io::Error::other(
                "internal kaboom",
            )))),
            Ok(QuickGameOutcome {
                game_id: Uuid::new_v4(),
                players: vec![],
                status: "active".into(),
                current_turn: 0,
                bet: 10,
            }),
        ));
        let app = make_app(mock).await;
        let game_id = Uuid::new_v4();
        let player_id = Uuid::new_v4();
        let req = test::TestRequest::post()
            .uri(&format!("/game/{}/play", game_id))
            .set_json(serde_json::json!({ "player_id": player_id, "card_index": 0 }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 500);
    }

    // ── stub endpoint tests ──

    #[actix_web::test]
    async fn test_list_games() {
        let app = test::init_service(App::new().service(list_games)).await;
        let req = test::TestRequest::get().uri("/games").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = test::read_body_json(resp).await;
        assert!(body.is_array());
        assert_eq!(body[0]["status"], "active");
        assert_eq!(body[0]["bet"], 10);
    }

    #[actix_web::test]
    async fn test_get_my_cards() {
        let app = test::init_service(App::new().service(get_my_cards)).await;
        let game_id = Uuid::new_v4();
        let req = test::TestRequest::get()
            .uri(&format!("/games/{}/me", game_id))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = test::read_body_json(resp).await;
        assert!(body.is_array());
        assert_eq!(body.as_array().unwrap().len(), 5);
    }

    #[actix_web::test]
    async fn test_get_my_cards_non_uuid_path() {
        let app = test::init_service(App::new().service(get_my_cards)).await;
        let req = test::TestRequest::get()
            .uri("/games/not-a-uuid/me")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 404);
    }

    #[actix_web::test]
    async fn test_start_game() {
        let app = test::init_service(App::new().service(start_game)).await;
        let game_id = Uuid::new_v4();
        let req = test::TestRequest::post()
            .uri(&format!("/games/{}/start", game_id))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["status"], "started");
    }
}
