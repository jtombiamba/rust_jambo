use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use sea_orm::{ActiveValue, ColumnTrait, EntityTrait, QueryFilter, Set, TransactionTrait};

use crate::api::dto::responses::ApiErrorResponse;
use crate::config::Config;
use crate::database::models::{game, player, GameMode, GameStatus};
use crate::database::repositories::{
    GameRunEventRepository, GameRunGameRepository, GameRunPlayerRepository, GameRunRepository,
    PlayerProfileRepository, PlayerRepository, RoomMemberRepository, RoomRepository,
    UserRepository,
};
use crate::game::constants::KORA_CREDIT_MULTIPLIER;
use crate::i18n::Lang;
use crate::mailer::Mailer;
use crate::messaging::events::RoomEvent;
use crate::messaging::redis::PublishResult;
use crate::messaging::RedisClient;

const EMAIL_CHANNEL_CAPACITY: usize = 256;

struct EmailTask {
    email: String,
    pseudo: String,
    room_name: String,
    code: String,
    lang: Lang,
}

#[derive(Debug, thiserror::Error)]
pub enum RoomServiceError {
    #[error("Room not found")]
    RoomNotFound,
    #[error("User is not a member of this room")]
    NotMember,
    #[error("User is already a member of this room")]
    AlreadyMember,
    #[error("Invalid invitation code")]
    InvalidCode,
    #[error("A game run is already active in this room")]
    RunAlreadyActive,
    #[error("Game run not found")]
    RunNotFound,
    #[error("Not part of this game run")]
    NotRunPlayer,
    #[error("Insufficient credits: need {required} but have {current}")]
    InsufficientCredits { required: i32, current: i32 },
    #[error("User not found")]
    UserNotFound,
    #[error("Database error: {0}")]
    Database(#[from] sea_orm::DbErr),
    #[error("Internal error: {0}")]
    Internal(String),
    #[error("Account frozen")]
    AccountFrozen,
    #[error("Game not found")]
    GameNotFound,
    #[error("Not enough players to start")]
    NotEnoughPlayers,
    #[error("All games in run have been played")]
    RunCompleted,
    #[error("Game run is not active (current status: {status})")]
    RunNotActive { status: String },
    #[error("Room is full (max {max} players)")]
    RoomFull { max: i32 },
    #[error("Too many players (max {max})")]
    TooManyPlayers { max: i32 },
    #[error("Must leave active run before leaving room")]
    LeaveBlockedByRun,
    #[error("Name is required")]
    NameRequired,
    #[error("Player profile not found")]
    ProfileNotFound,
    #[error("Cannot leave run while a game is in progress")]
    GameInProgress,
    #[error("Game is already being started in this run")]
    StartAlreadyInProgress,
}

impl RoomServiceError {
    pub fn source(&self) -> &'static str {
        match self {
            RoomServiceError::RoomNotFound => "room:room_not_found",
            RoomServiceError::NotMember => "room:not_member",
            RoomServiceError::AlreadyMember => "room:already_member",
            RoomServiceError::InvalidCode => "room:invalid_code",
            RoomServiceError::RunAlreadyActive => "room:run_already_active",
            RoomServiceError::RunNotFound => "room:run_not_found",
            RoomServiceError::NotRunPlayer => "room:not_run_player",
            RoomServiceError::InsufficientCredits { .. } => "room:insufficient_credits",
            RoomServiceError::UserNotFound => "room:user_not_found",
            RoomServiceError::Database(_) => "room:database",
            RoomServiceError::Internal(_) => "room:internal",
            RoomServiceError::AccountFrozen => "room:account_frozen",
            RoomServiceError::GameNotFound => "room:game_not_found",
            RoomServiceError::NotEnoughPlayers => "room:not_enough_players",
            RoomServiceError::RunCompleted => "room:run_completed",
            RoomServiceError::RunNotActive { .. } => "room:run_not_active",
            RoomServiceError::RoomFull { .. } => "room:room_full",
            RoomServiceError::TooManyPlayers { .. } => "room:too_many_players",
            RoomServiceError::LeaveBlockedByRun => "room:leave_blocked_by_run",
            RoomServiceError::NameRequired => "room:name_required",
            RoomServiceError::ProfileNotFound => "room:profile_not_found",
            RoomServiceError::GameInProgress => "room:game_in_progress",
            RoomServiceError::StartAlreadyInProgress => "room:start_already_in_progress",
        }
    }
}

