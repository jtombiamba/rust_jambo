use std::sync::Arc;

use actix_web::{web, HttpMessage, HttpRequest, HttpResponse, ResponseError};
use uuid::Uuid;

use crate::api::dto::dashboard::PaginationParams;
use crate::api::dto::requests::InviteActionQuery;
use crate::api::dto::requests::{
    CreateGameRequest, PlayCardRequest, SendInvitesRequest, UserSearchQuery,
};
use crate::api::dto::responses::{PlayCardResponse, RespondToInviteResponse};
use crate::api::services::dashboard_service::{DashboardService, SendInvitesParams};
use crate::auth::extractors::AuthenticatedUser;
use crate::database::repositories::DashboardRepository;
use crate::error::AppError;
use crate::i18n::I18n;
use crate::mailer::Mailer;
use crate::observability::CorrelationId;

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

pub async fn get_profile(
    auth_user: AuthenticatedUser,
    service: web::Data<Arc<DashboardServiceType>>,
) -> HttpResponse {
    service_response!(service.get_profile(auth_user.user_id).await)
}

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

pub async fn get_game(
    auth_user: AuthenticatedUser,
    service: web::Data<Arc<DashboardServiceType>>,
    path: web::Path<Uuid>,
) -> HttpResponse {
    service_response!(service.get_game(auth_user.user_id, path.into_inner()).await)
}

pub async fn get_active_game(
    auth_user: AuthenticatedUser,
    service: web::Data<Arc<DashboardServiceType>>,
) -> HttpResponse {
    service_response!(service.get_active_game(auth_user.user_id).await)
}

pub async fn create_game(
    auth_user: AuthenticatedUser,
    body: web::Json<CreateGameRequest>,
    orchestrator: web::Data<Arc<dyn crate::game::service::GameServiceTrait>>,
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

pub async fn send_invites(
    auth_user: AuthenticatedUser,
    path: web::Path<Uuid>,
    body: web::Json<SendInvitesRequest>,
    orchestrator: web::Data<Arc<dyn crate::game::service::GameServiceTrait>>,
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
        return HttpResponse::Ok().json(serde_json::json!({
            "success": true,
            "message": i18n.t("game.no_valid_users")
        }));
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
            HttpResponse::Ok().json(serde_json::json!({
                "success": true,
                "message": i18n.t("game.invites_sent"),
                "email_errors": email_errors
            }))
        }
        Err(e) => AppError::from(e).error_response(),
    }
}

