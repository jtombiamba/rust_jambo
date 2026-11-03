use async_trait::async_trait;
use std::sync::Mutex;

use jambo_core::error::auth::AuthError;
use jambo_core::i18n::Lang;
use jambo_core::payment::{CaptureResult, OrderCreated, PaymentServiceTrait};
use jambo_domain::room::dto::{
    ActiveRunResponse, CreateRunResponse, CurrentGameResponse, JoinRunResponse, RoomDetailResponse,
    RoomListItem, RunListItem, StartNextGameResponse,
};
use jambo_domain::room::error::RoomServiceError;
use jambo_domain::room::service_trait::RoomServiceTrait;
use jambo_mailer::Mailer;

/// Configurable `Mailer` mock used across API handler tests.
///
/// All send methods return the single shared `result` value (defaulting to
/// `Ok(())`), so a test only needs to configure the one method it exercises.
pub struct MockMailer {
    result: Mutex<Option<Result<(), String>>>,
}

impl MockMailer {
    pub fn ok() -> Self {
        Self {
            result: Mutex::new(Some(Ok(()))),
        }
    }

    pub fn with_result(result: Result<(), String>) -> Self {
        Self {
            result: Mutex::new(Some(result)),
        }
    }
}

#[async_trait]
impl Mailer for MockMailer {
    async fn send_password_reset(
        &self,
        _to_email: &str,
        _reset_link: &str,
        _lang: Lang,
    ) -> Result<(), String> {
        self.result.lock().unwrap().take().unwrap_or(Ok(()))
    }

    async fn send_invitation(
        &self,
        _to_email: &str,
        _inviter_name: &str,
        _game_id: &str,
        _lang: Lang,
    ) -> Result<(), String> {
        self.result.lock().unwrap().take().unwrap_or(Ok(()))
    }

    async fn send_freeze_expired(
        &self,
        _to_email: &str,
        _credit: i32,
        _lang: Lang,
    ) -> Result<(), String> {
        self.result.lock().unwrap().take().unwrap_or(Ok(()))
    }

    async fn send_contact_form(
        &self,
        _name: &str,
        _email: &str,
        _subject: &str,
        _message: &str,
        _lang: Lang,
    ) -> Result<(), String> {
        self.result.lock().unwrap().take().unwrap_or(Ok(()))
    }

    async fn send_stall_warning(
        &self,
        _to_email: &str,
        _game_id: &str,
        _inactive_minutes: i64,
        _remaining_minutes: i64,
        _lang: Lang,
    ) -> Result<(), String> {
        self.result.lock().unwrap().take().unwrap_or(Ok(()))
    }

    async fn send_stall_kicked(
        &self,
        _to_email: &str,
        _game_id: &str,
        _bet: i32,
        _lang: Lang,
    ) -> Result<(), String> {
        self.result.lock().unwrap().take().unwrap_or(Ok(()))
    }

    async fn send_room_invitation(
        &self,
        _to_email: &str,
        _inviter_name: &str,
        _room_name: &str,
        _invitation_code: &str,
        _lang: Lang,
    ) -> Result<(), String> {
        self.result.lock().unwrap().take().unwrap_or(Ok(()))
    }

    async fn send_cashout_requested(
        &self,
        _to_email: &str,
        _credits: i32,
        _amount_eur_cents: i32,
        _paypal_email: &str,
        _lang: Lang,
    ) -> Result<(), String> {
        self.result.lock().unwrap().take().unwrap_or(Ok(()))
    }

    async fn send_cashout_rejected(
        &self,
        _to_email: &str,
        _credits: i32,
        _amount_eur_cents: i32,
        _lang: Lang,
    ) -> Result<(), String> {
        self.result.lock().unwrap().take().unwrap_or(Ok(()))
    }

    async fn send_cashout_admin_alert(
        &self,
        _to_email: &str,
        _pseudo: &str,
        _email: &str,
        _credits: i32,
        _amount_eur_cents: i32,
        _paypal_email: &str,
    ) -> Result<(), String> {
        self.result.lock().unwrap().take().unwrap_or(Ok(()))
    }
}
// ── Room service mock ───────────────────────────────────────────────────

