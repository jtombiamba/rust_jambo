use std::sync::Arc;

use actix_web::{web, HttpRequest, HttpResponse, Responder, ResponseError};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::config::Config;
use crate::error::AppError;
use crate::game::service::GameServiceTrait;

#[derive(Debug, Deserialize)]
pub struct CreateBenchmarkGameRequest {
    pub user_ids: Vec<Uuid>,
    #[serde(default = "default_bet")]
    pub bet: i32,
}

fn default_bet() -> i32 {
    10
}

#[derive(Debug, Serialize)]
pub struct BenchmarkPlayerInfo {
    pub player_id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub position: i32,
    pub cards: Vec<i32>,
}

#[derive(Debug, Serialize)]
pub struct CreateBenchmarkGameResponse {
    pub game_id: Uuid,
    pub players: Vec<BenchmarkPlayerInfo>,
    pub current_turn_position: i32,
    pub bet: i32,
}

#[derive(Debug, Serialize)]
pub struct CleanupResponse {
    pub success: bool,
    pub users_deleted: u64,
    pub games_deleted: u64,
    pub game_cards_deleted: u64,
    pub players_deleted: u64,
    pub player_profiles_deleted: u64,
    pub game_invites_deleted: u64,
}

fn validate_benchmark_token(req: &HttpRequest, config: &Config) -> Option<HttpResponse> {
    if config.benchmark_api_token.is_empty() {
        return None;
    }
    let token = req
        .headers()
        .get("X-Benchmark-Token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if token != config.benchmark_api_token {
        Some(HttpResponse::Unauthorized().json(serde_json::json!({
            "error": "Invalid benchmark token"
        })))
    } else {
        None
    }
}

pub async fn create_benchmark_game(
    req: HttpRequest,
    config: web::Data<Config>,
    orchestrator: web::Data<Arc<dyn GameServiceTrait>>,
    payload: web::Json<CreateBenchmarkGameRequest>,
) -> impl Responder {
    if let Some(error_response) = validate_benchmark_token(&req, config.get_ref()) {
        return error_response;
    }

    tracing::debug!(
        "create_benchmark_game called with {} user_ids, bet={}",
        payload.user_ids.len(),
        payload.bet
    );

    match orchestrator
        .create_benchmark_multiplayer_game(payload.user_ids.clone(), payload.bet)
        .await
    {
        Ok(outcome) => {
            tracing::debug!(
                "Benchmark game {} created successfully with {} players",
                outcome.game_id,
                outcome.players.len()
            );
            let players: Vec<BenchmarkPlayerInfo> = outcome
                .players
                .into_iter()
                .map(|p| BenchmarkPlayerInfo {
                    player_id: p.player_id,
                    user_id: p.user_id,
                    name: p.name,
                    position: p.position,
                    cards: p.cards,
                })
                .collect();
            let response = CreateBenchmarkGameResponse {
                game_id: outcome.game_id,
                players,
                current_turn_position: outcome.current_turn,
                bet: outcome.bet,
            };
            HttpResponse::Created().json(response)
        }
        Err(e) => {
            tracing::error!("Benchmark game creation failed: {:?}", e);
            let app_err = AppError::from(e);
            let status = app_err.status_code();
            let body = serde_json::json!({
                "success": false,
                "error": app_err.to_string(),
            });
            HttpResponse::build(status).json(body)
        }
    }
}

pub async fn cleanup_benchmark_data(
    req: HttpRequest,
    config: web::Data<Config>,
    orchestrator: web::Data<Arc<dyn GameServiceTrait>>,
) -> impl Responder {
    if let Some(error_response) = validate_benchmark_token(&req, config.get_ref()) {
        return error_response;
    }

    match orchestrator.cleanup_benchmark_data().await {
        Ok(counts) => HttpResponse::Ok().json(CleanupResponse {
            success: true,
            users_deleted: counts.users_deleted,
            games_deleted: counts.games_deleted,
            game_cards_deleted: counts.game_cards_deleted,
            players_deleted: counts.players_deleted,
            player_profiles_deleted: counts.player_profiles_deleted,
            game_invites_deleted: counts.game_invites_deleted,
        }),
        Err(e) => AppError::from(e).error_response(),
    }
}
