use actix_web::http::StatusCode;

use jambo_types::responses::ApiErrorResponse;

use crate::i18n::{Lang, CURRENT_LANG, TRANSLATOR};
use crate::observability::CORRELATION_ID;

#[derive(Debug)]
pub enum AuthError {
    Validation {
        key: &'static str,
        field: Option<String>,
    },
    Conflict {
        key: &'static str,
        field: Option<String>,
    },
    Unauthorized {
        key: &'static str,
    },
    /// Internal error. The payload is kept for logging only; the client always
    /// receives the generic localized `server.internal_error` message.
    Internal {
        detail: String,
    },
    NotFound {
        key: &'static str,
    },
}

impl AuthError {
    pub fn source(&self) -> &'static str {
        match self {
            AuthError::Validation { .. } => "auth:validation",
            AuthError::Conflict { .. } => "auth:conflict",
            AuthError::Unauthorized { .. } => "auth:unauthorized",
            AuthError::Internal { .. } => "auth:internal",
            AuthError::NotFound { .. } => "auth:not_found",
        }
    }

    fn message_key(&self) -> &'static str {
        match self {
            AuthError::Validation { key, .. }
            | AuthError::Conflict { key, .. }
            | AuthError::Unauthorized { key }
            | AuthError::NotFound { key } => key,
            AuthError::Internal { .. } => "server.internal_error",
        }
    }
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthError::Validation { key, .. } => {
                write!(f, "Validation error: {}", TRANSLATOR.t(key, Lang::En))
            }
            AuthError::Conflict { key, .. } => {
                write!(f, "Conflict: {}", TRANSLATOR.t(key, Lang::En))
            }
            AuthError::Unauthorized { key } => {
                write!(f, "Unauthorized: {}", TRANSLATOR.t(key, Lang::En))
            }
            AuthError::Internal { detail } => write!(f, "Internal error: {detail}"),
            AuthError::NotFound { key } => {
                write!(f, "Not found: {}", TRANSLATOR.t(key, Lang::En))
            }
        }
    }
}

impl actix_web::ResponseError for AuthError {
    fn error_response(&self) -> actix_web::HttpResponse {
        let status = self.status_code();
        let request_id = CORRELATION_ID.try_with(|id| id.to_string()).ok();
        let lang = CURRENT_LANG.try_with(|l| *l).unwrap_or(Lang::En);
        let error_msg = TRANSLATOR.t(self.message_key(), lang);
        let field = match self {
            AuthError::Validation { field, .. } | AuthError::Conflict { field, .. } => {
                field.clone()
            }
            _ => None,
        };
        if let AuthError::Internal { detail } = self {
            tracing::error!(error = %detail, request_id = ?request_id, "Auth internal error");
        }
        actix_web::HttpResponse::build(status).json(ApiErrorResponse {
            success: false,
            error: error_msg,
            field,
            source: self.source().to_string(),
            request_id,
        })
    }

    fn status_code(&self) -> StatusCode {
        match self {
            AuthError::Validation { .. } => StatusCode::BAD_REQUEST,
            AuthError::Conflict { .. } => StatusCode::CONFLICT,
            AuthError::Unauthorized { .. } => StatusCode::UNAUTHORIZED,
            AuthError::Internal { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            AuthError::NotFound { .. } => StatusCode::NOT_FOUND,
        }
    }
}
