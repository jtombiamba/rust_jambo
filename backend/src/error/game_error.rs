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

    #[error("Insufficient credits: need {required} but have {current}")]
    InsufficientCredits { required: i32, current: i32 },

    #[error("Game is not in pending state")]
    GameNotPending,

    #[error("User is not the game creator")]
    NotCreator,

    #[error("User is not invited to this game")]
    NotInvited,

    #[error("User is already a player in this game")]
    AlreadyJoined,

    #[error("Game is full")]
    GameFull,

    #[error("Invite has expired")]
    InviteExpired,

    #[error("Creator cannot join their own game")]
    CreatorCannotJoin,
    #[error("Game is not in ready state")]
    GameNotReady,
    #[error("{0}")]
    Internal(#[source] Box<dyn std::error::Error + Send>),
}
