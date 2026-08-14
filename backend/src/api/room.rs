use std::sync::Arc;

use actix_web::{web, HttpResponse, ResponseError};
use uuid::Uuid;

use crate::auth::extractors::AuthenticatedUser;
use crate::room::service::RoomService;

#[derive(serde::Deserialize)]
pub struct CreateRoomRequest {
    pub name: String,
}

#[derive(serde::Deserialize)]
pub struct JoinRoomRequest {
    pub invitation_code: String,
}

#[derive(serde::Deserialize)]
pub struct CreateRunRequest {
    pub num_games: i32,
    pub bet: i32,
    pub player_ids: Vec<Uuid>,
}

#[derive(serde::Deserialize)]
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

pub async fn create_room(
    auth_user: AuthenticatedUser,
    body: web::Json<CreateRoomRequest>,
    service: web::Data<Arc<RoomService>>,
) -> HttpResponse {
    service_response!(
        service.create_room(auth_user.user_id, &body.name).await,
        actix_web::http::StatusCode::CREATED
    )
}

pub async fn list_rooms(
    auth_user: AuthenticatedUser,
    service: web::Data<Arc<RoomService>>,
) -> HttpResponse {
    service_response!(service.list_user_rooms(auth_user.user_id).await)
}

pub async fn get_room(
    auth_user: AuthenticatedUser,
    path: web::Path<Uuid>,
    service: web::Data<Arc<RoomService>>,
) -> HttpResponse {
    service_response!(
        service
            .get_room_detail(path.into_inner(), auth_user.user_id)
            .await
    )
}

pub async fn join_room(
    auth_user: AuthenticatedUser,
    body: web::Json<JoinRoomRequest>,
    service: web::Data<Arc<RoomService>>,
) -> HttpResponse {
    service_response!(
        service
            .join_room(auth_user.user_id, &body.invitation_code)
            .await
    )
}

pub async fn invite_to_room(
    auth_user: AuthenticatedUser,
    path: web::Path<Uuid>,
    body: web::Json<InviteToRoomRequest>,
    service: web::Data<Arc<RoomService>>,
) -> HttpResponse {
    match service
        .invite_to_room(path.into_inner(), auth_user.user_id, &body.email)
        .await
    {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({"success": true})),
        Err(e) => e.error_response(),
    }
}

pub async fn leave_room(
    auth_user: AuthenticatedUser,
    path: web::Path<Uuid>,
    service: web::Data<Arc<RoomService>>,
) -> HttpResponse {
    match service
        .leave_room(path.into_inner(), auth_user.user_id)
        .await
    {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({"success": true})),
        Err(e) => e.error_response(),
    }
}

pub async fn create_run(
    auth_user: AuthenticatedUser,
    path: web::Path<Uuid>,
    body: web::Json<CreateRunRequest>,
    service: web::Data<Arc<RoomService>>,
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

pub async fn join_run(
    auth_user: AuthenticatedUser,
    path: web::Path<Uuid>,
    service: web::Data<Arc<RoomService>>,
) -> HttpResponse {
    service_response!(service.join_run(path.into_inner(), auth_user.user_id).await)
}

pub async fn leave_run(
    auth_user: AuthenticatedUser,
    path: web::Path<Uuid>,
    service: web::Data<Arc<RoomService>>,
) -> HttpResponse {
    match service
        .leave_run(path.into_inner(), auth_user.user_id)
        .await
    {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({"success": true})),
        Err(e) => e.error_response(),
    }
}

pub async fn get_active_run(
    auth_user: AuthenticatedUser,
    path: web::Path<Uuid>,
    service: web::Data<Arc<RoomService>>,
) -> HttpResponse {
    service_response!(
        service
            .get_active_run(path.into_inner(), auth_user.user_id)
            .await
    )
}

pub async fn start_next_game(
    auth_user: AuthenticatedUser,
    path: web::Path<Uuid>,
    service: web::Data<Arc<RoomService>>,
) -> HttpResponse {
    service_response!(
        service
            .start_next_game(path.into_inner(), auth_user.user_id)
            .await
    )
}

pub async fn get_current_game(
    auth_user: AuthenticatedUser,
    path: web::Path<Uuid>,
    service: web::Data<Arc<RoomService>>,
) -> HttpResponse {
    service_response!(
        service
            .get_current_game(path.into_inner(), auth_user.user_id)
            .await
    )
}

pub async fn list_runs(
    auth_user: AuthenticatedUser,
    path: web::Path<Uuid>,
    service: web::Data<Arc<RoomService>>,
) -> HttpResponse {
    service_response!(
        service
            .list_runs(path.into_inner(), auth_user.user_id)
            .await
    )
}
