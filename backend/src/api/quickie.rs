use std::sync::Arc;

use actix_web::{post, web, HttpMessage, HttpRequest, HttpResponse, Responder, ResponseError};

use crate::api::dto::responses::{ApiErrorResponse, QuickGameResponse};
use crate::auth::config::AuthConfig;
use crate::auth::jwt;
use crate::error::AppError;
use crate::game::service::GameLifecycleService;
use crate::messaging::RedisClient;
use crate::observability::CorrelationId;

/// TTL for one-time game tokens in seconds (2 hours).
const GAME_TOKEN_TTL_SECS: u64 = 7200;

#[utoipa::path(
    post,
    path = "/api/quickie",
    tag = "game",
    params(("step_by_step" = Option<bool>, Query, description = "Enable step-by-step mode")),
    responses(
        (status = 201, description = "Quick game created", body = QuickGameResponse),
        (status = 400, description = "Invalid request", body = ApiErrorResponse),
    )
)]
#[post("/quickie")]
pub async fn create_quick_game(
    req: HttpRequest,
    orchestrator: web::Data<Arc<dyn GameLifecycleService>>,
    auth_config: web::Data<AuthConfig>,
    redis: web::Data<Option<RedisClient>>,
) -> impl Responder {
    let correlation_id = req.extensions().get::<CorrelationId>().copied();

    let step_by_step = req.query_string().contains("step_by_step=true");

    // Check if the user already has a valid auth cookie
    let token = req.cookie("Authorization").map(|c| c.value().to_string());
    let is_authenticated = token
        .as_ref()
        .and_then(|t| jwt::validate_token(t, &auth_config).ok())
        .is_some();

    match orchestrator
        .create_quick_game(correlation_id, step_by_step)
        .await
    {
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
