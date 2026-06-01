pub mod game_error;
pub mod validation_error;

use actix_web::{http::StatusCode, HttpResponse, ResponseError};
use sea_orm::DbErr;
use thiserror::Error;

use crate::api::dto::responses::ApiErrorResponse;
use crate::api::services::auth_service::AuthError;

pub use game_error::GameError;
pub use validation_error::ValidationError;

#[derive(Debug, Error)]
pub enum AppError {
    #[error(transparent)]
    Game(#[from] GameError),

    #[error(transparent)]
    Validation(#[from] ValidationError),

    #[error("Database error")]
    Database(#[from] DbErr),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Configuration error: {0}")]
    Config(#[from] config::ConfigError),

    #[error("External service error: {0}")]
    #[allow(dead_code)]
    ExternalService(String),
}

impl AppError {
    pub fn source(&self) -> &'static str {
        match self {
            AppError::Game(e) => e.source(),
            AppError::Validation(e) => e.source(),
            AppError::Database(_) => "app:database",
            AppError::Internal(_) => "app:internal",
            AppError::NotFound(_) => "app:not_found",
            AppError::Forbidden(_) => "app:forbidden",
            AppError::Unauthorized(_) => "app:unauthorized",
            AppError::Conflict(_) => "app:conflict",
            AppError::BadRequest(_) => "app:bad_request",
            AppError::Serialization(_) => "app:serialization",
            AppError::Config(_) => "app:config",
            AppError::ExternalService(_) => "app:external_service",
        }
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Internal(format!("IO error: {}", e))
    }
}

impl ResponseError for AppError {
    fn status_code(&self) -> StatusCode {
        match self {
            AppError::Game(e) => game_error_status_code(e),
            AppError::Validation(_) => StatusCode::BAD_REQUEST,
            AppError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            AppError::Forbidden(_) => StatusCode::FORBIDDEN,
            AppError::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            AppError::Conflict(_) => StatusCode::CONFLICT,
            AppError::BadRequest(_) => StatusCode::BAD_REQUEST,
            AppError::Serialization(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::Config(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::ExternalService(_) => StatusCode::BAD_GATEWAY,
        }
    }

    fn error_response(&self) -> HttpResponse {
        let status = self.status_code();
        let is_server_error = status.is_server_error();
        let error_message = if is_server_error {
            "Internal server error".to_string()
        } else {
            self.to_string()
        };
        let request_id = crate::observability::CORRELATION_ID
            .try_with(|id| id.to_string())
            .ok();
        if is_server_error {
            tracing::error!(error = ?self, request_id = ?request_id, "Server error occurred");
        }
        HttpResponse::build(status).json(ApiErrorResponse {
            success: false,
            error: error_message,
            field: None,
            source: self.source().to_string(),
            request_id,
        })
    }
}

fn game_error_status_code(e: &GameError) -> StatusCode {
    match e {
        GameError::GameNotFound
        | GameError::PlayerNotFound
        | GameError::CardNotFound
        | GameError::ProfileNotFound => StatusCode::NOT_FOUND,
        GameError::NotYourTurn
        | GameError::InvalidCard
        | GameError::NotCreator
        | GameError::NotInvited => StatusCode::FORBIDDEN,
        GameError::AccountFrozen { .. } => StatusCode::FORBIDDEN,
        GameError::GameFinished
        | GameError::GameNotPending
        | GameError::AlreadyJoined
        | GameError::GameFull
        | GameError::CreatorCannotJoin
        | GameError::GameNotReady => StatusCode::CONFLICT,
        GameError::RoundNotComplete | GameError::InviteExpired => StatusCode::BAD_REQUEST,
        GameError::InsufficientCredits { .. } => StatusCode::PAYMENT_REQUIRED,
        GameError::Database(_) | GameError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

impl From<AuthError> for AppError {
    fn from(e: AuthError) -> Self {
        match e {
            AuthError::Validation { error, .. } => AppError::BadRequest(error),
            AuthError::Conflict { error, .. } => AppError::Conflict(error),
            AuthError::Unauthorized { error } => AppError::Unauthorized(error),
            AuthError::Internal { error } => AppError::Internal(error),
            AuthError::NotFound { error } => AppError::NotFound(error),
        }
    }
}
