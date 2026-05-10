use std::sync::Arc;

use actix_web::{web, HttpMessage, HttpRequest, HttpResponse, ResponseError};
use uuid::Uuid;

use crate::api::dto::dashboard::PaginationParams;
use crate::api::dto::requests::{
    CreateGameRequest, PlayCardRequest, SendInvitesRequest, UserSearchQuery,
};
use crate::api::dto::responses::{
    AcceptInviteResponse, InvitationItem, InvitationsResponse, PlayCardResponse, UserSearchItem,
    UserSearchResponse,
};
use crate::api::services::dashboard_service::DashboardService;
use crate::auth::extractors::AuthenticatedUser;
use crate::database::repositories::{
    DashboardRepository, GameInviteRepository, PlayerRepository, UserRepository,
};
use crate::error::AppError;
use crate::observability::CorrelationId;

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
    body: web::Json<CreateGameRequest>,
    orchestrator: web::Data<std::sync::Arc<dyn crate::game::orchestrator::GameOrchestratorTrait>>,
    db: web::Data<sea_orm::DatabaseConnection>,
) -> HttpResponse {
    if let Err(e) = body.validate() {
        return HttpResponse::BadRequest().json(serde_json::json!({"error": e.to_string()}));
    }

    match body.game_mode.as_str() {
        "multiplayer" => {
            let result = orchestrator
                .create_multiplayer_game(
                    auth_user.user_id,
                    &auth_user.pseudo,
                    body.bet,
                    body.max_players,
                )
                .await;

            match result {
                Ok(outcome) => {
                    let response: crate::api::dto::responses::MultiplayerGameResponse =
                        outcome.into();
                    HttpResponse::Created().json(response)
                }
                Err(e) => {
                    tracing::error!(
                        "Failed to create multiplayer game for user {}: {}",
                        auth_user.user_id,
                        e
                    );
                    match &e {
                        crate::error::GameError::InsufficientCredits => {
                            HttpResponse::PaymentRequired()
                                .json(serde_json::json!({"error": e.to_string()}))
                        }
                        _ => HttpResponse::InternalServerError()
                            .json(serde_json::json!({"error": "Failed to create game"})),
                    }
                }
            }
        }
        _ => {
            let result = orchestrator
                .create_quick_game_for_user(auth_user.user_id, db.get_ref())
                .await;

            match result {
                Ok(outcome) => {
                    let response: crate::api::dto::responses::QuickGameResponse = outcome.into();
                    HttpResponse::Created().json(response)
                }
                Err(e) => {
                    tracing::error!("Failed to create solo game for user: {}", e);
                    HttpResponse::InternalServerError()
                        .json(serde_json::json!({"error": "Failed to create game"}))
                }
            }
        }
    }
}

