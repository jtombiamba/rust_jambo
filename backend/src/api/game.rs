use std::sync::Arc;

use actix_web::{post, web, HttpMessage, HttpRequest, HttpResponse, Responder, ResponseError};
use uuid::Uuid;

use crate::api::dto::requests::PlayCardRequest;
use crate::api::dto::responses::{
    AdvanceBotResponse, EvaluateRoundResponse, PlayCardResponse, PlayerActionRequest,
};
use crate::auth::config::AuthConfig;
use crate::error::AppError;
use crate::game::orchestrator::GameOrchestratorTrait;
use crate::messaging::RedisClient;
use crate::observability::CorrelationId;

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

    let idempotency_key = req
        .headers()
        .get("X-Idempotency-Key")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    match orchestrator
        .play_card(
            game_id,
            payload.player_id,
            payload.card_index,
            correlation_id,
            idempotency_key,
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

pub async fn advance_bot(
    req: HttpRequest,
    orchestrator: web::Data<Arc<dyn GameOrchestratorTrait>>,
    id: web::Path<Uuid>,
    payload: web::Json<PlayerActionRequest>,
) -> impl Responder {
    let game_id = id.into_inner();
    let auth_config = req.app_data::<web::Data<AuthConfig>>().cloned();
    let redis_client = req
        .app_data::<web::Data<Option<RedisClient>>>()
        .cloned()
        .and_then(|r| r.get_ref().clone());

    let token = req.cookie("Authorization").map(|c| c.value().to_string());
    let has_cookie = token.is_some();
    let mut auth_user_id =
        crate::websocket::validate_ws_token(token, auth_config.clone(), redis_client.clone()).await;

    tracing::debug!(
        "[advance_bot] game_id={}, auth_user_id={:?}, has_cookie={}",
        game_id,
        auth_user_id,
        has_cookie
    );

    // Track whether auth came from a game token (anonymous) vs a real user session
    let mut is_game_token_auth = false;

    if auth_user_id.is_none() {
        let game_token = req
            .query_string()
            .split('&')
            .filter_map(|pair| pair.split_once('='))
            .find(|(k, _)| *k == "token")
            .map(|(_, v)| v.to_string());
        tracing::debug!(
            "[advance_bot] game_token in query: {:?}",
            game_token.as_ref().map(|_| "present")
        );
        if let Some(ref gt) = game_token {
            auth_user_id =
                crate::websocket::validate_game_token(gt, game_id, auth_config, redis_client).await;
            tracing::debug!(
                "[advance_bot] validate_game_token returned: {:?}",
                auth_user_id
            );
            if auth_user_id.is_some() {
                is_game_token_auth = true;
            }
        }
    }

    tracing::debug!(
        "[advance_bot] is_game_token_auth={}, final auth_user_id={:?}",
        is_game_token_auth,
        auth_user_id
    );

    // Validate that the authenticated user (or token holder) owns the player_id.
    // For game token auth (anonymous), the returned "user_id" is the game_id, which
    // won't match any player's user_id. Skip the ownership check in that case —
    // possession of a valid game token for this game is sufficient authorization.
    if let Some(user_id) = auth_user_id {
        if !is_game_token_auth {
            // Real user session — verify ownership
            match orchestrator
                .verify_player_ownership(game_id, payload.player_id, user_id)
                .await
            {
                Ok(false) => {
                    tracing::warn!(
                        "[advance_bot] ownership check failed for user_id={}, player_id={}",
                        user_id,
                        payload.player_id
                    );
                    return AppError::from(crate::error::GameError::NotYourTurn).error_response();
                }
                Err(_) => {
                    // User ID not found in game — reject
                    return AppError::from(crate::error::GameError::NotYourTurn).error_response();
                }
                Ok(true) => {} // ownership confirmed
            }
        } else {
            tracing::debug!(
                "[advance_bot] game token auth — skipping ownership check for player_id={}",
                payload.player_id
            );
        }
        // Game token auth: skip ownership check, the token itself is proof of authorization
    }

    match orchestrator.advance_bot(game_id, payload.player_id).await {
        Ok(outcome) => {
            let response = AdvanceBotResponse {
                success: true,
                card_played: outcome.card_played,
                next_player_id: outcome.next_player_id,
                next_is_bot: outcome.next_is_bot,
                round_complete: outcome.round_complete,
                game_ended: outcome.game_ended,
            };
            HttpResponse::Ok().json(response)
        }
        Err(e) => AppError::from(e).error_response(),
    }
}

pub async fn evaluate_round(
    req: HttpRequest,
    orchestrator: web::Data<Arc<dyn GameOrchestratorTrait>>,
    id: web::Path<Uuid>,
    payload: web::Json<PlayerActionRequest>,
) -> impl Responder {
    let game_id = id.into_inner();
    let auth_config = req.app_data::<web::Data<AuthConfig>>().cloned();
    let redis_client = req
        .app_data::<web::Data<Option<RedisClient>>>()
        .cloned()
        .and_then(|r| r.get_ref().clone());

    let token = req.cookie("Authorization").map(|c| c.value().to_string());
    let has_cookie = token.is_some();
    let mut auth_user_id =
        crate::websocket::validate_ws_token(token, auth_config.clone(), redis_client.clone()).await;

    tracing::debug!(
        "[evaluate_round] game_id={}, auth_user_id={:?}, has_cookie={}",
        game_id,
        auth_user_id,
        has_cookie
    );

    // Track whether auth came from a game token (anonymous) vs a real user session
    let mut is_game_token_auth = false;

    if auth_user_id.is_none() {
        let game_token = req
            .query_string()
            .split('&')
            .filter_map(|pair| pair.split_once('='))
            .find(|(k, _)| *k == "token")
            .map(|(_, v)| v.to_string());
        tracing::debug!(
            "[evaluate_round] game_token in query: {:?}",
            game_token.as_ref().map(|_| "present")
        );
        if let Some(ref gt) = game_token {
            auth_user_id =
                crate::websocket::validate_game_token(gt, game_id, auth_config, redis_client).await;
            tracing::debug!(
                "[evaluate_round] validate_game_token returned: {:?}",
                auth_user_id
            );
            if auth_user_id.is_some() {
                is_game_token_auth = true;
            }
        }
    }

    tracing::debug!(
        "[evaluate_round] is_game_token_auth={}, final auth_user_id={:?}",
        is_game_token_auth,
        auth_user_id
    );

    // Idempotency guard: evaluate-round has financial side effects (credit transfers)
    let idempotency_key = req
        .headers()
        .get("X-Idempotency-Key")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    // Validate that the authenticated user (or token holder) owns the player_id.
    // For game token auth (anonymous), the returned "user_id" is the game_id, which
    // won't match any player's user_id. Skip the ownership check in that case —
    // possession of a valid game token for this game is sufficient authorization.
    if let Some(user_id) = auth_user_id {
        if !is_game_token_auth {
            // Real user session — verify ownership
            match orchestrator
                .verify_player_ownership(game_id, payload.player_id, user_id)
                .await
            {
                Ok(false) | Err(_) => {
                    return AppError::from(crate::error::GameError::NotYourTurn).error_response();
                }
                Ok(true) => {}
            }
        } else {
            tracing::debug!(
                "[evaluate_round] game token auth — skipping ownership check for player_id={}",
                payload.player_id
            );
        }
        // Game token auth: skip ownership check, the token itself is proof of authorization
    }

    match orchestrator
        .evaluate_round(game_id, payload.player_id, idempotency_key)
        .await
    {
        Ok(outcome) => {
            let response = EvaluateRoundResponse {
                success: true,
                round_number: outcome.round_number,
                winner_id: outcome.winner_id.unwrap_or_default(),
                winner_position: outcome.winner_position,
                game_ended: outcome.game_ended,
            };
            HttpResponse::Ok().json(response)
        }
        Err(e) => AppError::from(e).error_response(),
    }
}

#[cfg(test)]
#[path = "game_tests.rs"]
mod game_tests;
