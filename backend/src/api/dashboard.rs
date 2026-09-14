use std::sync::Arc;

use actix_web::{web, HttpMessage, HttpRequest, HttpResponse, ResponseError};
use uuid::Uuid;

use crate::api::dto::dashboard::{GameHistoryResponse, PaginationParams, PlayerProfileResponse};
use crate::api::dto::requests::InviteActionQuery;
use crate::api::dto::requests::{
    CreateGameRequest, PlayCardRequest, SendInvitesRequest, UserSearchQuery,
};
use crate::api::dto::responses::{
    ApiErrorResponse, InvitationsResponse, MultiplayerGameResponse, PlayCardResponse,
    QuickGameResponse, RespondToInviteResponse, SendInvitesResponse, SpectateTokenResponse,
    UserSearchResponse,
};
use crate::api::services::dashboard_service::{DashboardService, SendInvitesParams};
use crate::auth::config::AuthConfig;
use crate::auth::extractors::AuthenticatedUser;
use crate::auth::jwt;
use crate::database::repositories::DashboardRepository;
use crate::error::AppError;
use crate::i18n::I18n;
use crate::mailer::Mailer;
use crate::messaging::RedisClient;
use crate::observability::{metrics, CorrelationId};

/// TTL for read-only spectator tokens in seconds (6 hours).
const SPECTATE_TOKEN_TTL_SECS: u64 = 21600;

pub type DashboardServiceType = DashboardService<DashboardRepository>;

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

#[utoipa::path(
    get,
    path = "/api/me/profile",
    tag = "dashboard",
    security(("cookie_auth" = [])),
    responses(
        (status = 200, description = "Player profile", body = PlayerProfileResponse),
        (status = 401, description = "Authentication required", body = ApiErrorResponse),
    )
)]
pub async fn get_profile(
    auth_user: AuthenticatedUser,
    service: web::Data<Arc<DashboardServiceType>>,
) -> HttpResponse {
    service_response!(service.get_profile(auth_user.user_id).await)
}

#[utoipa::path(
    get,
    path = "/api/me/games",
    tag = "dashboard",
    security(("cookie_auth" = [])),
    params(PaginationParams),
    responses(
        (status = 200, description = "Game history", body = GameHistoryResponse),
        (status = 401, description = "Authentication required", body = ApiErrorResponse),
    )
)]
pub async fn list_games(
    auth_user: AuthenticatedUser,
    query: web::Query<PaginationParams>,
    service: web::Data<Arc<DashboardServiceType>>,
) -> HttpResponse {
    service_response!(
        service
            .list_games(auth_user.user_id, query.into_inner())
            .await
    )
}

#[utoipa::path(
    get,
    path = "/api/me/games/{game_id}",
    tag = "dashboard",
    security(("cookie_auth" = [])),
    params(("game_id" = Uuid, Path, description = "Game ID")),
    responses(
        (status = 200, description = "Game state", body = QuickGameResponse),
        (status = 401, description = "Authentication required", body = ApiErrorResponse),
        (status = 404, description = "Game not found", body = ApiErrorResponse),
    )
)]
pub async fn get_game(
    auth_user: AuthenticatedUser,
    service: web::Data<Arc<DashboardServiceType>>,
    path: web::Path<Uuid>,
) -> HttpResponse {
    service_response!(service.get_game(auth_user.user_id, path.into_inner()).await)
}

#[utoipa::path(
    get,
    path = "/api/me/active-game",
    tag = "dashboard",
    security(("cookie_auth" = [])),
    responses(
        (status = 200, description = "Active game", body = QuickGameResponse),
        (status = 401, description = "Authentication required", body = ApiErrorResponse),
    )
)]
pub async fn get_active_game(
    auth_user: AuthenticatedUser,
    service: web::Data<Arc<DashboardServiceType>>,
) -> HttpResponse {
    service_response!(service.get_active_game(auth_user.user_id).await)
}

