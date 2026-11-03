use std::sync::Arc;

use actix_web::{web, HttpResponse, ResponseError};
use uuid::Uuid;

use crate::api::dto::responses::{
    ActiveRunResponse, ApiErrorResponse, CreateRunResponse, CurrentGameResponse, JoinRunResponse,
    RoomCreatedResponse, RoomDetailResponse, RoomListItem, RunListItem, SimpleSuccessResponse,
    SpectateTokenResponse, StartNextGameResponse,
};
use crate::auth::config::AuthConfig;
use crate::auth::extractors::AuthenticatedUser;
use crate::auth::jwt;
use jambo_core::error::AppError;
use jambo_domain::messaging::RedisClient;
use jambo_domain::room::service_trait::RoomServiceTrait;

/// TTL for run-scoped spectator tokens in seconds (6 hours).
const RUN_SPECTATE_TOKEN_TTL_SECS: u64 = 21600;

#[derive(serde::Deserialize, utoipa::ToSchema)]
pub struct CreateRoomRequest {
    pub name: String,
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
pub struct JoinRoomRequest {
    pub invitation_code: String,
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
pub struct CreateRunRequest {
    pub num_games: i32,
    pub bet: i32,
    pub player_ids: Vec<Uuid>,
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
pub struct InviteToRoomRequest {
    pub email: String,
}

macro_rules! service_response {
    ($result:expr) => {{
        match $result {
            Ok(data) => HttpResponse::Ok().json(data),
            Err(e) => e.error_response(),
        }
    }};
    ($result:expr, $status:expr) => {{
        match $result {
            Ok(data) => HttpResponse::build($status).json(data),
            Err(e) => e.error_response(),
        }
    }};
}

pub(crate) fn to_room_created(room: jambo_db::models::room::Model) -> RoomCreatedResponse {
    RoomCreatedResponse {
        id: room.id,
        name: room.name,
        creator_id: room.creator_id,
        invitation_code: room.invitation_code,
        created_at: room.created_at,
        updated_at: room.updated_at,
    }
}

#[utoipa::path(
    post,
    path = "/api/me/rooms",
    tag = "rooms",
    security(("cookie_auth" = [])),
    request_body = CreateRoomRequest,
    responses(
        (status = 201, description = "Room created", body = RoomCreatedResponse),
        (status = 401, description = "Authentication required", body = ApiErrorResponse),
    )
)]
pub async fn create_room(
    auth_user: AuthenticatedUser,
    body: web::Json<CreateRoomRequest>,
    service: web::Data<Arc<dyn RoomServiceTrait>>,
) -> HttpResponse {
    match service.create_room(auth_user.user_id, &body.name).await {
        Ok(room) => HttpResponse::Created().json(to_room_created(room)),
        Err(e) => e.error_response(),
    }
}

#[utoipa::path(
    get,
    path = "/api/me/rooms",
    tag = "rooms",
    security(("cookie_auth" = [])),
    responses(
        (status = 200, description = "Rooms for the current user", body = [RoomListItem]),
        (status = 401, description = "Authentication required", body = ApiErrorResponse),
    )
)]
pub async fn list_rooms(
    auth_user: AuthenticatedUser,
    service: web::Data<Arc<dyn RoomServiceTrait>>,
) -> HttpResponse {
    service_response!(service.list_user_rooms(auth_user.user_id).await)
}

#[utoipa::path(
    get,
    path = "/api/me/rooms/{room_id}",
    tag = "rooms",
    security(("cookie_auth" = [])),
    params(("room_id" = Uuid, Path, description = "Room ID")),
    responses(
        (status = 200, description = "Room details", body = RoomDetailResponse),
        (status = 401, description = "Authentication required", body = ApiErrorResponse),
        (status = 404, description = "Room not found", body = ApiErrorResponse),
    )
)]
pub async fn get_room(
    auth_user: AuthenticatedUser,
    path: web::Path<Uuid>,
    service: web::Data<Arc<dyn RoomServiceTrait>>,
) -> HttpResponse {
    service_response!(
        service
            .get_room_detail(path.into_inner(), auth_user.user_id)
            .await
    )
}

#[utoipa::path(
    post,
    path = "/api/me/rooms/join",
    tag = "rooms",
    security(("cookie_auth" = [])),
    request_body = JoinRoomRequest,
    responses(
        (status = 200, description = "Joined room", body = RoomCreatedResponse),
        (status = 401, description = "Authentication required", body = ApiErrorResponse),
        (status = 404, description = "Room not found", body = ApiErrorResponse),
    )
)]
pub async fn join_room(
    auth_user: AuthenticatedUser,
    body: web::Json<JoinRoomRequest>,
    service: web::Data<Arc<dyn RoomServiceTrait>>,
) -> HttpResponse {
    match service
        .join_room(auth_user.user_id, &body.invitation_code)
        .await
    {
        Ok(room) => HttpResponse::Ok().json(to_room_created(room)),
        Err(e) => e.error_response(),
    }
}

