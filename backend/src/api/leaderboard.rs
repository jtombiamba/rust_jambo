use actix_web::{web, HttpResponse, ResponseError};
use std::sync::Arc;

use crate::auth::extractors::AuthenticatedUser;
use crate::cache::leaderboard;
use crate::cache::UserCache;
use crate::error::AppError;
use crate::i18n::I18n;
use crate::messaging::RedisClient;

pub async fn get_leaderboard(
    auth_user: AuthenticatedUser,
    redis_client: web::Data<Option<RedisClient>>,
    user_cache: web::Data<Arc<UserCache>>,
    i18n: I18n,
) -> HttpResponse {
    let redis = match redis_client.get_ref().clone() {
        Some(r) => r,
        None => {
            return AppError::Internal(i18n.t("leaderboard.redis_unavailable")).error_response();
        }
    };

    match leaderboard::get_leaderboard(redis, auth_user.user_id, &user_cache).await {
        Some(response) => HttpResponse::Ok().json(response),
        None => AppError::Internal(i18n.t("leaderboard.fetch_failed")).error_response(),
    }
}
