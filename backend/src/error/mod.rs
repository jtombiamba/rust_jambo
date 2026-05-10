pub mod game_error;
pub mod validation_error;

use actix_web::{http::StatusCode, HttpResponse, ResponseError};
use serde_json::json;
use thiserror::Error;

pub use game_error::GameError;
pub use validation_error::ValidationError;

#[derive(Debug, Error)]
pub enum AppError {
    #[error(transparent)]
    Game(#[from] GameError),

    #[error(transparent)]
    Validation(#[from] ValidationError),

    #[error("Configuration error: {0}")]
    Config(#[from] config::ConfigError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("WebSocket error: {0}")]
    #[allow(dead_code)]
    WebSocket(String),
    #[error("Internal error")]
    #[allow(dead_code)]
    Internal,
}

impl ResponseError for AppError {
    fn status_code(&self) -> StatusCode {
        match self {
            AppError::Game(e) => match e {
                GameError::GameNotFound | GameError::PlayerNotFound | GameError::CardNotFound => {
                    StatusCode::NOT_FOUND
                }
                GameError::NotYourTurn
                | GameError::InvalidCard
                | GameError::NotCreator
                | GameError::NotInvited => StatusCode::FORBIDDEN,
                GameError::GameFinished
                | GameError::GameNotPending
                | GameError::AlreadyJoined
                | GameError::GameFull
                | GameError::CreatorCannotJoin
                | GameError::GameNotReady => StatusCode::CONFLICT,
                GameError::RoundNotComplete | GameError::InviteExpired => StatusCode::BAD_REQUEST,
                GameError::InsufficientCredits => StatusCode::PAYMENT_REQUIRED,
                GameError::Database(_) | GameError::Internal(_) => {
                    StatusCode::INTERNAL_SERVER_ERROR
                }
            },
            AppError::Validation(_) => StatusCode::BAD_REQUEST,
            AppError::Config(_) | AppError::Serialization(_) | AppError::Io(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
            AppError::WebSocket(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn error_response(&self) -> HttpResponse {
        let status = self.status_code();
        let error_message = self.to_string();
        HttpResponse::build(status).json(json!({
            "success": false,
            "error": error_message
        }))
    }
}
