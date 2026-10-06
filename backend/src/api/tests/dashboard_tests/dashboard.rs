use crate::api::dashboard::{
    game_state, get_active_game, get_game, get_invitations, get_profile, list_games,
    mint_spectate_token, play_game, search_users, send_invites, start_game,
};
use crate::api::services::dashboard_service::DashboardServiceTrait;
use crate::auth::config::AuthConfig;
use crate::auth::extractors::AuthenticatedUser;
use crate::error::{AppError, GameError};
use crate::game::service::{
    mock::MockGameService, GameLifecycleService, GamePlayService, InviteService,
};
use crate::i18n::Translator;
use crate::mailer::Mailer;
use crate::test_helpers::{MockDashboardService, MockMailer};
use actix_web::{test, web, App, HttpMessage};
use sea_orm::{DatabaseBackend, MockDatabase};
use std::collections::HashSet;
use std::sync::Arc;
use uuid::Uuid;

fn authenticated_user() -> AuthenticatedUser {
    AuthenticatedUser {
        user_id: Uuid::new_v4(),
        pseudo: "TestPlayer".to_string(),
    }
}

fn test_auth_config() -> AuthConfig {
    AuthConfig {
        jwt_secret: "test-secret-key-for-testing-only-1234567890".to_string(),
        jwt_expiry_hours: 24,
        ip_hash_pepper: "test-pepper".to_string(),
        frontend_url: "http://localhost:5173".to_string(),
    }
}

async fn make_full_dashboard_app(
    dash: Arc<dyn DashboardServiceTrait>,
    game: Arc<MockGameService>,
) -> impl actix_web::dev::Service<
    actix_http::Request,
    Response = actix_web::dev::ServiceResponse,
    Error = actix_web::Error,
> {
    let invite: Arc<dyn InviteService> = game.clone();
    let lifecycle: Arc<dyn GameLifecycleService> = game.clone();
    let gameplay: Arc<dyn GamePlayService> = game.clone();
    let mailer: Arc<dyn Mailer> = Arc::new(MockMailer::ok());
    test::init_service(
        App::new()
            .app_data(web::Data::new(dash))
            .app_data(web::Data::new(invite))
            .app_data(web::Data::new(lifecycle))
            .app_data(web::Data::new(gameplay))
            .app_data(web::Data::new(mailer))
            .app_data(web::Data::new(Arc::new(Translator::new())))
            .app_data(web::Data::new(test_auth_config()))
            .app_data(web::Data::new(None::<crate::messaging::RedisClient>))
            .app_data(web::Data::new(
                MockDatabase::new(DatabaseBackend::Postgres).into_connection(),
            ))
            .route("/profile", web::get().to(get_profile))
            .route("/games", web::get().to(list_games))
            .route("/games/{game_id}", web::get().to(get_game))
            .route("/active-game", web::get().to(get_active_game))
            .route("/invitations", web::get().to(get_invitations))
            .route("/users/search", web::get().to(search_users))
            .route("/games/{game_id}/invites", web::post().to(send_invites))
            .route("/games/{game_id}/start", web::post().to(start_game))
            .route("/games/{game_id}/play", web::post().to(play_game))
            .route("/games/{game_id}/me", web::get().to(game_state))
            .route(
                "/games/{game_id}/spectate-token",
                web::post().to(mint_spectate_token),
            ),
    )
    .await
}

fn quick_game_with_current_user(player_id: Uuid) -> crate::api::dto::responses::QuickGameResponse {
    use crate::api::dto::responses::PlayerInfoDto;
    crate::api::dto::responses::QuickGameResponse {
        game_id: Uuid::new_v4(),
        players: vec![PlayerInfoDto {
            id: player_id,
            player_type: "human".into(),
            name: "alice".into(),
            position: 0,
            display_position: 0,
            cards: vec![1, 2],
            cards_count: 2,
            is_current_user: true,
        }],
        status: "active".into(),
        current_turn: 0,
        bet: 10,
        max_players: 4,
        invite_expires_at: None,
        deck_slots: None,
        ws_token: None,
        step_by_step: false,
        special_cards: None,
    }
}

#[actix_web::test]
async fn get_profile_success() {
    let app = make_full_dashboard_app(
        Arc::new(MockDashboardService::new()),
        Arc::new(MockGameService::ok()),
    )
    .await;
    let user = authenticated_user();
    let req = test::TestRequest::get().uri("/profile").to_request();
    req.extensions_mut().insert(user);
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["credit"], 100);
}

