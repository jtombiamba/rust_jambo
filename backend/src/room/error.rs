use crate::api::dto::responses::ApiErrorResponse;
use crate::database::models::RunStatus;

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
    RunNotActive { status: RunStatus },
    #[error("Previous game in this run is not yet finished")]
    PreviousGameNotFinished,
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
            RoomServiceError::PreviousGameNotFinished => "room:previous_game_not_finished",
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
            RoomServiceError::PreviousGameNotFinished => StatusCode::CONFLICT,
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
