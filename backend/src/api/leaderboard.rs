use actix_web::{web, HttpResponse, ResponseError};
use std::sync::Arc;

use crate::api::dto::responses::ApiErrorResponse;
use crate::auth::extractors::AuthenticatedUser;
use crate::cache::leaderboard::{self, LeaderboardResponse};
use crate::cache::UserCache;
use crate::error::AppError;
use crate::messaging::RedisClient;

#[utoipa::path(
    get,
    path = "/api/me/leaderboard",
    tag = "leaderboard",
    security(("cookie_auth" = [])),
    responses(
        (status = 200, description = "Leaderboard", body = LeaderboardResponse),
        (status = 401, description = "Authentication required", body = ApiErrorResponse),
        (status = 500, description = "Leaderboard unavailable", body = ApiErrorResponse),
    )
)]
pub async fn get_leaderboard(
    auth_user: AuthenticatedUser,
    redis_client: web::Data<Option<RedisClient>>,
    user_cache: web::Data<Arc<UserCache>>,
) -> HttpResponse {
    let redis = match redis_client.get_ref().clone() {
        Some(r) => r,
        None => {
            return AppError::Internal("Redis not available for leaderboard".into())
                .error_response();
        }
    };

    match leaderboard::get_leaderboard(redis, auth_user.user_id, &user_cache).await {
        Some(response) => HttpResponse::Ok().json(response),
        None => AppError::Internal("Failed to fetch leaderboard".into()).error_response(),
    }
}