pub async fn respond_to_invite(
    auth_user: AuthenticatedUser,
    path: web::Path<Uuid>,
    query: web::Query<InviteActionQuery>,
    orchestrator: web::Data<Arc<dyn crate::game::service::GameServiceTrait>>,
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

pub async fn get_invitations(
    auth_user: AuthenticatedUser,
    service: web::Data<Arc<DashboardServiceType>>,
) -> HttpResponse {
    service_response!(service.get_invitations(auth_user.user_id).await)
}

pub async fn start_game(
    auth_user: AuthenticatedUser,
    path: web::Path<Uuid>,
    orchestrator: web::Data<Arc<dyn crate::game::service::GameServiceTrait>>,
    service: web::Data<Arc<DashboardServiceType>>,
) -> HttpResponse {
    let game_id = path.into_inner();

    match orchestrator.start_game(game_id, auth_user.user_id).await {
        Ok(()) => service_response!(service.get_game(auth_user.user_id, game_id).await),
        Err(e) => AppError::from(e).error_response(),
    }
}

pub async fn play_game(
    req: HttpRequest,
    auth_user: AuthenticatedUser,
    path: web::Path<Uuid>,
    payload: web::Json<PlayCardRequest>,
    orchestrator: web::Data<Arc<dyn crate::game::service::GameServiceTrait>>,
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

pub async fn game_state(
    auth_user: AuthenticatedUser,
    path: web::Path<Uuid>,
    service: web::Data<Arc<DashboardServiceType>>,
) -> HttpResponse {
    service_response!(service.get_game(auth_user.user_id, path.into_inner()).await)
}

pub async fn search_users(
    _auth_user: AuthenticatedUser,
    query: web::Query<UserSearchQuery>,
    service: web::Data<Arc<DashboardServiceType>>,
) -> HttpResponse {
    service_response!(service.search_users(&query.into_inner()).await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::extractors::AuthenticatedUser;
    use crate::error::GameError;
    use crate::game::service::mock::MockGameService;
    use crate::game::service::AcceptInviteOutcome;
    use crate::i18n::Translator;
    use actix_web::{test, web, App};
    use std::sync::Arc;
    use uuid::Uuid;

    async fn make_app(
        mock: Arc<dyn crate::game::service::GameServiceTrait>,
    ) -> impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    > {
        test::init_service(
            App::new()
                .app_data(web::Data::new(mock))
                .app_data(web::Data::new(Arc::new(Translator::new())))
                .route("/{game_id}/respond", web::post().to(respond_to_invite)),
        )
        .await
    }

    fn authenticated_user() -> AuthenticatedUser {
        AuthenticatedUser {
            user_id: Uuid::new_v4(),
            pseudo: "TestPlayer".to_string(),
        }
    }

    fn accept_outcome() -> AcceptInviteOutcome {
        AcceptInviteOutcome {
            player_id: Uuid::new_v4(),
            position: 1,
            player_count: 2,
            max_players: 4,
            game_status: "pending".to_string(),
        }
    }

    #[actix_web::test]
    async fn respond_accept_success() {
        let outcome = accept_outcome();
        let mock = Arc::new(MockGameService::ok());
        mock.set_accept_invite_result(Ok(outcome));
        let app = make_app(mock).await;
        let user = authenticated_user();
        let game_id = Uuid::new_v4();

        let req = test::TestRequest::post()
            .uri(&format!("/{game_id}/respond?action=accept"))
            .to_request();
        req.extensions_mut().insert(user);

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["success"], true);
        assert_eq!(body["action"], "accept");
        assert_eq!(body["message"], "Joined game successfully");
        assert!(body["player_id"].is_string());
        assert_eq!(body["position"], 1);
        assert_eq!(body["player_count"], 2);
        assert_eq!(body["max_players"], 4);
        assert_eq!(body["game_status"], "pending");
    }

    #[actix_web::test]
    async fn respond_accept_game_ready() {
        let mut outcome = accept_outcome();
        outcome.game_status = "ready".to_string();
        let mock = Arc::new(MockGameService::ok());
        mock.set_accept_invite_result(Ok(outcome));
        let app = make_app(mock).await;
        let user = authenticated_user();
        let game_id = Uuid::new_v4();

        let req = test::TestRequest::post()
            .uri(&format!("/{game_id}/respond?action=accept"))
            .to_request();
        req.extensions_mut().insert(user);

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["success"], true);
        assert_eq!(body["message"], "Game is ready to start!");
    }

    #[actix_web::test]
    async fn respond_accept_not_invited() {
        let mock = Arc::new(MockGameService::ok());
        mock.set_accept_invite_result(Err(GameError::NotInvited));
        let app = make_app(mock).await;
        let user = authenticated_user();
        let game_id = Uuid::new_v4();

        let req = test::TestRequest::post()
            .uri(&format!("/{game_id}/respond?action=accept"))
            .to_request();
        req.extensions_mut().insert(user);

        let resp = test::call_service(&app, req).await;
        assert!(!resp.status().is_success());
        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["success"], false);
    }

    #[actix_web::test]
    async fn respond_decline_success() {
        let mock = Arc::new(MockGameService::ok());
        let app = make_app(mock).await;
        let user = authenticated_user();
        let game_id = Uuid::new_v4();

        let req = test::TestRequest::post()
            .uri(&format!("/{game_id}/respond?action=decline"))
            .to_request();
        req.extensions_mut().insert(user);

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["success"], true);
        assert_eq!(body["action"], "decline");
        assert_eq!(body["message"], "Invitation declined");
        assert!(body["player_id"].is_null());
        assert!(body["position"].is_null());
        assert!(body["player_count"].is_null());
        assert!(body["max_players"].is_null());
        assert!(body["game_status"].is_null());
    }

    #[actix_web::test]
    async fn respond_invalid_action_returns_400() {
        let mock = Arc::new(MockGameService::ok());
        let app = make_app(mock).await;
        let user = authenticated_user();
        let game_id = Uuid::new_v4();

        let req = test::TestRequest::post()
            .uri(&format!("/{game_id}/respond?action=foo"))
            .to_request();
        req.extensions_mut().insert(user);

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400);
        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["success"], false);
        assert!(body["error"].as_str().unwrap().contains("accept"));
    }

    #[actix_web::test]
    async fn respond_missing_action_returns_error() {
        let mock = Arc::new(MockGameService::ok());
        let app = make_app(mock).await;
        let user = authenticated_user();
        let game_id = Uuid::new_v4();

        let req = test::TestRequest::post()
            .uri(&format!("/{game_id}/respond"))
            .to_request();
        req.extensions_mut().insert(user);

        let resp = test::call_service(&app, req).await;
        assert!(!resp.status().is_success());
    }

    #[actix_web::test]
    async fn respond_accept_account_frozen() {
        let mock = Arc::new(MockGameService::ok());
        mock.set_accept_invite_result(Err(GameError::AccountFrozen {
            until: "2026-05-18T12:00:00+00:00".to_string(),
        }));
        let app = make_app(mock).await;
        let user = authenticated_user();
        let game_id = Uuid::new_v4();

        let req = test::TestRequest::post()
            .uri(&format!("/{game_id}/respond?action=accept"))
            .to_request();
        req.extensions_mut().insert(user);

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 403);
        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["success"], false);
        assert!(body["error"].as_str().unwrap().contains("frozen"));
    }
}