#[utoipa::path(
    post,
    path = "/api/me/games",
    tag = "dashboard",
    security(("cookie_auth" = [])),
    request_body = CreateGameRequest,
    responses(
        (status = 201, description = "Game created", content(
            (QuickGameResponse = "application/json"),
            (MultiplayerGameResponse = "application/json"),
        )),
        (status = 400, description = "Validation error", body = ApiErrorResponse),
        (status = 401, description = "Authentication required", body = ApiErrorResponse),
    )
)]
pub async fn create_game(
    auth_user: AuthenticatedUser,
    body: web::Json<CreateGameRequest>,
    orchestrator: web::Data<Arc<dyn crate::game::service::GameLifecycleService>>,
    db: web::Data<sea_orm::DatabaseConnection>,
) -> HttpResponse {
    if let Err(e) = body.validate() {
        return AppError::from(e).error_response();
    }

    match body.game_mode.as_str() {
        "multiplayer" => {
            match orchestrator
                .create_multiplayer_game(
                    auth_user.user_id,
                    &auth_user.pseudo,
                    body.bet,
                    body.max_players,
                )
                .await
            {
                Ok(outcome) => {
                    metrics::ACTIVE_GAMES.inc();
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
                    AppError::from(e).error_response()
                }
            }
        }
        _ => {
            match orchestrator
                .create_quick_game_for_user_with_step_by_step(
                    auth_user.user_id,
                    db.get_ref(),
                    body.step_by_step,
                )
                .await
            {
                Ok(outcome) => {
                    metrics::ACTIVE_GAMES.inc();
                    let response: crate::api::dto::responses::QuickGameResponse = outcome.into();
                    HttpResponse::Created().json(response)
                }
                Err(e) => {
                    tracing::error!("Failed to create solo game for user: {}", e);
                    AppError::from(e).error_response()
                }
            }
        }
    }
}

#[utoipa::path(
    post,
    path = "/api/games/{game_id}/invites",
    tag = "dashboard",
    security(("cookie_auth" = [])),
    params(("game_id" = Uuid, Path, description = "Game ID")),
    request_body = SendInvitesRequest,
    responses(
        (status = 200, description = "Invites sent", body = SendInvitesResponse),
        (status = 400, description = "Invalid request", body = ApiErrorResponse),
        (status = 401, description = "Authentication required", body = ApiErrorResponse),
        (status = 409, description = "Player already in game", body = ApiErrorResponse),
    )
)]
pub async fn send_invites(
    auth_user: AuthenticatedUser,
    path: web::Path<Uuid>,
    body: web::Json<SendInvitesRequest>,
    orchestrator: web::Data<Arc<dyn crate::game::service::InviteService>>,
    service: web::Data<Arc<DashboardServiceType>>,
    mailer: web::Data<Arc<dyn Mailer>>,
    i18n: I18n,
) -> HttpResponse {
    let game_id = path.into_inner();

    let (invited_user_ids, seen_uuid, duplicates) = match service
        .resolve_invite_user_ids(&SendInvitesParams {
            user_ids: body.user_ids.clone(),
            pseudos: body.pseudos.clone(),
        })
        .await
    {
        Ok(result) => result,
        Err(e) => return e.error_response(),
    };

    if !duplicates.is_empty() {
        return AppError::BadRequest(i18n.t_replace(
            "game.duplicate_players",
            "{duplicates}",
            &duplicates.join(", "),
        ))
        .error_response();
    }

    if seen_uuid.contains(&auth_user.user_id) {
        return AppError::BadRequest(i18n.t("game.cannot_invite_self")).error_response();
    }

    let existing_ids = match service.check_existing_players(game_id).await {
        Ok(ids) => ids,
        Err(e) => return e.error_response(),
    };

    let already_in: Vec<String> = invited_user_ids
        .iter()
        .filter(|id| existing_ids.contains(id))
        .map(|id| id.to_string())
        .collect();
    if !already_in.is_empty() {
        return AppError::Conflict(i18n.t("game.already_players")).error_response();
    }

    if invited_user_ids.is_empty() {
        return HttpResponse::Ok().json(SendInvitesResponse {
            success: true,
            message: i18n.t("game.no_valid_users"),
            email_errors: None,
        });
    }

    match orchestrator
        .send_invites(game_id, auth_user.user_id, invited_user_ids.clone())
        .await
    {
        Ok(()) => {
            let users = match service.find_users_by_ids(&invited_user_ids).await {
                Ok(u) => u,
                Err(e) => return e.error_response(),
            };
            let mut email_errors = 0u32;
            for user in &users {
                let game_id_str = game_id.to_string();
                if let Err(e) = mailer
                    .send_invitation(&user.email, &auth_user.pseudo, &game_id_str, i18n.lang)
                    .await
                {
                    tracing::error!("Failed to send invitation email to {}: {}", user.email, e);
                    email_errors += 1;
                }
            }
            HttpResponse::Ok().json(SendInvitesResponse {
                success: true,
                message: i18n.t("game.invites_sent"),
                email_errors: Some(email_errors),
            })
        }
        Err(e) => AppError::from(e).error_response(),
    }
}

