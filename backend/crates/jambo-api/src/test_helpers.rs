use async_trait::async_trait;
use std::collections::HashSet;
use std::sync::Mutex;

use crate::api::dto::auth::{
    AuthResponse, ForgotPasswordRequest, ForgotPasswordResponse, LoginRequest, RegisterRequest,
    ResetPasswordRequest, ResetPasswordResponse, UserInfo,
};
use crate::api::dto::dashboard::{GameHistoryResponse, PaginationParams, PlayerProfileResponse};
use crate::api::dto::requests::UserSearchQuery;
use crate::api::dto::responses::{
    CashoutHistoryResponse, CashoutRequestResponse, InvitationsResponse, QuickGameResponse,
    UserSearchResponse,
};
use crate::api::services::auth_service::{
    AuthError, AuthServiceTrait, LoginResult, RegisterResult,
};
use crate::api::services::cashout_service::CashoutServiceTrait;
use crate::api::services::dashboard_service::{DashboardServiceTrait, SendInvitesParams};
use jambo_core::error::AppError;
use jambo_core::i18n::Lang;
use jambo_db::models::User;

// ── Auth service mock ───────────────────────────────────────────────────

pub struct MockAuthService {
    register_result: Mutex<Option<Result<RegisterResult, AuthError>>>,
    login_result: Mutex<Option<Result<LoginResult, AuthError>>>,
    reset_password_result: Mutex<Option<Result<ResetPasswordResponse, AuthError>>>,
    me_result: Mutex<Option<Result<UserInfo, AuthError>>>,
}

impl MockAuthService {
    pub fn new() -> Self {
        Self {
            register_result: Mutex::new(None),
            login_result: Mutex::new(None),
            reset_password_result: Mutex::new(None),
            me_result: Mutex::new(None),
        }
    }

    pub fn set_register_result(&self, r: Result<RegisterResult, AuthError>) {
        *self.register_result.lock().unwrap() = Some(r);
    }

    pub fn set_login_result(&self, r: Result<LoginResult, AuthError>) {
        *self.login_result.lock().unwrap() = Some(r);
    }

    pub fn set_reset_password_result(&self, r: Result<ResetPasswordResponse, AuthError>) {
        *self.reset_password_result.lock().unwrap() = Some(r);
    }

    pub fn set_me_result(&self, r: Result<UserInfo, AuthError>) {
        *self.me_result.lock().unwrap() = Some(r);
    }
}

impl Default for MockAuthService {
    fn default() -> Self {
        Self::new()
    }
}

fn register_result() -> RegisterResult {
    RegisterResult {
        token: "token".into(),
        response: AuthResponse {
            success: true,
            message: "ok".into(),
            user: Some(UserInfo {
                id: uuid::Uuid::now_v7(),
                pseudo: "alice".into(),
                email: "a@b.co".into(),
                language: "en".into(),
            }),
        },
    }
}

fn login_result() -> LoginResult {
    LoginResult {
        token: "token".into(),
        response: AuthResponse {
            success: true,
            message: "ok".into(),
            user: Some(UserInfo {
                id: uuid::Uuid::now_v7(),
                pseudo: "alice".into(),
                email: "a@b.co".into(),
                language: "en".into(),
            }),
        },
    }
}

#[async_trait]
impl AuthServiceTrait for MockAuthService {
    async fn register(
        &self,
        _body: RegisterRequest,
        _ip_hash: Option<String>,
        _lang: Lang,
    ) -> Result<RegisterResult, AuthError> {
        self.register_result
            .lock()
            .unwrap()
            .take()
            .unwrap_or(Ok(register_result()))
    }

    async fn login(
        &self,
        _body: LoginRequest,
        _ip_hash: Option<String>,
        _lang: Lang,
    ) -> Result<LoginResult, AuthError> {
        self.login_result
            .lock()
            .unwrap()
            .take()
            .unwrap_or(Ok(login_result()))
    }

    async fn forgot_password(
        &self,
        _body: ForgotPasswordRequest,
        _lang: Lang,
    ) -> ForgotPasswordResponse {
        ForgotPasswordResponse {
            success: true,
            message: "ok".into(),
        }
    }

    async fn reset_password(
        &self,
        _body: ResetPasswordRequest,
        _lang: Lang,
    ) -> Result<ResetPasswordResponse, AuthError> {
        self.reset_password_result
            .lock()
            .unwrap()
            .take()
            .unwrap_or(Ok(ResetPasswordResponse {
                success: true,
                message: "ok".into(),
            }))
    }