pub async fn send_invites(
    auth_user: AuthenticatedUser,
    path: web::Path<Uuid>,
    body: web::Json<SendInvitesRequest>,
    orchestrator: web::Data<Arc<dyn crate::game::orchestrator::GameOrchestratorTrait>>,
    cache: web::Data<Arc<crate::cache::UserCache>>,
    db: web::Data<sea_orm::DatabaseConnection>,
) -> HttpResponse {
    let game_id = path.into_inner();
    let mut invited_user_ids: Vec<Uuid> = body.user_ids.clone();
    let user_repo = UserRepository::new(db.get_ref().clone());
    let mut resolved_from_pseudos = Vec::new();

    for pseudo in &body.pseudos {
        let trimmed = pseudo.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(uuid) = cache.get_uuid_by_pseudo(trimmed).await {
            resolved_from_pseudos.push((uuid, trimmed.to_string()));
            continue;
        }
        if let Ok(Some(user)) = user_repo.find_by_pseudo(trimmed).await {
            cache.put(user.id, user.pseudo.clone(), user.email).await;
            resolved_from_pseudos.push((user.id, user.pseudo));
        }
    }

    let mut seen_uuid = std::collections::HashSet::new();
    let mut seen_pseudo = std::collections::HashSet::new();
    let mut duplicates: Vec<String> = Vec::new();

    for (uuid, pseudo) in &resolved_from_pseudos {
        if !seen_uuid.insert(*uuid) {
            duplicates.push(pseudo.clone());
        }
        seen_pseudo.insert(pseudo.clone());
    }

    for uid in &invited_user_ids {
        if !seen_uuid.insert(*uid) {
            duplicates.push(uid.to_string());
        }
    }

    if !duplicates.is_empty() {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "error": format!("Duplicate players in invite list: {}", duplicates.join(", "))
        }));
    }

    invited_user_ids.extend(resolved_from_pseudos.into_iter().map(|(u, _)| u));

    if seen_uuid.contains(&auth_user.user_id) {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "error": "You cannot invite yourself"
        }));
    }

    let player_repo = PlayerRepository::new(db.get_ref().clone());
    let existing_players = match player_repo.list_by_game(game_id).await {
        Ok(players) => players,
        Err(e) => {
            tracing::error!("Failed to check existing players: {}", e);
            return HttpResponse::InternalServerError()
                .json(serde_json::json!({"error": "Internal server error"}));
        }
    };
    let existing_ids: std::collections::HashSet<Uuid> =
        existing_players.iter().filter_map(|p| p.user_id).collect();

    let already_in: Vec<String> = invited_user_ids
        .iter()
        .filter(|id| existing_ids.contains(id))
        .map(|id| id.to_string())
        .collect();
    if !already_in.is_empty() {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Some users are already players in this game"
        }));
    }

    if invited_user_ids.is_empty() {
        return HttpResponse::Ok().json(serde_json::json!({
            "success": true,
            "message": "No valid users to invite"
        }));
    }

    match orchestrator
        .send_invites(game_id, auth_user.user_id, invited_user_ids)
        .await
    {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({
            "success": true,
            "message": "Invites sent"
        })),
        Err(e) => {
            let status = match &e {
                crate::error::GameError::NotCreator => actix_web::http::StatusCode::FORBIDDEN,
                crate::error::GameError::GameNotPending => actix_web::http::StatusCode::CONFLICT,
                crate::error::GameError::GameNotFound => actix_web::http::StatusCode::NOT_FOUND,
                _ => actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            };
            HttpResponse::build(status).json(serde_json::json!({"error": e.to_string()}))
        }
    }
}

pub async fn accept_invite(
    auth_user: AuthenticatedUser,
    path: web::Path<Uuid>,
    orchestrator: web::Data<Arc<dyn crate::game::orchestrator::GameOrchestratorTrait>>,
) -> HttpResponse {
    let game_id = path.into_inner();

    match orchestrator
        .accept_invite(game_id, auth_user.user_id, &auth_user.pseudo)
        .await
    {
        Ok(outcome) => HttpResponse::Ok().json(AcceptInviteResponse {
            success: true,
            message: match outcome.game_status.as_str() {
                "ready" => "Game is ready to start!".to_string(),
                _ => "Joined game successfully".to_string(),
            },
            player_id: outcome.player_id,
            position: outcome.position,
            player_count: outcome.player_count,
            max_players: outcome.max_players,
            game_status: outcome.game_status,
        }),
        Err(e) => {
            let status = match &e {
                crate::error::GameError::NotInvited
                | crate::error::GameError::CreatorCannotJoin => {
                    actix_web::http::StatusCode::FORBIDDEN
                }
                crate::error::GameError::GameNotPending
                | crate::error::GameError::AlreadyJoined
                | crate::error::GameError::GameFull => actix_web::http::StatusCode::CONFLICT,
                crate::error::GameError::InsufficientCredits => {
                    actix_web::http::StatusCode::PAYMENT_REQUIRED
                }
                crate::error::GameError::GameNotFound => actix_web::http::StatusCode::NOT_FOUND,
                _ => actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            };
            HttpResponse::build(status).json(serde_json::json!({"error": e.to_string()}))
        }
    }
}

pub async fn get_invitations(
    auth_user: AuthenticatedUser,
    db: web::Data<sea_orm::DatabaseConnection>,
) -> HttpResponse {
    let invite_repo = GameInviteRepository::new(db.get_ref().clone());
    let user_repo = UserRepository::new(db.get_ref().clone());

    let pending = match invite_repo
        .list_pending_invites_for_user(auth_user.user_id)
        .await
    {
        Ok(row) => row,
        Err(e) => {
            tracing::error!("Failed to fetch invitations: {}", e);
            return HttpResponse::InternalServerError()
                .json(serde_json::json!({"error": "Internal server error"}));
        }
    };

    let mut items = Vec::new();
    for (invite, game) in pending {
        let player_count =
            crate::database::repositories::PlayerRepository::new(db.get_ref().clone())
                .list_by_game(game.id)
                .await
                .map(|p| p.len() as i64)
                .unwrap_or(0);

        let creator_pseudo = match game.creator_id {
            Some(uid) => user_repo
                .find_by_id(uid)
                .await
                .ok()
                .flatten()
                .map(|u| u.pseudo)
                .unwrap_or_else(|| "Unknown".to_string()),
            None => "Unknown".to_string(),
        };

        items.push(InvitationItem {
            invite_id: invite.id,
            game_id: game.id,
            creator_pseudo,
            bet: game.bet,
            player_count,
            max_players: game.max_players as i32,
            created_at: invite.created_at.to_rfc3339(),
            expires_at: game.invite_expires_at.map(|t| t.to_rfc3339()),
        });
    }

    HttpResponse::Ok().json(InvitationsResponse { invitations: items })
}

