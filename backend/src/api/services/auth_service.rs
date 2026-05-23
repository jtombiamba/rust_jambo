use std::sync::Arc;
use uuid::Uuid;

use crate::api::dto::auth::{
    AuthResponse, ErrorResponse, ForgotPasswordRequest, ForgotPasswordResponse, LoginRequest,
    RegisterRequest, ResetPasswordRequest, ResetPasswordResponse, UserInfo,
};
use crate::auth::config::AuthConfig;
use crate::auth::{jwt, password};
use crate::database::traits::UserRepoTrait;
use crate::i18n::{Lang, Translator};
use crate::mailer::Mailer;

#[derive(Debug)]
pub enum AuthError {
    Validation {
        error: String,
        field: Option<String>,
    },
    Conflict {
        error: String,
        field: Option<String>,
    },
    Unauthorized {
        error: String,
    },
    Internal {
        error: String,
    },
    NotFound {
        error: String,
    },
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthError::Validation { error, .. } => write!(f, "Validation error: {}", error),
            AuthError::Conflict { error, .. } => write!(f, "Conflict: {}", error),
            AuthError::Unauthorized { error } => write!(f, "Unauthorized: {}", error),
            AuthError::Internal { error } => write!(f, "Internal error: {}", error),
            AuthError::NotFound { error } => write!(f, "Not found: {}", error),
        }
    }
}

impl actix_web::ResponseError for AuthError {
    fn error_response(&self) -> actix_web::HttpResponse {
        use actix_web::http::StatusCode;
        match self {
            AuthError::Validation { error, field } => {
                actix_web::HttpResponse::build(StatusCode::BAD_REQUEST).json(ErrorResponse {
                    success: false,
                    error: error.clone(),
                    field: field.clone(),
                })
            }
            AuthError::Conflict { error, field } => {
                actix_web::HttpResponse::build(StatusCode::CONFLICT).json(ErrorResponse {
                    success: false,
                    error: error.clone(),
                    field: field.clone(),
                })
            }
            AuthError::Unauthorized { error } => {
                actix_web::HttpResponse::build(StatusCode::UNAUTHORIZED).json(ErrorResponse {
                    success: false,
                    error: error.clone(),
                    field: None,
                })
            }
            AuthError::Internal { error } => {
                actix_web::HttpResponse::InternalServerError().json(ErrorResponse {
                    success: false,
                    error: error.clone(),
                    field: None,
                })
            }
            AuthError::NotFound { error } => {
                actix_web::HttpResponse::NotFound().json(ErrorResponse {
                    success: false,
                    error: error.clone(),
                    field: None,
                })
            }
        }
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
                error: t("auth.pseudo_required"),
                field: Some("pseudo".into()),
            });
        }

        let email = body.email.trim().to_lowercase();
        if email.is_empty() || !email.contains('@') {
            return Err(AuthError::Validation {
                error: t("auth.email_invalid"),
                field: Some("email".into()),
            });
        }

        if body.password.len() < 8 {
            return Err(AuthError::Validation {
                error: t("auth.password_too_short"),
                field: Some("password".into()),
            });
        }

        if body.password != body.password_confirm {
            return Err(AuthError::Validation {
                error: t("auth.passwords_not_match"),
                field: Some("password_confirm".into()),
            });
        }

        let existing_email = self.repo.find_by_email(&email).await.map_err(|e| {
            tracing::error!("Database error checking email: {}", e);
            AuthError::Internal {
                error: t("server.internal_error"),
            }
        })?;

        if existing_email.is_some() {
            return Err(AuthError::Conflict {
                error: t("auth.email_in_use"),
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
                    error: t("server.internal_error"),
                }
            })?;

        if existing_pseudo.is_some() {
            return Err(AuthError::Conflict {
                error: t("auth.pseudo_taken"),
                field: Some("pseudo".into()),
            });
        }

        let password_hash = password::hash_password(&body.password).map_err(|e| {
            tracing::error!("Password hashing failed: {}", e);
            AuthError::Internal {
                error: t("server.internal_error"),
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
                    error: t("server.internal_error"),
                }
            })?;

        let token = jwt::generate_token(user.id, &user.pseudo, &self.config).map_err(|e| {
            tracing::error!("JWT generation failed: {}", e);
            AuthError::Internal {
                error: t("server.internal_error"),
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
                error: t("server.internal_error"),
            }
        })?;

        let user = match user {
            Some(u) => u,
            None => {
                return Err(AuthError::Unauthorized {
                    error: t("auth.invalid_credentials"),
                });
            }
        };

        let valid =
            password::verify_password(&body.password, &user.password_hash).map_err(|e| {
                tracing::error!("Password verification error: {}", e);
                AuthError::Internal {
                    error: t("server.internal_error"),
                }
            })?;

        if !valid {
            return Err(AuthError::Unauthorized {
                error: t("auth.invalid_credentials"),
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
                error: t("server.internal_error"),
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
                error: t("auth.password_too_short"),
                field: Some("password".into()),
            });
        }

        if body.password != body.password_confirm {
            return Err(AuthError::Validation {
                error: t("auth.passwords_not_match"),
                field: Some("password_confirm".into()),
            });
        }

        let reset_claims = jwt::validate_reset_token(&body.token, &self.config).map_err(|e| {
            tracing::info!("Invalid reset token: {}", e);
            AuthError::Unauthorized {
                error: t("password.reset_link_expired"),
            }
        })?;

        let user = self
            .repo
            .find_by_email(&reset_claims.email)
            .await
            .map_err(|e| {
                tracing::error!("Database error during password reset: {}", e);
                AuthError::Internal {
                    error: t("server.internal_error"),
                }
            })?;

        let user = user.ok_or_else(|| AuthError::Unauthorized {
            error: t("password.reset_link_expired"),
        })?;

        let password_hash = password::hash_password(&body.password).map_err(|e| {
            tracing::error!("Password hashing failed: {}", e);
            AuthError::Internal {
                error: t("server.internal_error"),
            }
        })?;

        self.repo
            .update_password_hash(user.id, &password_hash)
            .await
            .map_err(|e| {
                tracing::error!("Failed to update password: {}", e);
                AuthError::Internal {
                    error: t("server.internal_error"),
                }
            })?;

        Ok(ResetPasswordResponse {
            success: true,
            message: t("password.reset_success"),
        })
    }

    pub async fn me(&self, user_id: Uuid, lang: Lang) -> Result<UserInfo, AuthError> {
        let t = |key: &str| self.translator.t(key, lang);

        let user = self.repo.find_by_id(user_id).await.map_err(|e| {
            tracing::error!("Database error fetching user: {}", e);
            AuthError::Internal {
                error: t("server.internal_error"),
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
                error: t("auth.user_not_found"),
            }),
        }
    }
}