    async fn me(&self, _user_id: uuid::Uuid) -> Result<UserInfo, AuthError> {
        self.me_result
            .lock()
            .unwrap()
            .take()
            .unwrap_or(Ok(UserInfo {
                id: uuid::Uuid::now_v7(),
                pseudo: "alice".into(),
                email: "a@b.co".into(),
                language: "en".into(),
            }))
    }
}
// ── Dashboard service mock ──────────────────────────────────────────────

type ResolveResult = (Vec<uuid::Uuid>, HashSet<uuid::Uuid>, Vec<String>);

pub struct MockDashboardService {
    get_profile_result: Mutex<Option<Result<PlayerProfileResponse, AppError>>>,
    list_games_result: Mutex<Option<Result<GameHistoryResponse, AppError>>>,
    get_game_result: Mutex<Option<Result<QuickGameResponse, AppError>>>,
    get_player_id_result: Mutex<Option<Result<uuid::Uuid, AppError>>>,
    get_active_game_result: Mutex<Option<Result<QuickGameResponse, AppError>>>,
    resolve_result: Mutex<Option<Result<ResolveResult, AppError>>>,
    check_existing_players_result: Mutex<Option<Result<HashSet<uuid::Uuid>, AppError>>>,
    get_invitations_result: Mutex<Option<Result<InvitationsResponse, AppError>>>,
    search_users_result: Mutex<Option<Result<UserSearchResponse, AppError>>>,
    find_users_by_ids_result: Mutex<Option<Result<Vec<User>, AppError>>>,
}

impl MockDashboardService {
    pub fn new() -> Self {
        Self {
            get_profile_result: Mutex::new(None),
            list_games_result: Mutex::new(None),
            get_game_result: Mutex::new(None),
            get_player_id_result: Mutex::new(None),
            get_active_game_result: Mutex::new(None),
            resolve_result: Mutex::new(None),
            check_existing_players_result: Mutex::new(None),
            get_invitations_result: Mutex::new(None),
            search_users_result: Mutex::new(None),
            find_users_by_ids_result: Mutex::new(None),
        }
    }

    pub fn set_get_profile_result(&self, r: Result<PlayerProfileResponse, AppError>) {
        *self.get_profile_result.lock().unwrap() = Some(r);
    }

    pub fn set_list_games_result(&self, r: Result<GameHistoryResponse, AppError>) {
        *self.list_games_result.lock().unwrap() = Some(r);
    }

    pub fn set_get_game_result(&self, r: Result<QuickGameResponse, AppError>) {
        *self.get_game_result.lock().unwrap() = Some(r);
    }

    pub fn set_get_player_id_result(&self, r: Result<uuid::Uuid, AppError>) {
        *self.get_player_id_result.lock().unwrap() = Some(r);
    }

    pub fn set_get_active_game_result(&self, r: Result<QuickGameResponse, AppError>) {
        *self.get_active_game_result.lock().unwrap() = Some(r);
    }

    pub fn set_get_invitations_result(&self, r: Result<InvitationsResponse, AppError>) {
        *self.get_invitations_result.lock().unwrap() = Some(r);
    }

    pub fn set_resolve_result(&self, r: Result<ResolveResult, AppError>) {
        *self.resolve_result.lock().unwrap() = Some(r);
    }

    pub fn set_check_existing_players_result(&self, r: Result<HashSet<uuid::Uuid>, AppError>) {
        *self.check_existing_players_result.lock().unwrap() = Some(r);
    }

    pub fn set_search_users_result(&self, r: Result<UserSearchResponse, AppError>) {
        *self.search_users_result.lock().unwrap() = Some(r);
    }

    pub fn set_find_users_by_ids_result(&self, r: Result<Vec<User>, AppError>) {
        *self.find_users_by_ids_result.lock().unwrap() = Some(r);
    }
}

impl Default for MockDashboardService {
    fn default() -> Self {
        Self::new()
    }
}

fn default_profile() -> PlayerProfileResponse {
    PlayerProfileResponse {
        credit: 100,
        game_played: 0,
        wins: 0,
        kora_wins: 0,
        frozen_until: None,
        cashout_locked: false,
    }
}

fn default_quick_game() -> QuickGameResponse {
    QuickGameResponse {
        game_id: uuid::Uuid::now_v7(),
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
    }
}

#[async_trait]
impl DashboardServiceTrait for MockDashboardService {
    async fn get_profile(&self, _user_id: uuid::Uuid) -> Result<PlayerProfileResponse, AppError> {
        self.get_profile_result
            .lock()
            .unwrap()
            .take()
            .unwrap_or(Ok(default_profile()))
    }