#[utoipa::path(
    post,
    path = "/api/games/{game_id}/respond",
    tag = "dashboard",
    security(("cookie_auth" = [])),
    params(
        ("game_id" = Uuid, Path, description = "Game ID"),
        InviteActionQuery,
    ),
    responses(
        (status = 200, description = "Invite response", body = RespondToInviteResponse),
        (status = 400, description = "Invalid action", body = ApiErrorResponse),
        (status = 401, description = "Authentication required", body = ApiErrorResponse),
    )
)]
pub async fn respond_to_invite(
    auth_user: AuthenticatedUser,
    path: web::Path<Uuid>,
    query: web::Query<InviteActionQuery>,
    orchestrator: web::Data<Arc<dyn crate::game::service::InviteService>>,
    i18n: I18n,
) -> HttpResponse {
    let game_id = path.into_inner();
    let action = match query.validate() {
        Ok(a) => a,
        Err(e) => return AppError::from(e).error_response(),
    };

    match action {
        "accept" => match orchestrator
            .accept_invite(game_id, auth_user.user_id, &auth_user.pseudo)
            .await
        {
            Ok(outcome) => HttpResponse::Ok().json(RespondToInviteResponse {
                success: true,
                message: match outcome.game_status.as_str() {
                    "ready" => i18n.t("game.game_ready"),
                    _ => i18n.t("game.joined"),
                },
                action: "accept".to_string(),
                player_id: Some(outcome.player_id),
                position: Some(outcome.position),
                player_count: Some(outcome.player_count),
                max_players: Some(outcome.max_players),
                game_status: Some(outcome.game_status),
            }),
            Err(e) => AppError::from(e).error_response(),
        },
        "decline" => match orchestrator
            .decline_invite(game_id, auth_user.user_id)
            .await
        {
            Ok(()) => HttpResponse::Ok().json(RespondToInviteResponse {
                success: true,
                message: i18n.t("game.declined"),
                action: "decline".to_string(),
                player_id: None,
                position: None,
                player_count: None,
                max_players: None,
                game_status: None,
            }),
            Err(e) => AppError::from(e).error_response(),
        },
        _ => unreachable!(),
    }
}

#[utoipa::path(
    get,
    path = "/api/me/invitations",
    tag = "dashboard",
    security(("cookie_auth" = [])),
    responses(
        (status = 200, description = "Pending invitations", body = InvitationsResponse),
        (status = 401, description = "Authentication required", body = ApiErrorResponse),
    )
)]
pub async fn get_invitations(
    auth_user: AuthenticatedUser,
    service: web::Data<Arc<DashboardServiceType>>,
) -> HttpResponse {
    service_response!(service.get_invitations(auth_user.user_id).await)
}

#[utoipa::path(
    post,
    path = "/api/games/{game_id}/start",
    tag = "dashboard",
    security(("cookie_auth" = [])),
    params(("game_id" = Uuid, Path, description = "Game ID")),
    responses(
        (status = 200, description = "Game started", body = QuickGameResponse),
        (status = 401, description = "Authentication required", body = ApiErrorResponse),
        (status = 403, description = "Not the creator", body = ApiErrorResponse),
    )
)]
pub async fn start_game(
    auth_user: AuthenticatedUser,
    path: web::Path<Uuid>,
    orchestrator: web::Data<Arc<dyn crate::game::service::GameLifecycleService>>,
    service: web::Data<Arc<DashboardServiceType>>,
) -> HttpResponse {
    let game_id = path.into_inner();

    match orchestrator.start_game(game_id, auth_user.user_id).await {
        Ok(()) => service_response!(service.get_game(auth_user.user_id, game_id).await),
        Err(e) => AppError::from(e).error_response(),
    }
}

#[utoipa::path(
    post,
    path = "/api/games/{game_id}/play",
    tag = "dashboard",
    security(("cookie_auth" = [])),
    params(("game_id" = Uuid, Path, description = "Game ID")),
    request_body = PlayCardRequest,
    responses(
        (status = 200, description = "Card played", body = PlayCardResponse),
        (status = 400, description = "Invalid request", body = ApiErrorResponse),
        (status = 401, description = "Authentication required", body = ApiErrorResponse),
        (status = 403, description = "Not a player or not your turn", body = ApiErrorResponse),
    )
)]
pub async fn play_game(
    req: HttpRequest,
    auth_user: AuthenticatedUser,
    path: web::Path<Uuid>,
    payload: web::Json<PlayCardRequest>,
    orchestrator: web::Data<Arc<dyn crate::game::service::GamePlayService>>,
    service: web::Data<Arc<DashboardServiceType>>,
    i18n: I18n,
) -> HttpResponse {
    let game_id = path.into_inner();
    let correlation_id = req.extensions().get::<CorrelationId>().copied();

    if let Err(e) = payload.validate() {
        return AppError::from(e).error_response();
    }

    let game = match service.get_game(auth_user.user_id, game_id).await {
        Ok(g) => g,
        Err(e) => return e.error_response(),
    };

    let player = match game.players.iter().find(|p| p.is_current_user) {
        Some(p) => p,
        None => {
            return AppError::Forbidden(i18n.t("game.not_player")).error_response();
        }
    };

    match orchestrator
        .play_card(game_id, player.id, payload.card_index, correlation_id, None)
        .await
    {
        Ok(outcome) => {
            let response: PlayCardResponse = outcome.into();
            HttpResponse::Ok().json(response)
        }
        Err(e) => AppError::from(e).error_response(),
    }
}

