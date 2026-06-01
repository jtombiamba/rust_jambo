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
    #[error("Account is frozen until {until}")]
    AccountFrozen { until: String },
    #[error("{0}")]
    Internal(#[source] Box<dyn std::error::Error + Send>),
    #[error("Player profile not found")]
    ProfileNotFound,
}

impl GameError {
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
            GameError::InviteExpired => "game:invite_expired",
            GameError::CreatorCannotJoin => "game:creator_cannot_join",
            GameError::GameNotReady => "game:game_not_ready",
            GameError::AccountFrozen { .. } => "game:account_frozen",
            GameError::Internal(_) => "game:internal",
            GameError::ProfileNotFound => "game:profile_not_found",
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
}