type RoomModel = jambo_db::models::room::Model;

pub struct MockRoomService {
    create_room_result: Mutex<Option<Result<RoomModel, RoomServiceError>>>,
    list_user_rooms_result: Mutex<Option<Result<Vec<RoomListItem>, RoomServiceError>>>,
    get_room_detail_result: Mutex<Option<Result<RoomDetailResponse, RoomServiceError>>>,
    join_room_result: Mutex<Option<Result<RoomModel, RoomServiceError>>>,
    invite_to_room_result: Mutex<Option<Result<(), RoomServiceError>>>,
    leave_room_result: Mutex<Option<Result<(), RoomServiceError>>>,
    create_run_result: Mutex<Option<Result<CreateRunResponse, RoomServiceError>>>,
    join_run_result: Mutex<Option<Result<JoinRunResponse, RoomServiceError>>>,
    leave_run_result: Mutex<Option<Result<(), RoomServiceError>>>,
    get_active_run_result: Mutex<Option<Result<ActiveRunResponse, RoomServiceError>>>,
    start_next_game_result: Mutex<Option<Result<StartNextGameResponse, RoomServiceError>>>,
    get_current_game_result: Mutex<Option<Result<CurrentGameResponse, RoomServiceError>>>,
    list_runs_result: Mutex<Option<Result<Vec<RunListItem>, RoomServiceError>>>,
    is_run_participant_result: Mutex<Option<Result<bool, RoomServiceError>>>,
}

impl MockRoomService {
    pub fn new() -> Self {
        Self {
            create_room_result: Mutex::new(None),
            list_user_rooms_result: Mutex::new(None),
            get_room_detail_result: Mutex::new(None),
            join_room_result: Mutex::new(None),
            invite_to_room_result: Mutex::new(None),
            leave_room_result: Mutex::new(None),
            create_run_result: Mutex::new(None),
            join_run_result: Mutex::new(None),
            leave_run_result: Mutex::new(None),
            get_active_run_result: Mutex::new(None),
            start_next_game_result: Mutex::new(None),
            get_current_game_result: Mutex::new(None),
            list_runs_result: Mutex::new(None),
            is_run_participant_result: Mutex::new(None),
        }
    }

    pub fn set_create_room_result(&self, r: Result<RoomModel, RoomServiceError>) {
        *self.create_room_result.lock().unwrap() = Some(r);
    }

    pub fn set_get_room_detail_result(&self, r: Result<RoomDetailResponse, RoomServiceError>) {
        *self.get_room_detail_result.lock().unwrap() = Some(r);
    }
}

impl Default for MockRoomService {
    fn default() -> Self {
        Self::new()
    }
}

fn default_room() -> RoomModel {
    let now = chrono::Utc::now();
    RoomModel {
        id: uuid::Uuid::now_v7(),
        name: "Room".into(),
        creator_id: uuid::Uuid::now_v7(),
        invitation_code: "ABC123".into(),
        created_at: now,
        updated_at: now,
    }
}

#[async_trait]
impl RoomServiceTrait for MockRoomService {
    async fn create_room(
        &self,
        _user_id: uuid::Uuid,
        _name: &str,
    ) -> Result<RoomModel, RoomServiceError> {
        self.create_room_result
            .lock()
            .unwrap()
            .take()
            .unwrap_or(Ok(default_room()))
    }

    async fn list_user_rooms(
        &self,
        _user_id: uuid::Uuid,
    ) -> Result<Vec<RoomListItem>, RoomServiceError> {
        self.list_user_rooms_result
            .lock()
            .unwrap()
            .take()
            .unwrap_or(Ok(vec![]))
    }

    async fn get_room_detail(
        &self,
        _room_id: uuid::Uuid,
        _user_id: uuid::Uuid,
    ) -> Result<RoomDetailResponse, RoomServiceError> {
        self.get_room_detail_result
            .lock()
            .unwrap()
            .take()
            .unwrap_or_else(|| {
                Ok(RoomDetailResponse {
                    id: uuid::Uuid::now_v7(),
                    name: "Room".into(),
                    creator_id: uuid::Uuid::now_v7(),
                    invitation_code: "ABC123".into(),
                    created_at: chrono::Utc::now().to_rfc3339(),
                    members: vec![],
                    member_count: 0,
                    active_run: None,
                })
            })
    }

