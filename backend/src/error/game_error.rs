use sea_orm::DbErr;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GameError {
    #[error("Database error: {0}")]
    Database(#[from] DbErr),

    #[error("Game not found")]
    GameNotFound,

    #[error("Player not found")]
    PlayerNotFound,

    #[error("Card not found or already played")]
    CardNotFound,

    #[error("Not your turn to play")]
    NotYourTurn,

    #[error("Invalid card play: must follow suit if possible")]
    InvalidCard,

    #[error("Round not complete")]
    RoundNotComplete,

    #[error("Game already finished")]
    GameFinished,

    #[error("{0}")]
    Internal(#[source] Box<dyn std::error::Error + Send>),
}