#[actix_web::test]
async fn list_games_success() {
    let app = make_full_dashboard_app(
        Arc::new(MockDashboardService::new()),
        Arc::new(MockGameService::ok()),
    )
    .await;
    let user = authenticated_user();
    let req = test::TestRequest::get().uri("/games").to_request();
    req.extensions_mut().insert(user);
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn get_game_success() {
    let app = make_full_dashboard_app(
        Arc::new(MockDashboardService::new()),
        Arc::new(MockGameService::ok()),
    )
    .await;
    let user = authenticated_user();
    let game_id = Uuid::new_v4();
    let req = test::TestRequest::get()
        .uri(&format!("/games/{game_id}"))
        .to_request();
    req.extensions_mut().insert(user);
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn get_active_game_success() {
    let app = make_full_dashboard_app(
        Arc::new(MockDashboardService::new()),
        Arc::new(MockGameService::ok()),
    )
    .await;
    let user = authenticated_user();
    let req = test::TestRequest::get().uri("/active-game").to_request();
    req.extensions_mut().insert(user);
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn get_invitations_success() {
    let app = make_full_dashboard_app(
        Arc::new(MockDashboardService::new()),
        Arc::new(MockGameService::ok()),
    )
    .await;
    let user = authenticated_user();
    let req = test::TestRequest::get().uri("/invitations").to_request();
    req.extensions_mut().insert(user);
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn search_users_success() {
    let app = make_full_dashboard_app(
        Arc::new(MockDashboardService::new()),
        Arc::new(MockGameService::ok()),
    )
    .await;
    let user = authenticated_user();
    let req = test::TestRequest::get()
        .uri("/users/search?q=al")
        .to_request();
    req.extensions_mut().insert(user);
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn game_state_success() {
    let app = make_full_dashboard_app(
        Arc::new(MockDashboardService::new()),
        Arc::new(MockGameService::ok()),
    )
    .await;
    let user = authenticated_user();
    let game_id = Uuid::new_v4();
    let req = test::TestRequest::get()
        .uri(&format!("/games/{game_id}/me"))
        .to_request();
    req.extensions_mut().insert(user);
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn send_invites_success() {
    let invited_id = Uuid::new_v4();
    let dash = Arc::new(MockDashboardService::new());
    let mut seen = HashSet::new();
    seen.insert(invited_id);
    dash.set_resolve_result(Ok((vec![invited_id], seen, vec![])));
    dash.set_check_existing_players_result(Ok(HashSet::new()));
    dash.set_find_users_by_ids_result(Ok(vec![]));

    let app = make_full_dashboard_app(dash, Arc::new(MockGameService::ok())).await;
    let user = authenticated_user();
    let game_id = Uuid::new_v4();
    let req = test::TestRequest::post()
        .uri(&format!("/games/{game_id}/invites"))
        .set_json(serde_json::json!({ "user_ids": [invited_id] }))
        .to_request();
    req.extensions_mut().insert(user);
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], true);
}

#[actix_web::test]
async fn send_invites_duplicates_returns_400() {
    let dash = Arc::new(MockDashboardService::new());
    dash.set_resolve_result(Ok((vec![], HashSet::new(), vec!["dup".to_string()])));
    let app = make_full_dashboard_app(dash, Arc::new(MockGameService::ok())).await;
    let user = authenticated_user();
    let game_id = Uuid::new_v4();
    let req = test::TestRequest::post()
        .uri(&format!("/games/{game_id}/invites"))
        .set_json(serde_json::json!({ "user_ids": [], "pseudos": [] }))
        .to_request();
    req.extensions_mut().insert(user);
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
}

#[actix_web::test]
async fn send_invites_self_returns_400() {
    let user = authenticated_user();
    let dash = Arc::new(MockDashboardService::new());
    let mut seen = HashSet::new();
    seen.insert(user.user_id);
    dash.set_resolve_result(Ok((vec![], seen, vec![])));
    let app = make_full_dashboard_app(dash, Arc::new(MockGameService::ok())).await;
    let game_id = Uuid::new_v4();
    let req = test::TestRequest::post()
        .uri(&format!("/games/{game_id}/invites"))
        .set_json(serde_json::json!({ "user_ids": [], "pseudos": [] }))
        .to_request();
    req.extensions_mut().insert(user);
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
}

#[actix_web::test]
async fn send_invites_already_in_returns_409() {
    let invited_id = Uuid::new_v4();
    let dash = Arc::new(MockDashboardService::new());
    let mut seen = HashSet::new();
    seen.insert(invited_id);
    dash.set_resolve_result(Ok((vec![invited_id], seen, vec![])));
    let mut existing = HashSet::new();
    existing.insert(invited_id);
    dash.set_check_existing_players_result(Ok(existing));
    let app = make_full_dashboard_app(dash, Arc::new(MockGameService::ok())).await;
    let user = authenticated_user();
    let game_id = Uuid::new_v4();
    let req = test::TestRequest::post()
        .uri(&format!("/games/{game_id}/invites"))
        .set_json(serde_json::json!({ "user_ids": [invited_id] }))
        .to_request();
    req.extensions_mut().insert(user);
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 409);
}

#[actix_web::test]
async fn send_invites_no_valid_users_returns_200() {
    let dash = Arc::new(MockDashboardService::new());
    dash.set_resolve_result(Ok((vec![], HashSet::new(), vec![])));
    dash.set_check_existing_players_result(Ok(HashSet::new()));
    let app = make_full_dashboard_app(dash, Arc::new(MockGameService::ok())).await;
    let user = authenticated_user();
    let game_id = Uuid::new_v4();
    let req = test::TestRequest::post()
        .uri(&format!("/games/{game_id}/invites"))
        .set_json(serde_json::json!({ "user_ids": [], "pseudos": [] }))
        .to_request();
    req.extensions_mut().insert(user);
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn start_game_success() {
    let dash = Arc::new(MockDashboardService::new());
    let app = make_full_dashboard_app(dash, Arc::new(MockGameService::ok())).await;
    let user = authenticated_user();
    let game_id = Uuid::new_v4();
    let req = test::TestRequest::post()
        .uri(&format!("/games/{game_id}/start"))
        .to_request();
    req.extensions_mut().insert(user);
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn play_game_success() {
    let player_id = Uuid::new_v4();
    let dash = Arc::new(MockDashboardService::new());
    dash.set_get_game_result(Ok(quick_game_with_current_user(player_id)));
    let app = make_full_dashboard_app(dash, Arc::new(MockGameService::ok())).await;
    let user = authenticated_user();
    let game_id = Uuid::new_v4();
    let req = test::TestRequest::post()
        .uri(&format!("/games/{game_id}/play"))
        .set_json(serde_json::json!({ "player_id": player_id, "card_index": 0 }))
        .to_request();
    req.extensions_mut().insert(user);
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], true);
}

#[actix_web::test]
async fn mint_spectate_token_non_participant_returns_403() {
    let dash = Arc::new(MockDashboardService::new());
    dash.set_check_existing_players_result(Ok(HashSet::new()));
    let app = make_full_dashboard_app(dash, Arc::new(MockGameService::ok())).await;
    let user = authenticated_user();
    let game_id = Uuid::new_v4();
    let req = test::TestRequest::post()
        .uri(&format!("/games/{game_id}/spectate-token"))
        .to_request();
    req.extensions_mut().insert(user);
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);
}

#[actix_web::test]
async fn mint_spectate_token_success() {
    let user = authenticated_user();
    let dash = Arc::new(MockDashboardService::new());
    let mut participants = HashSet::new();
    participants.insert(user.user_id);
    dash.set_check_existing_players_result(Ok(participants));
    let app = make_full_dashboard_app(dash, Arc::new(MockGameService::ok())).await;
    let game_id = Uuid::new_v4();
    let req = test::TestRequest::post()
        .uri(&format!("/games/{game_id}/spectate-token"))
        .to_request();
    req.extensions_mut().insert(user);
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert!(body["token"].is_string());
    assert!(body["url"].is_string());
}

#[actix_web::test]
async fn play_game_not_player_returns_403() {
    let dash = Arc::new(MockDashboardService::new());
    let game = crate::api::dto::responses::QuickGameResponse {
        game_id: Uuid::new_v4(),
        players: vec![],
        status: "active".into(),
        current_turn: 0,
        bet: 10,
        max_players: 4,
        invite_expires_at: None,
        deck_slots: None,
        ws_token: None,
        step_by_step: false,
        special_cards: None,
    };
    dash.set_get_game_result(Ok(game));
    let app = make_full_dashboard_app(dash, Arc::new(MockGameService::ok())).await;
    let user = authenticated_user();
    let game_id = Uuid::new_v4();
    let req = test::TestRequest::post()
        .uri(&format!("/games/{game_id}/play"))
        .set_json(serde_json::json!({ "player_id": Uuid::new_v4(), "card_index": 0 }))
        .to_request();
    req.extensions_mut().insert(user);
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);
}

fn db_err() -> AppError {
    AppError::Database(sea_orm::DbErr::Custom("db down".to_string()))
}

#[actix_web::test]
async fn get_profile_error_returns_500() {
    let dash = Arc::new(MockDashboardService::new());
    dash.set_get_profile_result(Err(db_err()));
    let app = make_full_dashboard_app(dash, Arc::new(MockGameService::ok())).await;
    let user = authenticated_user();
    let req = test::TestRequest::get().uri("/profile").to_request();
    req.extensions_mut().insert(user);
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 500);
}