    async fn join_room(
        &self,
        _user_id: uuid::Uuid,
        _invitation_code: &str,
    ) -> Result<RoomModel, RoomServiceError> {
        self.join_room_result
            .lock()
            .unwrap()
            .take()
            .unwrap_or(Ok(default_room()))
    }

    async fn invite_to_room(
        &self,
        _room_id: uuid::Uuid,
        _user_id: uuid::Uuid,
        _email: &str,
    ) -> Result<(), RoomServiceError> {
        self.invite_to_room_result
            .lock()
            .unwrap()
            .take()
            .unwrap_or(Ok(()))
    }

    async fn leave_room(
        &self,
        _room_id: uuid::Uuid,
        _user_id: uuid::Uuid,
    ) -> Result<(), RoomServiceError> {
        self.leave_room_result
            .lock()
            .unwrap()
            .take()
            .unwrap_or(Ok(()))
    }

    async fn create_run(
        &self,
        _room_id: uuid::Uuid,
        _user_id: uuid::Uuid,
        _num_games: i32,
        _bet: i32,
        _player_ids: &[uuid::Uuid],
    ) -> Result<CreateRunResponse, RoomServiceError> {
        self.create_run_result
            .lock()
            .unwrap()
            .take()
            .unwrap_or(Ok(CreateRunResponse {
                run_id: uuid::Uuid::now_v7(),
                room_id: uuid::Uuid::now_v7(),
                num_games: 1,
                bet_per_game: 10,
                num_players: 1,
                status: "active".into(),
            }))
    }

    async fn join_run(
        &self,
        _run_id: uuid::Uuid,
        _user_id: uuid::Uuid,
    ) -> Result<JoinRunResponse, RoomServiceError> {
        self.join_run_result
            .lock()
            .unwrap()
            .take()
            .unwrap_or(Ok(JoinRunResponse {
                run_id: uuid::Uuid::now_v7(),
                provisioned_credits: 100,
                profile_credit_remaining: 400,
            }))
    }

    async fn leave_run(
        &self,
        _run_id: uuid::Uuid,
        _user_id: uuid::Uuid,
    ) -> Result<(), RoomServiceError> {
        self.leave_run_result
            .lock()
            .unwrap()
            .take()
            .unwrap_or(Ok(()))
    }

    async fn get_active_run(
        &self,
        _room_id: uuid::Uuid,
        _user_id: uuid::Uuid,
    ) -> Result<ActiveRunResponse, RoomServiceError> {
        self.get_active_run_result
            .lock()
            .unwrap()
            .take()
            .unwrap_or(Ok(ActiveRunResponse {
                id: uuid::Uuid::now_v7(),
                room_id: uuid::Uuid::now_v7(),
                num_games: 1,
                bet_per_game: 10,
                current_game_index: 0,
                status: jambo_db::models::RunStatus::Active,
                players: vec![],
                games: vec![],
            }))
    }

    async fn start_next_game(
        &self,
        _run_id: uuid::Uuid,
        _user_id: uuid::Uuid,
    ) -> Result<StartNextGameResponse, RoomServiceError> {
        self.start_next_game_result
            .lock()
            .unwrap()
            .take()
            .unwrap_or(Ok(StartNextGameResponse {
                game_id: uuid::Uuid::now_v7(),
                game_index: 1,
                total_games: 1,
                current_game_index: 1,
                all_games_created: None,
                status: None,
            }))
    }

    async fn get_current_game(
        &self,
        _run_id: uuid::Uuid,
        _user_id: uuid::Uuid,
    ) -> Result<CurrentGameResponse, RoomServiceError> {
        self.get_current_game_result
            .lock()
            .unwrap()
            .take()
            .unwrap_or(Ok(CurrentGameResponse {
                run_id: uuid::Uuid::now_v7(),
                game_id: uuid::Uuid::now_v7(),
                game_index: 0,
                status: jambo_db::models::RunStatus::Active,
            }))
    }

