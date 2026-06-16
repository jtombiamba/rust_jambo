use std::sync::Arc;

use actix_web::{post, web, HttpMessage, HttpRequest, HttpResponse, Responder, ResponseError};

use crate::api::dto::responses::QuickGameResponse;
use crate::auth::config::AuthConfig;
use crate::auth::jwt;
use crate::error::AppError;
use crate::game::orchestrator::GameOrchestratorTrait;
use crate::messaging::RedisClient;
use crate::observability::CorrelationId;

/// TTL for one-time game tokens in seconds (2 hours).
const GAME_TOKEN_TTL_SECS: u64 = 7200;

#[post("/quickie")]
pub async fn create_quick_game(
    req: HttpRequest,
    orchestrator: web::Data<Arc<dyn GameOrchestratorTrait>>,
    auth_config: web::Data<AuthConfig>,
    redis: web::Data<Option<RedisClient>>,
) -> impl Responder {
    let correlation_id = req.extensions().get::<CorrelationId>().copied();

    // Check if the user already has a valid auth cookie
    let token = req.cookie("Authorization").map(|c| c.value().to_string());
    let is_authenticated = token
        .as_ref()
        .and_then(|t| jwt::validate_token(t, &auth_config).ok())
        .is_some();

    match orchestrator.create_quick_game(correlation_id).await {
        Ok(mut outcome) => {
            // If not authenticated, generate a one-time game token for WebSocket auth
            if !is_authenticated {
                match jwt::generate_game_token(outcome.game_id, &auth_config, GAME_TOKEN_TTL_SECS) {
                    Ok((game_token, claims)) => {
                        let redis_key = format!("ws_token:{}:{}", outcome.game_id, claims.jti);
                        if let Some(redis_client) = redis.get_ref().as_ref() {
                            let mut client = redis_client.clone();
                            if let Err(e) = client
                                .set_ex(&redis_key, &game_token, GAME_TOKEN_TTL_SECS)
                                .await
                            {
                                tracing::error!(
                                    "Failed to store game token in Redis for game {}: {}",
                                    outcome.game_id,
                                    e
                                );
                            }
                        }

                        outcome.ws_token = Some(game_token);
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to generate game token for quickie {}: {}",
                            outcome.game_id,
                            e
                        );
                    }
                }
            }

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
    use crate::auth::config::AuthConfig;
    use crate::error::GameError;
    use crate::game::orchestrator::{
        mock::MockGameOrchestrator, PlayCardOutcome, QuickGameOutcome,
    };
    use actix_web::{test, web, App};
    use std::sync::Arc;
    use uuid::Uuid;

    fn test_auth_config() -> AuthConfig {
        AuthConfig {
            jwt_secret: "test-secret-key-for-testing-only-1234567890".to_string(),
            jwt_expiry_hours: 24,
            ip_hash_pepper: "test-pepper".to_string(),
            frontend_url: "http://localhost:5173".to_string(),
        }
    }

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
                .app_data(web::Data::new(test_auth_config()))
                .app_data(web::Data::new(None::<RedisClient>))
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
                ws_token: None,
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