#[utoipa::path(
    post,
    path = "/api/me/rooms/{room_id}/invite",
    tag = "rooms",
    security(("cookie_auth" = [])),
    params(("room_id" = Uuid, Path, description = "Room ID")),
    request_body = InviteToRoomRequest,
    responses(
        (status = 200, description = "Invitation sent", body = SimpleSuccessResponse),
        (status = 401, description = "Authentication required", body = ApiErrorResponse),
        (status = 404, description = "Room not found", body = ApiErrorResponse),
    )
)]
pub async fn invite_to_room(
    auth_user: AuthenticatedUser,
    path: web::Path<Uuid>,
    body: web::Json<InviteToRoomRequest>,
    service: web::Data<Arc<dyn RoomServiceTrait>>,
) -> HttpResponse {
    match service
        .invite_to_room(path.into_inner(), auth_user.user_id, &body.email)
        .await
    {
        Ok(()) => HttpResponse::Ok().json(SimpleSuccessResponse { success: true }),
        Err(e) => e.error_response(),
    }
}

#[utoipa::path(
    post,
    path = "/api/me/rooms/{room_id}/leave",
    tag = "rooms",
    security(("cookie_auth" = [])),
    params(("room_id" = Uuid, Path, description = "Room ID")),
    responses(
        (status = 200, description = "Left room", body = SimpleSuccessResponse),
        (status = 401, description = "Authentication required", body = ApiErrorResponse),
    )
)]
pub async fn leave_room(
    auth_user: AuthenticatedUser,
    path: web::Path<Uuid>,
    service: web::Data<Arc<dyn RoomServiceTrait>>,
) -> HttpResponse {
    match service
        .leave_room(path.into_inner(), auth_user.user_id)
        .await
    {
        Ok(()) => HttpResponse::Ok().json(SimpleSuccessResponse { success: true }),
        Err(e) => e.error_response(),
    }
}

#[utoipa::path(
    post,
    path = "/api/me/rooms/{room_id}/runs",
    tag = "rooms",
    security(("cookie_auth" = [])),
    params(("room_id" = Uuid, Path, description = "Room ID")),
    request_body = CreateRunRequest,
    responses(
        (status = 201, description = "Run created", body = CreateRunResponse),
        (status = 401, description = "Authentication required", body = ApiErrorResponse),
        (status = 404, description = "Room not found", body = ApiErrorResponse),
    )
)]
pub async fn create_run(
    auth_user: AuthenticatedUser,
    path: web::Path<Uuid>,
    body: web::Json<CreateRunRequest>,
    service: web::Data<Arc<dyn RoomServiceTrait>>,
) -> HttpResponse {
    service_response!(
        service
            .create_run(
                path.into_inner(),
                auth_user.user_id,
                body.num_games,
                body.bet,
                &body.player_ids,
            )
            .await,
        actix_web::http::StatusCode::CREATED
    )
}

#[utoipa::path(
    post,
    path = "/api/me/runs/{run_id}/join",
    tag = "rooms",
    security(("cookie_auth" = [])),
    params(("run_id" = Uuid, Path, description = "Run ID")),
    responses(
        (status = 200, description = "Joined run", body = JoinRunResponse),
        (status = 401, description = "Authentication required", body = ApiErrorResponse),
        (status = 404, description = "Run not found", body = ApiErrorResponse),
    )
)]
pub async fn join_run(
    auth_user: AuthenticatedUser,
    path: web::Path<Uuid>,
    service: web::Data<Arc<dyn RoomServiceTrait>>,
) -> HttpResponse {
    service_response!(service.join_run(path.into_inner(), auth_user.user_id).await)
}

#[utoipa::path(
    post,
    path = "/api/me/runs/{run_id}/leave",
    tag = "rooms",
    security(("cookie_auth" = [])),
    params(("run_id" = Uuid, Path, description = "Run ID")),
    responses(
        (status = 200, description = "Left run", body = SimpleSuccessResponse),
        (status = 401, description = "Authentication required", body = ApiErrorResponse),
    )
)]
pub async fn leave_run(
    auth_user: AuthenticatedUser,
    path: web::Path<Uuid>,
    service: web::Data<Arc<dyn RoomServiceTrait>>,
) -> HttpResponse {
    match service
        .leave_run(path.into_inner(), auth_user.user_id)
        .await
    {
        Ok(()) => HttpResponse::Ok().json(SimpleSuccessResponse { success: true }),
        Err(e) => e.error_response(),
    }
}

#[utoipa::path(
    get,
    path = "/api/me/rooms/{room_id}/runs/active",
    tag = "rooms",
    security(("cookie_auth" = [])),
    params(("room_id" = Uuid, Path, description = "Room ID")),
    responses(
        (status = 200, description = "Active run", body = ActiveRunResponse),
        (status = 401, description = "Authentication required", body = ApiErrorResponse),
        (status = 404, description = "Run not found", body = ApiErrorResponse),
    )
)]
pub async fn get_active_run(
    auth_user: AuthenticatedUser,
    path: web::Path<Uuid>,
    service: web::Data<Arc<dyn RoomServiceTrait>>,
) -> HttpResponse {
    service_response!(
        service
            .get_active_run(path.into_inner(), auth_user.user_id)
            .await
    )
}

