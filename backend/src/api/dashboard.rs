use std::sync::Arc;

use actix_web::{web, HttpResponse};
use uuid::Uuid;

use crate::api::dto::dashboard::PaginationParams;
use crate::api::services::dashboard_service::DashboardService;
use crate::auth::extractors::AuthenticatedUser;
use crate::database::repositories::DashboardRepository;

pub type DashboardServiceType = DashboardService<DashboardRepository>;

pub async fn get_profile(
    auth_user: AuthenticatedUser,
    service: web::Data<Arc<DashboardServiceType>>,
) -> HttpResponse {
    service
        .get_profile(auth_user.user_id)
        .await
        .unwrap_or_else(|e| {
            HttpResponse::InternalServerError().json(serde_json::json!({"error": e.to_string()}))
        })
}

pub async fn list_games(
    auth_user: AuthenticatedUser,
    query: web::Query<PaginationParams>,
    service: web::Data<Arc<DashboardServiceType>>,
) -> HttpResponse {
    service
        .list_games(auth_user.user_id, query.into_inner())
        .await
        .unwrap_or_else(|e| {
            HttpResponse::InternalServerError().json(serde_json::json!({"error": e.to_string()}))
        })
}

pub async fn get_game(
    auth_user: AuthenticatedUser,
    service: web::Data<Arc<DashboardServiceType>>,
    path: web::Path<Uuid>,
) -> HttpResponse {
    service
        .get_game(auth_user.user_id, path.into_inner())
        .await
        .unwrap_or_else(|e| {
            HttpResponse::InternalServerError().json(serde_json::json!({"error": e.to_string()}))
        })
}

pub async fn get_active_game(
    auth_user: AuthenticatedUser,
    service: web::Data<Arc<DashboardServiceType>>,
) -> HttpResponse {
    service
        .get_active_game(auth_user.user_id)
        .await
        .unwrap_or_else(|e| {
            HttpResponse::InternalServerError().json(serde_json::json!({"error": e.to_string()}))
        })
}

pub async fn create_game(
    auth_user: AuthenticatedUser,
    orchestrator: web::Data<std::sync::Arc<dyn crate::game::orchestrator::GameOrchestratorTrait>>,
    db: web::Data<sea_orm::DatabaseConnection>,
) -> HttpResponse {
    let result = orchestrator
        .create_quick_game_for_user(auth_user.user_id, db.get_ref())
        .await;

    match result {
        Ok(outcome) => {
            let response: crate::api::dto::responses::QuickGameResponse = outcome.into();
            HttpResponse::Created().json(response)
        }
        Err(e) => {
            tracing::error!("Failed to create game for user: {}", e);
            HttpResponse::InternalServerError()
                .json(serde_json::json!({"error": "Failed to create game"}))
        }
    }
}