#[actix_web::test]
async fn list_games_error_returns_500() {
    let dash = Arc::new(MockDashboardService::new());
    dash.set_list_games_result(Err(db_err()));
    let app = make_full_dashboard_app(dash, Arc::new(MockGameService::ok())).await;
    let user = authenticated_user();
    let req = test::TestRequest::get().uri("/games").to_request();
    req.extensions_mut().insert(user);
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 500);
}

#[actix_web::test]
async fn get_game_error_returns_error() {
    let dash = Arc::new(MockDashboardService::new());
    dash.set_get_game_result(Err(AppError::NotFound("game.not_found")));
    let app = make_full_dashboard_app(dash, Arc::new(MockGameService::ok())).await;
    let user = authenticated_user();
    let game_id = Uuid::new_v4();
    let req = test::TestRequest::get()
        .uri(&format!("/games/{game_id}"))
        .to_request();
    req.extensions_mut().insert(user);
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);
}

#[actix_web::test]
async fn get_active_game_error_returns_404() {
    let dash = Arc::new(MockDashboardService::new());
    dash.set_get_active_game_result(Err(AppError::NotFound("game.no_active_game")));
    let app = make_full_dashboard_app(dash, Arc::new(MockGameService::ok())).await;
    let user = authenticated_user();
    let req = test::TestRequest::get().uri("/active-game").to_request();
    req.extensions_mut().insert(user);
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);
}