    async fn list_games(
        &self,
        _user_id: uuid::Uuid,
        _query: PaginationParams,
    ) -> Result<GameHistoryResponse, AppError> {
        self.list_games_result
            .lock()
            .unwrap()
            .take()
            .unwrap_or(Ok(GameHistoryResponse {
                games: vec![],
                total: 0,
                page: 1,
                per_page: 10,
            }))
    }

    async fn get_game(
        &self,
        _user_id: uuid::Uuid,
        _game_id: uuid::Uuid,
    ) -> Result<QuickGameResponse, AppError> {
        self.get_game_result
            .lock()
            .unwrap()
            .take()
            .unwrap_or(Ok(default_quick_game()))
    }

    async fn get_player_id(
        &self,
        _user_id: uuid::Uuid,
        _game_id: uuid::Uuid,
    ) -> Result<uuid::Uuid, AppError> {
        self.get_player_id_result
            .lock()
            .unwrap()
            .take()
            .unwrap_or(Ok(uuid::Uuid::new_v4()))
    }

    async fn get_active_game(&self, _user_id: uuid::Uuid) -> Result<QuickGameResponse, AppError> {
        self.get_active_game_result
            .lock()
            .unwrap()
            .take()
            .unwrap_or(Ok(default_quick_game()))
    }

    async fn resolve_invite_user_ids(
        &self,
        _params: &SendInvitesParams,
    ) -> Result<ResolveResult, AppError> {
        self.resolve_result
            .lock()
            .unwrap()
            .take()
            .unwrap_or(Ok((vec![], HashSet::new(), vec![])))
    }

    async fn check_existing_players(
        &self,
        _game_id: uuid::Uuid,
    ) -> Result<HashSet<uuid::Uuid>, AppError> {
        self.check_existing_players_result
            .lock()
            .unwrap()
            .take()
            .unwrap_or(Ok(HashSet::new()))
    }

    async fn get_invitations(&self, _user_id: uuid::Uuid) -> Result<InvitationsResponse, AppError> {
        self.get_invitations_result
            .lock()
            .unwrap()
            .take()
            .unwrap_or(Ok(InvitationsResponse {
                invitations: vec![],
            }))
    }

    async fn search_users(&self, _query: &UserSearchQuery) -> Result<UserSearchResponse, AppError> {
        self.search_users_result
            .lock()
            .unwrap()
            .take()
            .unwrap_or(Ok(UserSearchResponse { users: vec![] }))
    }

    async fn find_users_by_ids(&self, _ids: &[uuid::Uuid]) -> Result<Vec<User>, AppError> {
        self.find_users_by_ids_result
            .lock()
            .unwrap()
            .take()
            .unwrap_or(Ok(vec![]))
    }
}

// ── Cashout service mock ────────────────────────────────────────────────

pub struct MockCashoutService {
    request_result: Mutex<Option<Result<CashoutRequestResponse, AppError>>>,
    list_result: Mutex<Option<Result<CashoutHistoryResponse, AppError>>>,
}

impl MockCashoutService {
    pub fn new() -> Self {
        Self {
            request_result: Mutex::new(None),
            list_result: Mutex::new(None),
        }
    }

    pub fn set_request_result(&self, r: Result<CashoutRequestResponse, AppError>) {
        *self.request_result.lock().unwrap() = Some(r);
    }

    pub fn set_list_result(&self, r: Result<CashoutHistoryResponse, AppError>) {
        *self.list_result.lock().unwrap() = Some(r);
    }
}

impl Default for MockCashoutService {
    fn default() -> Self {
        Self::new()
    }
}

fn default_cashout_request() -> CashoutRequestResponse {
    CashoutRequestResponse {
        id: uuid::Uuid::now_v7(),
        credits: 250,
        amount_eur_cents: 100,
        status: "requested".into(),
    }
}

#[async_trait]
impl CashoutServiceTrait for MockCashoutService {
    async fn request_cashout(
        &self,
        _user_id: uuid::Uuid,
        _credits: i32,
        _paypal_email: &str,
    ) -> Result<CashoutRequestResponse, AppError> {
        self.request_result
            .lock()
            .unwrap()
            .take()
            .unwrap_or(Ok(default_cashout_request()))
    }

    async fn list_cashouts(
        &self,
        _user_id: uuid::Uuid,
        _page: u64,
        _per_page: u64,
    ) -> Result<CashoutHistoryResponse, AppError> {
        self.list_result
            .lock()
            .unwrap()
            .take()
            .unwrap_or(Ok(CashoutHistoryResponse {
                items: vec![],
                total: 0,
                page: 1,
                per_page: 10,
            }))
    }
}