impl actix_web::ResponseError for RoomServiceError {
    fn status_code(&self) -> actix_web::http::StatusCode {
        use actix_web::http::StatusCode;
        match self {
            RoomServiceError::RoomNotFound
            | RoomServiceError::RunNotFound
            | RoomServiceError::ProfileNotFound => StatusCode::NOT_FOUND,
            RoomServiceError::NotMember | RoomServiceError::NotRunPlayer => StatusCode::FORBIDDEN,
            RoomServiceError::AlreadyMember
            | RoomServiceError::RunAlreadyActive
            | RoomServiceError::LeaveBlockedByRun
            | RoomServiceError::RoomFull { .. }
            | RoomServiceError::GameInProgress
            | RoomServiceError::StartAlreadyInProgress => StatusCode::CONFLICT,
            RoomServiceError::InsufficientCredits { .. } => StatusCode::PAYMENT_REQUIRED,
            RoomServiceError::InvalidCode
            | RoomServiceError::NameRequired
            | RoomServiceError::NotEnoughPlayers
            | RoomServiceError::TooManyPlayers { .. } => StatusCode::BAD_REQUEST,
            RoomServiceError::AccountFrozen => StatusCode::FORBIDDEN,
            RoomServiceError::RunCompleted | RoomServiceError::RunNotActive { .. } => {
                StatusCode::BAD_REQUEST
            }
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn error_response(&self) -> actix_web::HttpResponse {
        let status = self.status_code();
        let is_server_error = status.is_server_error();
        let msg = if is_server_error {
            "Internal server error".to_string()
        } else {
            self.to_string()
        };
        let request_id = crate::observability::CORRELATION_ID
            .try_with(|id| id.to_string())
            .ok();
        if is_server_error {
            tracing::error!(error = ?self, request_id = ?request_id, "Room service error occurred");
        }
        actix_web::HttpResponse::build(status).json(ApiErrorResponse {
            success: false,
            error: msg,
            field: None,
            source: self.source().to_string(),
            request_id,
        })
    }
}

pub struct RoomService {
    db: sea_orm::DatabaseConnection,
    room_repo: RoomRepository,
    member_repo: RoomMemberRepository,
    run_repo: GameRunRepository,
    run_player_repo: GameRunPlayerRepository,
    run_game_repo: GameRunGameRepository,
    run_event_repo: GameRunEventRepository,
    profile_repo: PlayerProfileRepository,
    #[allow(dead_code)]
    player_repo: PlayerRepository,
    user_repo: UserRepository,
    mailer: Arc<dyn Mailer>,
    config: Config,
    redis_client: Option<RedisClient>,
    start_game_locks: tokio::sync::Mutex<HashMap<Uuid, Arc<tokio::sync::Mutex<()>>>>,
    email_tx: tokio::sync::mpsc::Sender<EmailTask>,
    email_rx: tokio::sync::Mutex<Option<tokio::sync::mpsc::Receiver<EmailTask>>>,
}

impl RoomService {
    pub fn new(
        db: sea_orm::DatabaseConnection,
        mailer: Arc<dyn Mailer>,
        config: Config,
        redis_client: Option<RedisClient>,
    ) -> Self {
        let (email_tx, email_rx) = tokio::sync::mpsc::channel(EMAIL_CHANNEL_CAPACITY);
        Self {
            db: db.clone(),
            room_repo: RoomRepository::new(db.clone()),
            member_repo: RoomMemberRepository::new(db.clone()),
            run_repo: GameRunRepository::new(db.clone()),
            run_player_repo: GameRunPlayerRepository::new(db.clone()),
            run_game_repo: GameRunGameRepository::new(db.clone()),
            run_event_repo: GameRunEventRepository::new(db.clone()),
            profile_repo: PlayerProfileRepository::new(db.clone()),
            player_repo: PlayerRepository::new(db.clone()),
            user_repo: UserRepository::new(db.clone(), config.default_credit),
            mailer,
            config,
            redis_client,
            start_game_locks: tokio::sync::Mutex::new(HashMap::new()),
            email_tx,
            email_rx: tokio::sync::Mutex::new(Some(email_rx)),
        }
    }

    pub async fn start_email_consumer(self: &Arc<Self>) {
        let Some(mut rx) = self.email_rx.lock().await.take() else {
            tracing::warn!("Email consumer already started");
            return;
        };
        let mailer = self.mailer.clone();
        tokio::spawn(async move {
            while let Some(task) = rx.recv().await {
                if let Err(e) = mailer
                    .send_room_invitation(
                        &task.email,
                        &task.pseudo,
                        &task.room_name,
                        &task.code,
                        task.lang,
                    )
                    .await
                {
                    tracing::error!("Failed to send room invitation to {}: {}", task.email, e);
                }
            }
            tracing::info!("Email consumer shutting down");
        });
    }

    fn generate_invitation_code() -> String {
        use rand::Rng;
        let chars: Vec<char> = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789".chars().collect();
        let mut rng = rand::thread_rng();
        (0..8)
            .map(|_| chars[rng.gen_range(0..chars.len())])
            .collect()
    }

    async fn publish_event(&self, event: &RoomEvent) {
        if let Some(mut redis) = self.redis_client.clone() {
            match redis.publish_room_event_with_retry(event).await {
                PublishResult::Published => {}
                PublishResult::RetryExhausted(e) => {
                    tracing::error!("Failed to publish room event after retries: {}", e);
                }
            }
        }
    }

    async fn acquire_start_game_lock(&self, run_id: Uuid) -> Result<(), RoomServiceError> {
        if let Some(mut redis) = self.redis_client.clone() {
            let key = format!("start_game_lock:{}", run_id);
            let instance_id = Uuid::now_v7().to_string();
            let acquired = redis
                .set_nx_ex(&key, &instance_id, 30)
                .await
                .map_err(|e| RoomServiceError::Internal(format!("Redis lock error: {}", e)))?;
            if !acquired {
                return Err(RoomServiceError::StartAlreadyInProgress);
            }
            return Ok(());
        }
        Ok(())
    }

    async fn release_start_game_lock(&self, run_id: Uuid) {
        if let Some(mut redis) = self.redis_client.clone() {
            let key = format!("start_game_lock:{}", run_id);
            let _ = redis.del(&key).await;
        }
    }

    pub async fn create_room(
        &self,
        user_id: Uuid,
        name: &str,
    ) -> Result<crate::database::models::room::Model, RoomServiceError> {
        if name.trim().is_empty() {
            return Err(RoomServiceError::NameRequired);
        }

        let code = Self::generate_invitation_code();
        let room = self.room_repo.create(user_id, name.trim(), &code).await?;

        self.member_repo.create(room.id, user_id).await?;

        Ok(room)
    }

    pub async fn list_user_rooms(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<serde_json::Value>, RoomServiceError> {
        let rooms = self.room_repo.list_by_user(user_id).await?;

        if rooms.is_empty() {
            return Ok(vec![]);
        }

        let room_ids: Vec<Uuid> = rooms.iter().map(|r| r.id).collect();
        let counts = self.member_repo.count_by_rooms(&room_ids).await?;

        let result = rooms
            .iter()
            .map(|room| {
                let member_count = counts.get(&room.id).copied().unwrap_or(0);
                serde_json::json!({
                    "id": room.id,
                    "name": room.name,
                    "creator_id": room.creator_id,
                    "invitation_code": room.invitation_code,
                    "created_at": room.created_at.to_rfc3339(),
                    "member_count": member_count,
                })
            })
            .collect();

        Ok(result)
    }

    pub async fn get_room_detail(
        &self,
        room_id: Uuid,
        user_id: Uuid,
    ) -> Result<serde_json::Value, RoomServiceError> {
        let room = self
            .room_repo
            .find_by_id(room_id)
            .await?
            .ok_or(RoomServiceError::RoomNotFound)?;

        let member = self.member_repo.find_membership(room_id, user_id).await?;
        if member.is_none() {
            return Err(RoomServiceError::NotMember);
        }

        let members = self.member_repo.list_by_room(room_id).await?;

        let member_user_ids: Vec<Uuid> = members.iter().map(|m| m.user_id).collect();
        let users = if member_user_ids.is_empty() {
            Vec::new()
        } else {
            self.user_repo
                .find_by_ids(&member_user_ids)
                .await
                .map_err(|_| RoomServiceError::UserNotFound)?
        };
        let user_map: HashMap<Uuid, String> = users.into_iter().map(|u| (u.id, u.pseudo)).collect();

        let mut member_infos = Vec::new();
        for m in &members {
            member_infos.push(serde_json::json!({
                "user_id": m.user_id,
                "pseudo": user_map.get(&m.user_id).cloned().unwrap_or_default(),
                "joined_at": m.joined_at.to_rfc3339(),
            }));
        }

        let active_run = self.run_repo.find_active_by_room(room_id).await?;

        let current_game_info = if let Some(ref run) = active_run {
            if run.current_game_index > 0 {
                self.run_game_repo
                    .find_by_run_and_index(run.id, run.current_game_index - 1)
                    .await?
                    .map(|rg| {
                        serde_json::json!({
                            "game_id": rg.game_id,
                            "game_index": rg.game_index,
                            "status": rg.status,
                        })
                    })
            } else {
                None
            }
        } else {
            None
        };

        Ok(serde_json::json!({
            "id": room.id,
            "name": room.name,
            "creator_id": room.creator_id,
            "invitation_code": room.invitation_code,
            "created_at": room.created_at.to_rfc3339(),
            "members": member_infos,
            "member_count": members.len(),
            "active_run": active_run.map(|r| serde_json::json!({
                "id": r.id,
                "num_games": r.num_games,
                "bet_per_game": r.bet_per_game,
                "current_game_index": r.current_game_index,
                "status": r.status,
                "all_games_created": r.current_game_index >= r.num_games,
                "current_game": current_game_info,
            })),
        }))
    }

    pub async fn join_room(
        &self,
        user_id: Uuid,
        invitation_code: &str,
    ) -> Result<crate::database::models::room::Model, RoomServiceError> {
        let room = self
            .room_repo
            .find_by_invitation_code(invitation_code)
            .await?
            .ok_or(RoomServiceError::InvalidCode)?;

        let existing = self.member_repo.find_membership(room.id, user_id).await?;
        if existing.is_some() {
            return Err(RoomServiceError::AlreadyMember);
        }

        let member_count = self.member_repo.count_by_room(room.id).await?;
        if member_count >= self.config.room_max_players as usize {
            return Err(RoomServiceError::RoomFull {
                max: self.config.room_max_players,
            });
        }

        self.member_repo.create(room.id, user_id).await?;

        if let Ok(Some(u)) = self.user_repo.find_by_id(user_id).await {
            self.publish_event(&RoomEvent::MemberJoined {
                room_id: room.id,
                user_id,
                pseudo: u.pseudo,
            })
            .await;
        }

        Ok(room)
    }

    pub async fn invite_to_room(
        &self,
        room_id: Uuid,
        user_id: Uuid,
        email: &str,
    ) -> Result<(), RoomServiceError> {
        let room = self
            .room_repo
            .find_by_id(room_id)
            .await?
            .ok_or(RoomServiceError::RoomNotFound)?;

        let member = self.member_repo.find_membership(room_id, user_id).await?;
        if member.is_none() {
            return Err(RoomServiceError::NotMember);
        }

        let user = self
            .user_repo
            .find_by_id(user_id)
            .await
            .map_err(|_| RoomServiceError::UserNotFound)?
            .ok_or(RoomServiceError::UserNotFound)?;

        let lang = crate::i18n::Lang::parse(&user.language).unwrap_or_default();

        let task = EmailTask {
            email: email.to_string(),
            pseudo: user.pseudo.clone(),
            room_name: room.name.clone(),
            code: room.invitation_code.clone(),
            lang,
        };

        match tokio::time::timeout(std::time::Duration::from_secs(5), self.email_tx.send(task))
            .await
        {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                tracing::error!(
                    "Email consumer channel closed, cannot send invitation to {}: {}",
                    email,
                    e
                );
            }
            Err(_) => {
                tracing::warn!(
                    "Email channel full (timed out after 5s), dropping invitation for {}",
                    email
                );
            }
        }

        Ok(())
    }

    pub async fn leave_room(&self, room_id: Uuid, user_id: Uuid) -> Result<(), RoomServiceError> {
        let room = self
            .room_repo
            .find_by_id(room_id)
            .await?
            .ok_or(RoomServiceError::RoomNotFound)?;

        let member = self.member_repo.find_membership(room_id, user_id).await?;
        if member.is_none() {
            return Err(RoomServiceError::NotMember);
        }

        let active_run = self.run_repo.find_active_by_room(room_id).await?;
        if let Some(ref run) = active_run {
            let run_player = self
                .run_player_repo
                .find_by_run_and_user(run.id, user_id)
                .await?;
            if run_player.is_some() {
                return Err(RoomServiceError::LeaveBlockedByRun);
            }
        }

        self.member_repo.remove(room_id, user_id).await?;

        let remaining = self.member_repo.list_by_room(room_id).await?;

        if remaining.is_empty() {
            self.room_repo.delete(room_id).await?;
        } else if user_id == room.creator_id {
            let oldest = remaining.iter().min_by_key(|m| m.joined_at);
            if let Some(new_creator) = oldest {
                self.room_repo
                    .update_creator(room_id, new_creator.user_id)
                    .await?;
            }
        }

        if let Ok(Some(u)) = self.user_repo.find_by_id(user_id).await {
            self.publish_event(&RoomEvent::MemberLeft {
                room_id,
                user_id,
                pseudo: u.pseudo,
            })
            .await;
        }

        Ok(())
    }
}

include!("room_service_runs.rs");
include!("room_service_games.rs");
include!("room_service_stall.rs");

#[cfg(test)]
#[path = "room_service_tests.rs"]
mod room_service_tests;
