pub mod game_error;
pub mod validation_error;

use actix_web::{http::StatusCode, HttpResponse, ResponseError};
use sea_orm::DbErr;

use crate::api::dto::responses::ApiErrorResponse;
use crate::api::services::auth_service::AuthError;
use crate::i18n::{Lang, CURRENT_LANG, TRANSLATOR};

pub use game_error::GameError;
pub use validation_error::ValidationError;

#[derive(Debug)]
pub enum AppError {
    Game(GameError),

    Validation(ValidationError),

    Database(DbErr),

    /// Internal/server error. The payload is kept for logging only; the client
    /// always receives the generic localized `server.internal_error` message.
    Internal(String),

    /// Translatable errors. The payload is a translation-catalog key.
    NotFound(&'static str),

    Forbidden(&'static str),

    Unauthorized(&'static str),

    Conflict(&'static str),

    BadRequest(&'static str),

    /// Translatable error with interpolation parameters (e.g. `{credits}`).
    BadRequestParams {
        key: &'static str,
        params: Vec<(&'static str, String)>,
    },

    Serialization(serde_json::Error),

    Config(config::ConfigError),

    #[allow(dead_code)]
    ExternalService(String),
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppError::Game(e) => write!(f, "{e}"),
            AppError::Validation(e) => write!(f, "{e}"),
            AppError::Database(e) => write!(f, "Database error: {e}"),
            AppError::Internal(s) => write!(f, "Internal error: {s}"),
            AppError::NotFound(key) => write!(f, "{}", TRANSLATOR.t(key, Lang::En)),
            AppError::Forbidden(key) => write!(f, "{}", TRANSLATOR.t(key, Lang::En)),
            AppError::Unauthorized(key) => write!(f, "{}", TRANSLATOR.t(key, Lang::En)),
            AppError::Conflict(key) => write!(f, "{}", TRANSLATOR.t(key, Lang::En)),
            AppError::BadRequest(key) => write!(f, "{}", TRANSLATOR.t(key, Lang::En)),
            AppError::BadRequestParams { key, params } => {
                let params: Vec<(&str, &str)> =
                    params.iter().map(|(k, v)| (*k, v.as_str())).collect();
                write!(f, "{}", TRANSLATOR.t_params(key, Lang::En, &params))
            }
            AppError::Serialization(e) => write!(f, "Serialization error: {e}"),
            AppError::Config(e) => write!(f, "Configuration error: {e}"),
            AppError::ExternalService(s) => write!(f, "External service error: {s}"),
        }
    }
}

impl std::error::Error for AppError {}

impl From<GameError> for AppError {
    fn from(e: GameError) -> Self {
        AppError::Game(e)
    }
}

impl From<ValidationError> for AppError {
    fn from(e: ValidationError) -> Self {
        AppError::Validation(e)
    }
}

impl From<DbErr> for AppError {
    fn from(e: DbErr) -> Self {
        AppError::Database(e)
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::Serialization(e)
    }
}

impl From<config::ConfigError> for AppError {
    fn from(e: config::ConfigError) -> Self {
        AppError::Config(e)
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Internal(format!("IO error: {e}"))
    }
}

impl From<AuthError> for AppError {
    fn from(e: AuthError) -> Self {
        match e {
            AuthError::Validation { key, .. } => AppError::BadRequest(key),
            AuthError::Conflict { key, .. } => AppError::Conflict(key),
            AuthError::Unauthorized { key } => AppError::Unauthorized(key),
            AuthError::Internal { detail } => AppError::Internal(detail),
            AuthError::NotFound { key } => AppError::NotFound(key),
        }
    }
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
            AppError::BadRequest(_) | AppError::BadRequestParams { .. } => "app:bad_request",
            AppError::Serialization(_) => "app:serialization",
            AppError::Config(_) => "app:config",
            AppError::ExternalService(_) => "app:external_service",
        }
    }

    fn message_key(&self) -> &'static str {
        match self {
            AppError::Game(e) => e.message_key(),
            AppError::Validation(e) => e.message_key(),
            AppError::Database(_)
            | AppError::Internal(_)
            | AppError::Serialization(_)
            | AppError::Config(_)
            | AppError::ExternalService(_) => "server.internal_error",
            AppError::NotFound(key)
            | AppError::Forbidden(key)
            | AppError::Unauthorized(key)
            | AppError::Conflict(key)
            | AppError::BadRequest(key) => key,
            AppError::BadRequestParams { key, .. } => key,
        }
    }

    fn params(&self) -> Vec<(&'static str, String)> {
        match self {
            AppError::Game(e) => e.params(),
            AppError::Validation(e) => e.params(),
            AppError::BadRequestParams { params, .. } => params.clone(),
            _ => Vec::new(),
        }
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
            AppError::BadRequest(_) | AppError::BadRequestParams { .. } => StatusCode::BAD_REQUEST,
            AppError::Serialization(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::Config(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::ExternalService(_) => StatusCode::BAD_GATEWAY,
        }
    }

    fn error_response(&self) -> HttpResponse {
        let status = self.status_code();
        let is_server_error = status.is_server_error();
        let request_id = crate::observability::CORRELATION_ID
            .try_with(|id| id.to_string())
            .ok();

        let lang = CURRENT_LANG.try_with(|l| *l).unwrap_or(Lang::En);

        let error_message = if is_server_error {
            TRANSLATOR.t("server.internal_error", lang)
        } else {
            let own_params = self.params();
            let params: Vec<(&str, &str)> =
                own_params.iter().map(|(k, v)| (*k, v.as_str())).collect();
            TRANSLATOR.t_params(self.message_key(), lang, &params)
        };

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
        GameError::SpecialClaimNotAllowed => StatusCode::FORBIDDEN,
        GameError::AccountFrozen { .. } => StatusCode::FORBIDDEN,
        GameError::CashoutLocked => StatusCode::FORBIDDEN,
        GameError::GameFinished
        | GameError::GameNotPending
        | GameError::AlreadyJoined
        | GameError::GameFull
        | GameError::CreatorCannotJoin
        | GameError::GameNotReady => StatusCode::CONFLICT,
        GameError::ClaimPending => StatusCode::CONFLICT,
        // GameError::RoundNotComplete | GameError::InviteExpired | GameError::StepByStepOnly | GameError::NotABot => StatusCode::BAD_REQUEST,
        GameError::RoundNotComplete | GameError::StepByStepOnly | GameError::NotABot => {
            StatusCode::BAD_REQUEST
        }
        GameError::InsufficientCredits { .. } => StatusCode::PAYMENT_REQUIRED,
        GameError::VersionConflict | GameError::IdempotencyConflict => StatusCode::CONFLICT,
        GameError::Database(_) | GameError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{Lang, CURRENT_LANG};
    use actix_web::body::to_bytes;

    async fn error_body(err: AppError, lang: Lang) -> serde_json::Value {
        let res = CURRENT_LANG.sync_scope(lang, || err.error_response());
        let body = to_bytes(res.into_body()).await.unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    #[actix_web::test]
    async fn game_error_translates_to_french() {
        let body = error_body(AppError::Game(GameError::GameNotFound), Lang::Fr).await;
        assert_eq!(body["error"], "Partie introuvable");
    }

    #[actix_web::test]
    async fn game_error_renders_english() {
        let body = error_body(AppError::Game(GameError::GameNotFound), Lang::En).await;
        assert_eq!(body["error"], "Game not found");
    }

    #[actix_web::test]
    async fn error_falls_back_to_english_without_scoped_lang() {
        let res = AppError::BadRequest("game.not_found").error_response();
        let body = to_bytes(res.into_body()).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"], "Game not found");
    }

    #[actix_web::test]
    async fn language_switch_changes_message() {
        let en = error_body(AppError::Game(GameError::GameFinished), Lang::En).await;
        let fr = error_body(AppError::Game(GameError::GameFinished), Lang::Fr).await;
        assert_eq!(en["error"], "Game already finished");
        assert_eq!(fr["error"], "Partie déjà terminée");
        assert_ne!(en["error"], fr["error"]);
    }

    #[actix_web::test]
    async fn insufficient_credits_params_interpolated() {
        let err = GameError::InsufficientCredits {
            required: 10,
            current: 5,
        };
        let body = error_body(AppError::Game(err), Lang::Fr).await;
        let expected = crate::i18n::TRANSLATOR.t_params(
            "game.insufficient_credits",
            Lang::Fr,
            &[("{required}", "10"), ("{current}", "5")],
        );
        assert_eq!(body["error"], expected.as_str());
        assert!(body["error"].as_str().unwrap().contains("10"));
        assert!(body["error"].as_str().unwrap().contains("5"));
    }

    #[actix_web::test]
    async fn bad_request_params_interpolated() {
        let err = AppError::BadRequestParams {
            key: "cashout.minimum",
            params: vec![("{credits}", "250".to_string())],
        };
        let body = error_body(err, Lang::En).await;
        assert_eq!(body["error"], "Cashout must be at least 250 credits");
    }

    #[actix_web::test]
    async fn server_error_is_generic_and_translated() {
        let err = AppError::Internal("secret db detail".to_string());
        let body = error_body(err, Lang::Fr).await;
        assert_eq!(body["error"], "Erreur interne du serveur");
    }

    #[actix_web::test]
    async fn validation_error_translates_with_param() {
        let err = ValidationError::MissingField("pseudo".to_string());
        let body = error_body(AppError::Validation(err), Lang::Fr).await;
        assert_eq!(body["error"], "Champ obligatoire manquant\u{a0}: pseudo");
    }

    #[actix_web::test]
    async fn validation_card_index_translates() {
        let err = ValidationError::CardIndexOutOfRange(33);
        let body = error_body(AppError::Validation(err), Lang::En).await;
        assert_eq!(body["error"], "Card index 33 out of valid range (0-31)");
    }

    #[actix_web::test]
    async fn auth_error_translates_and_preserves_field() {
        let err = AuthError::Validation {
            key: "auth.password_too_short",
            field: Some("password".into()),
        };
        let res = CURRENT_LANG.sync_scope(Lang::Fr, || err.error_response());
        let body = to_bytes(res.into_body()).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            json["error"],
            "Le mot de passe doit contenir au moins 8 caractères"
        );
        assert_eq!(json["field"], "password");
    }
}
