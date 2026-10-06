use std::sync::Arc;
use uuid::Uuid;

use crate::api::dto::auth::{
    AuthResponse, ForgotPasswordRequest, ForgotPasswordResponse, LoginRequest, RegisterRequest,
    ResetPasswordRequest, ResetPasswordResponse, UserInfo,
};
use crate::api::dto::responses::ApiErrorResponse;
use crate::auth::config::AuthConfig;
use crate::auth::{jwt, password};
use crate::database::traits::UserRepoTrait;
use crate::i18n::{Lang, Translator};
use crate::mailer::Mailer;

/// Trait seam for the auth service, enabling handler-level testing with a mock.
#[async_trait::async_trait]
pub trait AuthServiceTrait: Send + Sync {
    async fn register(
        &self,
        body: RegisterRequest,
        ip_hash: Option<String>,
        lang: Lang,
    ) -> Result<RegisterResult, AuthError>;

    async fn login(
        &self,
        body: LoginRequest,
        ip_hash: Option<String>,
        lang: Lang,
    ) -> Result<LoginResult, AuthError>;

    async fn forgot_password(
        &self,
        body: ForgotPasswordRequest,
        lang: Lang,
    ) -> ForgotPasswordResponse;

    async fn reset_password(
        &self,
        body: ResetPasswordRequest,
        lang: Lang,
    ) -> Result<ResetPasswordResponse, AuthError>;

    async fn me(&self, user_id: Uuid) -> Result<UserInfo, AuthError>;
}

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
                write!(
                    f,
                    "Validation error: {}",
                    crate::i18n::TRANSLATOR.t(key, Lang::En)
                )
            }
            AuthError::Conflict { key, .. } => {
                write!(f, "Conflict: {}", crate::i18n::TRANSLATOR.t(key, Lang::En))
            }
            AuthError::Unauthorized { key } => {
                write!(
                    f,
                    "Unauthorized: {}",
                    crate::i18n::TRANSLATOR.t(key, Lang::En)
                )
            }
            AuthError::Internal { detail } => write!(f, "Internal error: {detail}"),
            AuthError::NotFound { key } => {
                write!(f, "Not found: {}", crate::i18n::TRANSLATOR.t(key, Lang::En))
            }
        }
    }
}