    async fn list_runs(
        &self,
        _room_id: uuid::Uuid,
        _user_id: uuid::Uuid,
    ) -> Result<Vec<RunListItem>, RoomServiceError> {
        self.list_runs_result
            .lock()
            .unwrap()
            .take()
            .unwrap_or(Ok(vec![]))
    }

    async fn is_run_participant(
        &self,
        _run_id: uuid::Uuid,
        _user_id: uuid::Uuid,
    ) -> Result<bool, RoomServiceError> {
        self.is_run_participant_result
            .lock()
            .unwrap()
            .take()
            .unwrap_or(Ok(true))
    }
}

// ── Payment service mock ────────────────────────────────────────────────

pub struct MockPaymentService {
    configured: bool,
    create_order_result: Mutex<Option<Result<OrderCreated, String>>>,
    create_topup_order_result: Mutex<Option<Result<OrderCreated, String>>>,
    capture_order_result: Mutex<Option<Result<CaptureResult, String>>>,
}

impl MockPaymentService {
    pub fn configured() -> Self {
        Self {
            configured: true,
            create_order_result: Mutex::new(None),
            create_topup_order_result: Mutex::new(None),
            capture_order_result: Mutex::new(None),
        }
    }

    pub fn unconfigured() -> Self {
        Self {
            configured: false,
            create_order_result: Mutex::new(None),
            create_topup_order_result: Mutex::new(None),
            capture_order_result: Mutex::new(None),
        }
    }

    pub fn set_capture_order_result(&self, r: Result<CaptureResult, String>) {
        *self.capture_order_result.lock().unwrap() = Some(r);
    }

    pub fn set_create_order_result(&self, r: Result<OrderCreated, String>) {
        *self.create_order_result.lock().unwrap() = Some(r);
    }

    pub fn set_create_topup_order_result(&self, r: Result<OrderCreated, String>) {
        *self.create_topup_order_result.lock().unwrap() = Some(r);
    }
}

fn default_order() -> OrderCreated {
    OrderCreated {
        order_id: "ORDER_1".into(),
        approval_url: "https://approve".into(),
    }
}

#[async_trait]
impl PaymentServiceTrait for MockPaymentService {
    fn is_configured(&self) -> bool {
        self.configured
    }

    async fn create_order(
        &self,
        _return_url: &str,
        _cancel_url: &str,
    ) -> Result<OrderCreated, String> {
        self.create_order_result
            .lock()
            .unwrap()
            .take()
            .unwrap_or(Ok(default_order()))
    }

    async fn create_topup_order(
        &self,
        _return_url: &str,
        _cancel_url: &str,
    ) -> Result<OrderCreated, String> {
        self.create_topup_order_result
            .lock()
            .unwrap()
            .take()
            .unwrap_or(Ok(default_order()))
    }

    async fn capture_order(
        &self,
        _order_id: &str,
        _idempotency_key: Option<&str>,
    ) -> Result<CaptureResult, String> {
        self.capture_order_result
            .lock()
            .unwrap()
            .take()
            .unwrap_or(Ok(CaptureResult {
                success: true,
                order_id: "ORDER_1".into(),
            }))
    }

    async fn create_payout(
        &self,
        _paypal_email: &str,
        _amount_eur_cents: i32,
        _item_id: &str,
    ) -> Result<String, String> {
        Ok("batch-id".into())
    }
}

// ── Auth error convenience constructors ─────────────────────────────────

#[allow(dead_code)]
pub fn auth_validation_err() -> AuthError {
    AuthError::Validation {
        key: "auth.email_invalid",
        field: Some("email".into()),
    }
}

#[allow(dead_code)]
pub fn auth_conflict_err() -> AuthError {
    AuthError::Conflict {
        key: "auth.email_in_use",
        field: Some("email".into()),
    }
}

#[allow(dead_code)]
pub fn auth_unauthorized_err() -> AuthError {
    AuthError::Unauthorized {
        key: "auth.invalid_credentials",
    }
}

#[allow(dead_code)]
pub fn auth_not_found_err() -> AuthError {
    AuthError::NotFound {
        key: "auth.user_not_found",
    }
}
