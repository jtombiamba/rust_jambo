use std::sync::Arc;

use actix_web::{post, web, HttpMessage, HttpRequest, HttpResponse, Responder, ResponseError};

use crate::api::dto::responses::QuickGameResponse;
use crate::error::AppError;
use crate::game::orchestrator::GameOrchestratorTrait;
use crate::observability::CorrelationId;

#[post("/quickie")]
pub async fn create_quick_game(
    req: HttpRequest,
    orchestrator: web::Data<Arc<dyn GameOrchestratorTrait>>,
) -> impl Responder {
    let correlation_id = req.extensions().get::<CorrelationId>().copied();

    match orchestrator.create_quick_game(correlation_id).await {
        Ok(outcome) => {
            let response: QuickGameResponse = outcome.into();
            let json_str = serde_json::to_string(&response).unwrap_or_default();
            tracing::debug!("[DEBUG] QuickGameResponse JSON: {}", json_str);
            HttpResponse::Created().json(response)
        }
        Err(e) => AppError::from(e).error_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::GameError;
    use crate::game::orchestrator::{
        mock::MockGameOrchestrator, PlayCardOutcome, QuickGameOutcome,
    };
    use actix_web::{test, web, App};
    use std::sync::Arc;
    use uuid::Uuid;

    async fn make_app(
        mock: Arc<dyn GameOrchestratorTrait>,
    ) -> impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    > {
        test::init_service(
            App::new()
                .app_data(web::Data::new(mock))
                .service(create_quick_game),
        )
        .await
    }

    #[actix_web::test]
    async fn create_quick_game_success() {
        let game_id = Uuid::new_v4();
        let mock = Arc::new(MockGameOrchestrator::new(
            Ok(PlayCardOutcome {
                card_id: Uuid::new_v4(),
                next_turn: Some(Uuid::new_v4()),
                game_ended: false,
                round_completed: false,
                current_round: 1,
            }),
            Ok(QuickGameOutcome {
                game_id,
                players: vec![],
                status: "active".into(),
                current_turn: 2,
                bet: 10,
                max_players: 4,
                invite_expires_at: None,
                deck_slots: None,
            }),
        ));
        let app = make_app(mock).await;

        let req = test::TestRequest::post().uri("/quickie").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["game_id"], game_id.to_string());
        assert_eq!(body["status"], "active");
        assert_eq!(body["bet"], 10);
    }

    #[actix_web::test]
    async fn create_quick_game_error() {
        let mock = Arc::new(MockGameOrchestrator::new(
            Ok(PlayCardOutcome {
                card_id: Uuid::new_v4(),
                next_turn: None,
                game_ended: true,
                round_completed: false,
                current_round: 1,
            }),
            Err(GameError::Database(sea_orm::DbErr::Custom(
                "db error".into(),
            ))),
        ));
        let app = make_app(mock).await;

        let req = test::TestRequest::post().uri("/quickie").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 500);
        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["success"], false);
    }
}