#[utoipa::path(
    post,
    path = "/api/me/runs/{run_id}/next-game",
    tag = "rooms",
    security(("cookie_auth" = [])),
    params(("run_id" = Uuid, Path, description = "Run ID")),
    responses(
        (status = 200, description = "Next game started", body = StartNextGameResponse),
        (status = 401, description = "Authentication required", body = ApiErrorResponse),
        (status = 404, description = "Run not found", body = ApiErrorResponse),
    )
)]
pub async fn start_next_game(
    auth_user: AuthenticatedUser,
    path: web::Path<Uuid>,
    service: web::Data<Arc<dyn RoomServiceTrait>>,
) -> HttpResponse {
    service_response!(
        service
            .start_next_game(path.into_inner(), auth_user.user_id)
            .await
    )
}

#[utoipa::path(
    get,
    path = "/api/me/runs/{run_id}/current-game",
    tag = "rooms",
    security(("cookie_auth" = [])),
    params(("run_id" = Uuid, Path, description = "Run ID")),
    responses(
        (status = 200, description = "Current game", body = CurrentGameResponse),
        (status = 401, description = "Authentication required", body = ApiErrorResponse),
        (status = 404, description = "Run not found", body = ApiErrorResponse),
    )
)]
pub async fn get_current_game(
    auth_user: AuthenticatedUser,
    path: web::Path<Uuid>,
    service: web::Data<Arc<dyn RoomServiceTrait>>,
) -> HttpResponse {
    service_response!(
        service
            .get_current_game(path.into_inner(), auth_user.user_id)
            .await
    )
}

#[utoipa::path(
    get,
    path = "/api/me/rooms/{room_id}/runs",
    tag = "rooms",
    security(("cookie_auth" = [])),
    params(("room_id" = Uuid, Path, description = "Room ID")),
    responses(
        (status = 200, description = "Runs in room", body = [RunListItem]),
        (status = 401, description = "Authentication required", body = ApiErrorResponse),
        (status = 404, description = "Room not found", body = ApiErrorResponse),
    )
)]
pub async fn list_runs(
    auth_user: AuthenticatedUser,
    path: web::Path<Uuid>,
    service: web::Data<Arc<dyn RoomServiceTrait>>,
) -> HttpResponse {
    service_response!(
        service
            .list_runs(path.into_inner(), auth_user.user_id)
            .await
    )
}

#[utoipa::path(
    post,
    path = "/api/me/runs/{run_id}/spectate-token",
    tag = "rooms",
    security(("cookie_auth" = [])),
    params(("run_id" = Uuid, Path, description = "Run ID")),
    responses(
        (status = 200, description = "Run spectator token minted", body = SpectateTokenResponse),
        (status = 401, description = "Authentication required", body = ApiErrorResponse),
        (status = 403, description = "Not a run participant", body = ApiErrorResponse),
    )
)]
/// Mint a run-scoped spectator token. Unlike the per-game spectator token, this
/// token grants read-only access to every game in the run, so a streamer can
/// share one link that follows the run across games.
pub async fn mint_run_spectate_token(
    auth_user: AuthenticatedUser,
    path: web::Path<Uuid>,
    service: web::Data<Arc<dyn RoomServiceTrait>>,
    auth_config: web::Data<AuthConfig>,
    redis: web::Data<Option<RedisClient>>,
) -> HttpResponse {
    let run_id = path.into_inner();

    match service.is_run_participant(run_id, auth_user.user_id).await {
        Ok(true) => {}
        Ok(false) => return AppError::Forbidden("room.not_run_player").error_response(),
        Err(e) => return e.error_response(),
    }

    match jwt::generate_run_spectate_token(run_id, &auth_config, RUN_SPECTATE_TOKEN_TTL_SECS) {
        Ok((token, claims)) => {
            let redis_key = format!("spectate_run_token:{}:{}", run_id, claims.jti);
            if let Some(redis_client) = redis.get_ref().as_ref() {
                let mut client = redis_client.clone();
                if let Err(e) = client
                    .set_ex(&redis_key, &token, RUN_SPECTATE_TOKEN_TTL_SECS)
                    .await
                {
                    tracing::error!(
                        "Failed to store run spectate token for run {}: {}",
                        run_id,
                        e
                    );
                }
            }

            let url = format!(
                "{}/run/{}/stream?token={}",
                auth_config.frontend_url, run_id, token
            );
            HttpResponse::Ok().json(SpectateTokenResponse { token, url })
        }
        Err(e) => {
            tracing::error!(
                "Failed to generate run spectate token for run {}: {}",
                run_id,
                e
            );
            AppError::Internal("Failed to generate run spectate token".to_string()).error_response()
        }
    }
}
