use uuid::Uuid;

use jambo_core::i18n::Lang;
use jambo_core::password;
use jambo_db::models::User;
use jambo_db::traits::UserRepoTrait;

use crate::api::dto::auth::{
    ChangeEmailRequest, ChangeEmailResponse, ChangeLanguageRequest, ChangeLanguageResponse,
    ChangePasswordRequest, ChangePasswordResponse, UserInfo,
};
use crate::api::services::auth_service::{AuthError, AuthService};

/// Account-settings operations (password, email and language) for a logged-in
/// user. Kept in a dedicated module so each service file stays focused and small.
impl<R: UserRepoTrait> AuthService<R> {
    pub async fn change_password(
        &self,
        user_id: Uuid,
        body: ChangePasswordRequest,
        lang: Lang,
    ) -> Result<ChangePasswordResponse, AuthError> {
        let t = |key: &str| self.translator.t(key, lang);

        if body.new_password.len() < 8 {
            return Err(AuthError::Validation {
                key: "auth.password_too_short",
                field: Some("new_password".into()),
            });
        }

        if body.new_password != body.new_password_confirm {
            return Err(AuthError::Validation {
                key: "auth.passwords_not_match",
                field: Some("new_password_confirm".into()),
            });
        }

        let user = self.fetch_user(user_id).await?;
        self.verify_password(&user, &body.current_password)?;

        let password_hash = password::hash_password(&body.new_password).map_err(|e| {
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

        Ok(ChangePasswordResponse {
            success: true,
            message: t("auth.password_changed"),
        })
    }

    pub async fn change_email(
        &self,
        user_id: Uuid,
        body: ChangeEmailRequest,
        lang: Lang,
    ) -> Result<ChangeEmailResponse, AuthError> {
        let t = |key: &str| self.translator.t(key, lang);

        let new_email = body.new_email.trim().to_lowercase();
        if new_email.is_empty() || !new_email.contains('@') {
            return Err(AuthError::Validation {
                key: "auth.email_invalid",
                field: Some("new_email".into()),
            });
        }

        let user = self.fetch_user(user_id).await?;
        self.verify_password(&user, &body.current_password)?;

        let existing = self.repo.find_by_email(&new_email).await.map_err(|e| {
            tracing::error!("Database error checking email: {}", e);
            AuthError::Internal {
                detail: "database error checking email".into(),
            }
        })?;

        if existing.is_some_and(|u| u.id != user.id) {
            return Err(AuthError::Conflict {
                key: "auth.email_in_use",
                field: Some("new_email".into()),
            });
        }

        let updated = self
            .repo
            .update_email(user.id, &new_email)
            .await
            .map_err(|e| {
                tracing::error!("Failed to update email: {}", e);
                AuthError::Internal {
                    detail: "failed to update email".into(),
                }
            })?;

        Ok(ChangeEmailResponse {
            success: true,
            message: t("auth.email_changed"),
            user: Some(UserInfo {
                id: updated.id,
                pseudo: updated.pseudo,
                email: updated.email,
                language: updated.language,
            }),
        })
    }

    pub async fn set_language(
        &self,
        user_id: Uuid,
        body: ChangeLanguageRequest,
        lang: Lang,
    ) -> Result<ChangeLanguageResponse, AuthError> {
        let t = |key: &str| self.translator.t(key, lang);

        let requested = Lang::parse(&body.language).ok_or(AuthError::Validation {
            key: "language.invalid_lang",
            field: Some("language".into()),
        })?;

        self.repo
            .update_language(user_id, requested.as_str())
            .await
            .map_err(|e| {
                tracing::error!("Failed to update language: {}", e);
                AuthError::Internal {
                    detail: "failed to update language".into(),
                }
            })?;

        Ok(ChangeLanguageResponse {
            success: true,
            message: t("language.set_success"),
            language: requested.as_str().to_string(),
        })
    }

    async fn fetch_user(&self, user_id: Uuid) -> Result<User, AuthError> {
        let user = self.repo.find_by_id(user_id).await.map_err(|e| {
            tracing::error!("Database error fetching user: {}", e);
            AuthError::Internal {
                detail: "database error fetching user".into(),
            }
        })?;

        user.ok_or(AuthError::NotFound {
            key: "auth.user_not_found",
        })
    }

    fn verify_password(&self, user: &User, current_password: &str) -> Result<(), AuthError> {
        let valid =
            password::verify_password(current_password, &user.password_hash).map_err(|e| {
                tracing::error!("Password verification error: {}", e);
                AuthError::Internal {
                    detail: "password verification error".into(),
                }
            })?;

        if !valid {
            return Err(AuthError::Unauthorized {
                key: "auth.current_password_invalid",
            });
        }

        Ok(())
    }
}