impl actix_web::ResponseError for AuthError {
    fn error_response(&self) -> actix_web::HttpResponse {
        let status = self.status_code();
        let request_id = crate::observability::CORRELATION_ID
            .try_with(|id| id.to_string())
            .ok();
        let lang = crate::i18n::CURRENT_LANG
            .try_with(|l| *l)
            .unwrap_or(Lang::En);
        let error_msg = crate::i18n::TRANSLATOR.t(self.message_key(), lang);
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

    fn status_code(&self) -> actix_web::http::StatusCode {
        use actix_web::http::StatusCode;
        match self {
            AuthError::Validation { .. } => StatusCode::BAD_REQUEST,
            AuthError::Conflict { .. } => StatusCode::CONFLICT,
            AuthError::Unauthorized { .. } => StatusCode::UNAUTHORIZED,
            AuthError::Internal { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            AuthError::NotFound { .. } => StatusCode::NOT_FOUND,
        }
    }
}

pub struct RegisterResult {
    pub response: AuthResponse,
    pub token: String,
}

pub struct LoginResult {
    pub response: AuthResponse,
    pub token: String,
}

pub struct AuthService<R: UserRepoTrait> {
    repo: Arc<R>,
    config: AuthConfig,
    mailer: Arc<dyn Mailer>,
    translator: Arc<Translator>,
}

impl<R: UserRepoTrait> AuthService<R> {
    pub fn new(
        repo: Arc<R>,
        config: AuthConfig,
        mailer: Arc<dyn Mailer>,
        translator: Arc<Translator>,
    ) -> Self {
        Self {
            repo,
            config,
            mailer,
            translator,
        }
    }

    pub async fn register(
        &self,
        body: RegisterRequest,
        ip_hash: Option<String>,
        lang: Lang,
    ) -> Result<RegisterResult, AuthError> {
        let t = |key: &str| self.translator.t(key, lang);

        if body.pseudo.trim().is_empty() {
            return Err(AuthError::Validation {
                key: "auth.pseudo_required",
                field: Some("pseudo".into()),
            });
        }

        let email = body.email.trim().to_lowercase();
        if email.is_empty() || !email.contains('@') {
            return Err(AuthError::Validation {
                key: "auth.email_invalid",
                field: Some("email".into()),
            });
        }

        if body.password.len() < 8 {
            return Err(AuthError::Validation {
                key: "auth.password_too_short",
                field: Some("password".into()),
            });
        }

        if body.password != body.password_confirm {
            return Err(AuthError::Validation {
                key: "auth.passwords_not_match",
                field: Some("password_confirm".into()),
            });
        }

        let existing_email = self.repo.find_by_email(&email).await.map_err(|e| {
            tracing::error!("Database error checking email: {}", e);
            AuthError::Internal {
                detail: "database error checking email".into(),
            }
        })?;

        if existing_email.is_some() {
            return Err(AuthError::Conflict {
                key: "auth.email_in_use",
                field: Some("email".into()),
            });
        }

        let existing_pseudo = self
            .repo
            .find_by_pseudo(body.pseudo.trim())
            .await
            .map_err(|e| {
                tracing::error!("Database error checking pseudo: {}", e);
                AuthError::Internal {
                    detail: "database error checking pseudo".into(),
                }
            })?;

        if existing_pseudo.is_some() {
            return Err(AuthError::Conflict {
                key: "auth.pseudo_taken",
                field: Some("pseudo".into()),
            });
        }

        let password_hash = password::hash_password(&body.password).map_err(|e| {
            tracing::error!("Password hashing failed: {}", e);
            AuthError::Internal {
                detail: "password hashing failed".into(),
            }
        })?;

        let (user, _profile) = self
            .repo
            .create_user_with_profile(
                body.pseudo.trim(),
                &email,
                &password_hash,
                ip_hash.as_deref(),
            )
            .await
            .map_err(|e| {
                tracing::error!("Failed to create user: {}", e);
                AuthError::Internal {
                    detail: "failed to create user".into(),
                }
            })?;

        let token = jwt::generate_token(user.id, &user.pseudo, &self.config).map_err(|e| {
            tracing::error!("JWT generation failed: {}", e);
            AuthError::Internal {
                detail: "jwt generation failed".into(),
            }
        })?;

        Ok(RegisterResult {
            token,
            response: AuthResponse {
                success: true,
                message: t("auth.account_created"),
                user: Some(UserInfo {
                    id: user.id,
                    pseudo: user.pseudo,
                    email: user.email,
                    language: user.language,
                }),
            },
        })
    }

    pub async fn login(
        &self,
        body: LoginRequest,
        ip_hash: Option<String>,
        lang: Lang,
    ) -> Result<LoginResult, AuthError> {
        let t = |key: &str| self.translator.t(key, lang);
        let email = body.email.trim().to_lowercase();

        let user = self.repo.find_by_email(&email).await.map_err(|e| {
            tracing::error!("Database error during login: {}", e);
            AuthError::Internal {
                detail: "database error during login".into(),
            }
        })?;

        let user = match user {
            Some(u) => u,
            None => {
                return Err(AuthError::Unauthorized {
                    key: "auth.invalid_credentials",
                });
            }
        };

        let valid =
            password::verify_password(&body.password, &user.password_hash).map_err(|e| {
                tracing::error!("Password verification error: {}", e);
                AuthError::Internal {
                    detail: "password verification error".into(),
                }
            })?;

        if !valid {
            return Err(AuthError::Unauthorized {
                key: "auth.invalid_credentials",
            });
        }

        if let Some(ref hash) = ip_hash {
            if let Err(e) = self.repo.update_last_ip_hash(user.id, hash).await {
                tracing::warn!("Failed to update IP hash on login: {}", e);
            }
        }

        let token = jwt::generate_token(user.id, &user.pseudo, &self.config).map_err(|e| {
            tracing::error!("JWT generation failed: {}", e);
            AuthError::Internal {
                detail: "jwt generation failed".into(),
            }
        })?;

        Ok(LoginResult {
            token,
            response: AuthResponse {
                success: true,
                message: t("auth.logged_in"),
                user: Some(UserInfo {
                    id: user.id,
                    pseudo: user.pseudo,
                    email: user.email,
                    language: user.language,
                }),
            },
        })
    }

    pub async fn forgot_password(
        &self,
        body: ForgotPasswordRequest,
        lang: Lang,
    ) -> ForgotPasswordResponse {
        let email = body.email.trim().to_lowercase();

        if !email.is_empty() {
            if let Ok(Some(user)) = self.repo.find_by_email(&email).await {
                if let Ok(token) = jwt::generate_reset_token(&email, &self.config) {
                    let reset_link = format!(
                        "{}/password-reset?token={}",
                        self.config.frontend_url, token
                    );

                    let user_lang = Lang::parse(&user.language).unwrap_or(Lang::En);
                    tracing::info!("Send password reset link for {}", email);
                    if let Err(e) = self
                        .mailer
                        .send_password_reset(&email, &reset_link, user_lang)
                        .await
                    {
                        tracing::error!("Failed to send password reset email to {email}: {e}");
                    }
                }
            }
        }

        let message = self.translator.t("password.forgot", lang);
        ForgotPasswordResponse {
            success: true,
            message: message.replace("{email}", &email),
        }
    }

    pub async fn reset_password(
        &self,
        body: ResetPasswordRequest,
        lang: Lang,
    ) -> Result<ResetPasswordResponse, AuthError> {
        let t = |key: &str| self.translator.t(key, lang);

        if body.password.len() < 8 {
            return Err(AuthError::Validation {
                key: "auth.password_too_short",
                field: Some("password".into()),
            });
        }

        if body.password != body.password_confirm {
            return Err(AuthError::Validation {
                key: "auth.passwords_not_match",
                field: Some("password_confirm".into()),
            });
        }

        let reset_claims = jwt::validate_reset_token(&body.token, &self.config).map_err(|e| {
            tracing::info!("Invalid reset token: {}", e);
            AuthError::Unauthorized {
                key: "password.reset_link_expired",
            }
        })?;

        let user = self
            .repo
            .find_by_email(&reset_claims.email)
            .await
            .map_err(|e| {
                tracing::error!("Database error during password reset: {}", e);
                AuthError::Internal {
                    detail: "database error during password reset".into(),
                }
            })?;

        let user = user.ok_or(AuthError::Unauthorized {
            key: "password.reset_link_expired",
        })?;

        let password_hash = password::hash_password(&body.password).map_err(|e| {
            tracing::error!("Password hashing failed: {}", e);
            AuthError::Internal {
                detail: "password hashing failed".into(),
            }
        })?;

        self.repo
            .update_password_hash(user.id, &password_hash)
            .await
            .map_err(|e| {
                tracing::error!("Failed to update password: {}", e);
                AuthError::Internal {
                    detail: "failed to update password".into(),
                }
            })?;

        Ok(ResetPasswordResponse {
            success: true,
            message: t("password.reset_success"),
        })
    }

    pub async fn me(&self, user_id: Uuid) -> Result<UserInfo, AuthError> {
        let user = self.repo.find_by_id(user_id).await.map_err(|e| {
            tracing::error!("Database error fetching user: {}", e);
            AuthError::Internal {
                detail: "database error fetching user".into(),
            }
        })?;

        match user {
            Some(u) => Ok(UserInfo {
                id: u.id,
                pseudo: u.pseudo,
                email: u.email,
                language: u.language,
            }),
            None => Err(AuthError::NotFound {
                key: "auth.user_not_found",
            }),
        }
    }
}

#[async_trait::async_trait]
impl<R: UserRepoTrait> AuthServiceTrait for AuthService<R> {
    async fn register(
        &self,
        body: RegisterRequest,
        ip_hash: Option<String>,
        lang: Lang,
    ) -> Result<RegisterResult, AuthError> {
        AuthService::register(self, body, ip_hash, lang).await
    }

    async fn login(
        &self,
        body: LoginRequest,
        ip_hash: Option<String>,
        lang: Lang,
    ) -> Result<LoginResult, AuthError> {
        AuthService::login(self, body, ip_hash, lang).await
    }

    async fn forgot_password(
        &self,
        body: ForgotPasswordRequest,
        lang: Lang,
    ) -> ForgotPasswordResponse {
        AuthService::forgot_password(self, body, lang).await
    }

    async fn reset_password(
        &self,
        body: ResetPasswordRequest,
        lang: Lang,
    ) -> Result<ResetPasswordResponse, AuthError> {
        AuthService::reset_password(self, body, lang).await
    }

    async fn me(&self, user_id: Uuid) -> Result<UserInfo, AuthError> {
        AuthService::me(self, user_id).await
    }
}
