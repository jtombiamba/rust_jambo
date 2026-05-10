use std::sync::Arc;
use uuid::Uuid;

use crate::api::dto::auth::{
    AuthResponse, ErrorResponse, ForgotPasswordRequest, ForgotPasswordResponse, LoginRequest,
    RegisterRequest, ResetPasswordRequest, ResetPasswordResponse, UserInfo,
};
use crate::auth::config::AuthConfig;
use crate::auth::{jwt, password};
use crate::database::traits::UserRepoTrait;
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
}

impl<R: UserRepoTrait> AuthService<R> {
    pub fn new(repo: Arc<R>, config: AuthConfig, mailer: Arc<dyn Mailer>) -> Self {
        Self {
            repo,
            config,
            mailer,
        }
    }

    pub async fn register(
        &self,
        body: RegisterRequest,
        ip_hash: Option<String>,
    ) -> Result<RegisterResult, AuthError> {
        if body.pseudo.trim().is_empty() {
            return Err(AuthError::Validation {
                error: "Pseudo is required".into(),
                field: Some("pseudo".into()),
            });
        }

        let email = body.email.trim().to_lowercase();
        if email.is_empty() || !email.contains('@') {
            return Err(AuthError::Validation {
                error: "A valid email is required".into(),
                field: Some("email".into()),
            });
        }

        if body.password.len() < 8 {
            return Err(AuthError::Validation {
                error: "Password must be at least 8 characters".into(),
                field: Some("password".into()),
            });
        }

        if body.password != body.password_confirm {
            return Err(AuthError::Validation {
                error: "Passwords do not match".into(),
                field: Some("password_confirm".into()),
            });
        }

        let existing_email = self.repo.find_by_email(&email).await.map_err(|e| {
            tracing::error!("Database error checking email: {}", e);
            AuthError::Internal {
                error: "Internal server error".into(),
            }
        })?;

        if existing_email.is_some() {
            return Err(AuthError::Conflict {
                error: "This email is already in use".into(),
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
                    error: "Internal server error".into(),
                }
            })?;

        if existing_pseudo.is_some() {
            return Err(AuthError::Conflict {
                error: "This pseudo is already taken".into(),
                field: Some("pseudo".into()),
            });
        }

        let password_hash = password::hash_password(&body.password).map_err(|e| {
            tracing::error!("Password hashing failed: {}", e);
            AuthError::Internal {
                error: "Internal server error".into(),
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
                    error: "Internal server error".into(),
                }
            })?;

        let token = jwt::generate_token(user.id, &user.pseudo, &self.config).map_err(|e| {
            tracing::error!("JWT generation failed: {}", e);
            AuthError::Internal {
                error: "Internal server error".into(),
            }
        })?;

        Ok(RegisterResult {
            token,
            response: AuthResponse {
                success: true,
                message: "Account created successfully".to_string(),
                user: Some(UserInfo {
                    id: user.id,
                    pseudo: user.pseudo,
                    email: user.email,
                }),
            },
        })
    }

    pub async fn login(
        &self,
        body: LoginRequest,
        ip_hash: Option<String>,
    ) -> Result<LoginResult, AuthError> {
        let email = body.email.trim().to_lowercase();

        let user = self.repo.find_by_email(&email).await.map_err(|e| {
            tracing::error!("Database error during login: {}", e);
            AuthError::Internal {
                error: "Internal server error".into(),
            }
        })?;

        let user = match user {
            Some(u) => u,
            None => {
                return Err(AuthError::Unauthorized {
                    error: "Invalid email or password".into(),
                });
            }
        };

        let valid =
            password::verify_password(&body.password, &user.password_hash).map_err(|e| {
                tracing::error!("Password verification error: {}", e);
                AuthError::Internal {
                    error: "Internal server error".into(),
                }
            })?;

        if !valid {
            return Err(AuthError::Unauthorized {
                error: "Invalid email or password".into(),
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
                error: "Internal server error".into(),
            }
        })?;

        Ok(LoginResult {
            token,
            response: AuthResponse {
                success: true,
                message: "Logged in successfully".to_string(),
                user: Some(UserInfo {
                    id: user.id,
                    pseudo: user.pseudo,
                    email: user.email,
                }),
            },
        })
    }

    pub async fn forgot_password(&self, body: ForgotPasswordRequest) -> ForgotPasswordResponse {
        let email = body.email.trim().to_lowercase();

        if !email.is_empty() {
            if let Ok(Some(_user)) = self.repo.find_by_email(&email).await {
                if let Ok(token) = jwt::generate_reset_token(&email, &self.config) {
                    let reset_link = format!(
                        "{}/password-reset?token={}",
                        self.config.frontend_url, token
                    );

                    if let Err(e) = self.mailer.send_password_reset(&email, &reset_link).await {
                        tracing::error!("Failed to send password reset email to {email}: {e}");
                    }
                }
            }
        }

        ForgotPasswordResponse {
            success: true,
            message: format!(
                "If {} exists, you will receive an email to reset your password",
                email
            ),
        }
    }

    pub async fn reset_password(
        &self,
        body: ResetPasswordRequest,
    ) -> Result<ResetPasswordResponse, AuthError> {
        if body.password.len() < 8 {
            return Err(AuthError::Validation {
                error: "Password must be at least 8 characters".into(),
                field: Some("password".into()),
            });
        }

        if body.password != body.password_confirm {
            return Err(AuthError::Validation {
                error: "Passwords do not match".into(),
                field: Some("password_confirm".into()),
            });
        }

        let reset_claims = jwt::validate_reset_token(&body.token, &self.config).map_err(|e| {
            tracing::info!("Invalid reset token: {}", e);
            AuthError::Unauthorized {
                error: "This reset link is invalid or has expired".into(),
            }
        })?;

        let user = self
            .repo
            .find_by_email(&reset_claims.email)
            .await
            .map_err(|e| {
                tracing::error!("Database error during password reset: {}", e);
                AuthError::Internal {
                    error: "Internal server error".into(),
                }
            })?;

        let user = user.ok_or_else(|| AuthError::Unauthorized {
            error: "This reset link is invalid or has expired".into(),
        })?;

        let password_hash = password::hash_password(&body.password).map_err(|e| {
            tracing::error!("Password hashing failed: {}", e);
            AuthError::Internal {
                error: "Internal server error".into(),
            }
        })?;

        self.repo
            .update_password_hash(user.id, &password_hash)
            .await
            .map_err(|e| {
                tracing::error!("Failed to update password: {}", e);
                AuthError::Internal {
                    error: "Internal server error".into(),
                }
            })?;

        Ok(ResetPasswordResponse {
            success: true,
            message: "Password reset successfully".to_string(),
        })
    }

    pub async fn me(&self, user_id: Uuid) -> Result<UserInfo, AuthError> {
        let user = self.repo.find_by_id(user_id).await.map_err(|e| {
            tracing::error!("Database error fetching user: {}", e);
            AuthError::Internal {
                error: "Internal server error".into(),
            }
        })?;

        match user {
            Some(u) => Ok(UserInfo {
                id: u.id,
                pseudo: u.pseudo,
                email: u.email,
            }),
            None => Err(AuthError::NotFound {
                error: "User not found".into(),
            }),
        }
    }
}