#[actix_web::test]
async fn get_invitations_error_returns_500() {
    let dash = Arc::new(MockDashboardService::new());
    dash.set_get_invitations_result(Err(db_err()));
    let app = make_full_dashboard_app(dash, Arc::new(MockGameService::ok())).await;
    let user = authenticated_user();
    let req = test::TestRequest::get().uri("/invitations").to_request();
    req.extensions_mut().insert(user);
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 500);
}

#[actix_web::test]
async fn search_users_error_returns_500() {
    let dash = Arc::new(MockDashboardService::new());
    dash.set_search_users_result(Err(db_err()));
    let app = make_full_dashboard_app(dash, Arc::new(MockGameService::ok())).await;
    let user = authenticated_user();
    let req = test::TestRequest::get()
        .uri("/users/search?q=al")
        .to_request();
    req.extensions_mut().insert(user);
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 500);
}

#[actix_web::test]
async fn start_game_service_error_returns_error() {
    let dash = Arc::new(MockDashboardService::new());
    let game = Arc::new(MockGameService::ok());
    game.set_start_game_result(Err(GameError::NotYourTurn));
    let app = make_full_dashboard_app(dash, game).await;
    let user = authenticated_user();
    let game_id = Uuid::new_v4();
    let req = test::TestRequest::post()
        .uri(&format!("/games/{game_id}/start"))
        .to_request();
    req.extensions_mut().insert(user);
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);
}

#[actix_web::test]
async fn play_game_get_game_error_returns_error() {
    let dash = Arc::new(MockDashboardService::new());
    dash.set_get_game_result(Err(AppError::NotFound("game.not_found")));
    let app = make_full_dashboard_app(dash, Arc::new(MockGameService::ok())).await;
    let user = authenticated_user();
    let game_id = Uuid::new_v4();
    let req = test::TestRequest::post()
        .uri(&format!("/games/{game_id}/play"))
        .set_json(serde_json::json!({ "player_id": Uuid::new_v4(), "card_index": 0 }))
        .to_request();
    req.extensions_mut().insert(user);
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);
}