#[utoipa::path(
    get,
    path = "/api/games/{game_id}/me",
    tag = "dashboard",
    security(("cookie_auth" = [])),
    params(("game_id" = Uuid, Path, description = "Game ID")),
    responses(
        (status = 200, description = "Game state", body = QuickGameResponse),
        (status = 401, description = "Authentication required", body = ApiErrorResponse),
        (status = 404, description = "Game not found", body = ApiErrorResponse),
    )
)]
pub async fn game_state(
    auth_user: AuthenticatedUser,
    path: web::Path<Uuid>,
    service: web::Data<Arc<DashboardServiceType>>,
) -> HttpResponse {
    service_response!(service.get_game(auth_user.user_id, path.into_inner()).await)
}

#[utoipa::path(
    get,
    path = "/api/users/search",
    tag = "dashboard",
    security(("cookie_auth" = [])),
    params(UserSearchQuery),
    responses(
        (status = 200, description = "Matching users", body = UserSearchResponse),
        (status = 401, description = "Authentication required", body = ApiErrorResponse),
    )
)]
pub async fn search_users(
    _auth_user: AuthenticatedUser,
    query: web::Query<UserSearchQuery>,
    service: web::Data<Arc<DashboardServiceType>>,
) -> HttpResponse {
    service_response!(service.search_users(&query.into_inner()).await)
}

#[utoipa::path(
    post,
    path = "/api/games/{game_id}/spectate-token",
    tag = "dashboard",
    security(("cookie_auth" = [])),
    params(("game_id" = Uuid, Path, description = "Game ID")),
    responses(
        (status = 200, description = "Spectator token minted", body = SpectateTokenResponse),
        (status = 401, description = "Authentication required", body = ApiErrorResponse),
        (status = 403, description = "Not a participant", body = ApiErrorResponse),
    )
)]
/// Mint a private, short-lived spectator token for the stream view.
/// Only a game participant may mint a token, and the token grants read-only
/// access to public game state (no hand data) via the WebSocket.
pub async fn mint_spectate_token(
    auth_user: AuthenticatedUser,
    path: web::Path<Uuid>,
    service: web::Data<Arc<DashboardServiceType>>,
    auth_config: web::Data<AuthConfig>,
    redis: web::Data<Option<RedisClient>>,
) -> HttpResponse {
    let game_id = path.into_inner();

    let participants = match service.check_existing_players(game_id).await {
        Ok(ids) => ids,
        Err(e) => return e.error_response(),
    };
    if !participants.contains(&auth_user.user_id) {
        return AppError::Forbidden("You are not a participant of this game".to_string())
            .error_response();
    }

    match jwt::generate_spectate_token(game_id, &auth_config, SPECTATE_TOKEN_TTL_SECS) {
        Ok((token, claims)) => {
            let redis_key = format!("spectate_token:{}:{}", game_id, claims.jti);
            if let Some(redis_client) = redis.get_ref().as_ref() {
                let mut client = redis_client.clone();
                if let Err(e) = client
                    .set_ex(&redis_key, &token, SPECTATE_TOKEN_TTL_SECS)
                    .await
                {
                    tracing::error!("Failed to store spectate token for game {}: {}", game_id, e);
                }
            }

            let url = format!(
                "{}/game/{}/stream?token={}",
                auth_config.frontend_url, game_id, token
            );
            HttpResponse::Ok().json(SpectateTokenResponse { token, url })
        }
        Err(e) => {
            tracing::error!(
                "Failed to generate spectate token for game {}: {}",
                game_id,
                e
            );
            AppError::Internal("Failed to generate spectate token".to_string()).error_response()
        }
    }
}

#[cfg(test)]
#[path = "dashboard_tests.rs"]
mod tests;
