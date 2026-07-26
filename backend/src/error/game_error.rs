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

    // #[error("Invite has expired")]
    // InviteExpired,
    #[error("Creator cannot join their own game")]
    CreatorCannotJoin,
    #[error("Game is not in ready state")]
    GameNotReady,
    #[error("Account is frozen until {until}")]
    AccountFrozen { until: String },
    #[error("Optimistic lock conflict: game state was modified concurrently")]
    VersionConflict,
    #[error("{0}")]
    Internal(#[source] Box<dyn std::error::Error + Send>),
    #[error("Player profile not found")]
    ProfileNotFound,
    #[error("This operation is only available in step-by-step mode")]
    StepByStepOnly,
    #[error("Current player is not a bot")]
    NotABot,
    #[error("A request with this idempotency key is already in progress")]
    IdempotencyConflict,
}

impl From<sea_orm::TransactionError<sea_orm::DbErr>> for GameError {
    fn from(e: sea_orm::TransactionError<sea_orm::DbErr>) -> Self {
        match e {
            sea_orm::TransactionError::Connection(e) => GameError::Database(e),
            sea_orm::TransactionError::Transaction(e) => GameError::Database(e),
        }
    }
}

impl GameError {
    #[track_caller]
    pub fn internal(msg: impl Into<String>) -> Self {
        GameError::Internal(Box::new(std::io::Error::other(msg.into())))
    }

    pub fn source(&self) -> &'static str {
        match self {
            GameError::Database(_) => "game:database",
            GameError::GameNotFound => "game:game_not_found",
            GameError::PlayerNotFound => "game:player_not_found",
            GameError::CardNotFound => "game:card_not_found",
            GameError::NotYourTurn => "game:not_your_turn",
            GameError::InvalidCard => "game:invalid_card",
            GameError::RoundNotComplete => "game:round_not_complete",
            GameError::GameFinished => "game:game_finished",
            GameError::InsufficientCredits { .. } => "game:insufficient_credits",
            GameError::GameNotPending => "game:game_not_pending",
            GameError::NotCreator => "game:not_creator",
            GameError::NotInvited => "game:not_invited",
            GameError::AlreadyJoined => "game:already_joined",
            GameError::GameFull => "game:game_full",
            // GameError::InviteExpired => "game:invite_expired",
            GameError::CreatorCannotJoin => "game:creator_cannot_join",
            GameError::GameNotReady => "game:game_not_ready",
            GameError::AccountFrozen { .. } => "game:account_frozen",
            GameError::VersionConflict => "game:version_conflict",
            GameError::Internal(_) => "game:internal",
            GameError::ProfileNotFound => "game:profile_not_found",
            GameError::StepByStepOnly => "game:step_by_step_only",
            GameError::NotABot => "game:not_a_bot",
            GameError::IdempotencyConflict => "game:idempotency_conflict",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_account_frozen_display() {
        let err = GameError::AccountFrozen {
            until: "2026-05-18T12:00:00+00:00".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "Account is frozen until 2026-05-18T12:00:00+00:00"
        );
    }

    #[test]
    fn test_insufficient_credits_display() {
        let err = GameError::InsufficientCredits {
            required: 10,
            current: 5,
        };
        assert_eq!(err.to_string(), "Insufficient credits: need 10 but have 5");
    }

    #[test]
    fn test_game_error_displays() {
        assert_eq!(GameError::GameNotFound.to_string(), "Game not found");
        assert_eq!(GameError::PlayerNotFound.to_string(), "Player not found");
        assert_eq!(
            GameError::CardNotFound.to_string(),
            "Card not found or already played"
        );
        assert_eq!(GameError::NotYourTurn.to_string(), "Not your turn to play");
        assert_eq!(
            GameError::InvalidCard.to_string(),
            "Invalid card play: must follow suit if possible"
        );
        assert_eq!(
            GameError::RoundNotComplete.to_string(),
            "Round not complete"
        );
        assert_eq!(GameError::GameFinished.to_string(), "Game already finished");
        assert_eq!(
            GameError::GameNotPending.to_string(),
            "Game is not in pending state"
        );
        assert_eq!(
            GameError::NotCreator.to_string(),
            "User is not the game creator"
        );
        assert_eq!(
            GameError::NotInvited.to_string(),
            "User is not invited to this game"
        );
        assert_eq!(
            GameError::AlreadyJoined.to_string(),
            "User is already a player in this game"
        );
        assert_eq!(GameError::GameFull.to_string(), "Game is full");
        assert_eq!(
            GameError::CreatorCannotJoin.to_string(),
            "Creator cannot join their own game"
        );
        assert_eq!(
            GameError::GameNotReady.to_string(),
            "Game is not in ready state"
        );
        assert_eq!(
            GameError::ProfileNotFound.to_string(),
            "Player profile not found"
        );
        assert_eq!(
            GameError::StepByStepOnly.to_string(),
            "This operation is only available in step-by-step mode"
        );
        assert_eq!(
            GameError::NotABot.to_string(),
            "Current player is not a bot"
        );
        assert_eq!(
            GameError::IdempotencyConflict.to_string(),
            "A request with this idempotency key is already in progress"
        );
    }

    #[test]
    fn test_game_error_sources() {
        assert_eq!(
            GameError::Database(DbErr::RecordNotFound("x".into())).source(),
            "game:database"
        );
        assert_eq!(GameError::GameNotFound.source(), "game:game_not_found");
        assert_eq!(GameError::PlayerNotFound.source(), "game:player_not_found");
        assert_eq!(GameError::NotYourTurn.source(), "game:not_your_turn");
        assert_eq!(GameError::GameFinished.source(), "game:game_finished");
        assert_eq!(GameError::VersionConflict.source(), "game:version_conflict");
        assert_eq!(
            GameError::Internal(Box::new(std::io::Error::other("oops"))).source(),
            "game:internal"
        );
    }

    #[test]
    fn test_internal_error_creation() {
        let err = GameError::internal("something broke");
        assert!(err.to_string().contains("something broke"));
    }

    #[test]
    fn test_from_transaction_error() {
        let db_err = DbErr::RecordNotFound("test".into());
        let tx_err = sea_orm::TransactionError::Connection(db_err.clone());
        let game_err: GameError = tx_err.into();
        assert!(matches!(game_err, GameError::Database(_)));

        let db_err2 = DbErr::RecordNotFound("test2".into());
        let tx_err2 = sea_orm::TransactionError::Transaction(db_err2);
        let game_err2: GameError = tx_err2.into();
        assert!(matches!(game_err2, GameError::Database(_)));
    }
}