pub async fn start_game(
    auth_user: AuthenticatedUser,
    path: web::Path<Uuid>,
    orchestrator: web::Data<Arc<dyn crate::game::orchestrator::GameOrchestratorTrait>>,
) -> HttpResponse {
    let game_id = path.into_inner();

    match orchestrator.start_game(game_id, auth_user.user_id).await {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({
            "success": true,
            "message": "Game started"
        })),
        Err(e) => {
            let status = match &e {
                crate::error::GameError::NotCreator => actix_web::http::StatusCode::FORBIDDEN,
                crate::error::GameError::GameNotReady => actix_web::http::StatusCode::CONFLICT,
                crate::error::GameError::GameNotFound => actix_web::http::StatusCode::NOT_FOUND,
                _ => actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            };
            HttpResponse::build(status).json(serde_json::json!({"error": e.to_string()}))
        }
    }
}

pub async fn play_game(
    req: HttpRequest,
    auth_user: AuthenticatedUser,
    path: web::Path<Uuid>,
    payload: web::Json<PlayCardRequest>,
    orchestrator: web::Data<Arc<dyn crate::game::orchestrator::GameOrchestratorTrait>>,
    db: web::Data<sea_orm::DatabaseConnection>,
) -> HttpResponse {
    let game_id = path.into_inner();
    let correlation_id = req.extensions().get::<CorrelationId>().copied();

    if let Err(e) = payload.validate() {
        return AppError::from(e).error_response();
    }

    let player_repo = PlayerRepository::new(db.get_ref().clone());
    let player = match player_repo
        .find_by_game_and_user(game_id, auth_user.user_id)
        .await
    {
        Ok(Some(p)) => p,
        _ => {
            return HttpResponse::Forbidden()
                .json(serde_json::json!({"error": "You are not a player in this game"}));
        }
    };

    match orchestrator
        .play_card(game_id, player.id, payload.card_index, correlation_id)
        .await
    {
        Ok(outcome) => {
            let response: PlayCardResponse = outcome.into();
            HttpResponse::Ok().json(response)
        }
        Err(e) => AppError::from(e).error_response(),
    }
}

pub async fn game_state(
    auth_user: AuthenticatedUser,
    path: web::Path<Uuid>,
    service: web::Data<Arc<DashboardServiceType>>,
) -> HttpResponse {
    let game_id = path.into_inner();
    service
        .get_game(auth_user.user_id, game_id)
        .await
        .unwrap_or_else(|e| {
            HttpResponse::InternalServerError().json(serde_json::json!({"error": e.to_string()}))
        })
}

pub async fn search_users(
    _auth_user: AuthenticatedUser,
    query: web::Query<UserSearchQuery>,
    db: web::Data<sea_orm::DatabaseConnection>,
    cache: web::Data<Arc<crate::cache::UserCache>>,
) -> HttpResponse {
    if query.q.trim().len() < 2 {
        return HttpResponse::Ok().json(UserSearchResponse { users: vec![] });
    }

    let user_repo = UserRepository::new(db.get_ref().clone());
    match user_repo
        .find_by_pseudo_prefix(query.q.trim(), query.limit)
        .await
    {
        Ok(users) => {
            let bulk: Vec<(Uuid, String, String)> = users
                .iter()
                .map(|u| (u.id, u.pseudo.clone(), u.email.clone()))
                .collect();
            cache.populate_bulk(&bulk).await;

            let items: Vec<UserSearchItem> = users
                .into_iter()
                .map(|u| UserSearchItem {
                    id: u.id,
                    pseudo: u.pseudo,
                })
                .collect();
            HttpResponse::Ok().json(UserSearchResponse { users: items })
        }
        Err(e) => {
            tracing::error!("User search failed: {}", e);
            HttpResponse::InternalServerError()
                .json(serde_json::json!({"error": "Internal server error"}))
        }
    }
}
