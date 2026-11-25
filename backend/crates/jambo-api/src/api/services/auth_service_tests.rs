use std::sync::{Arc, Mutex};

use sea_orm::DbErr;
use uuid::Uuid;

use jambo_core::i18n::{Lang, Translator};
use jambo_db::models::{ConnectAccountStatus, PlayerProfile, User};
use jambo_db::traits::UserRepoTrait;

use crate::api::dto::auth::{
    ChangeEmailRequest, ChangeLanguageRequest, ChangePasswordRequest, LoginRequest,
};
use crate::api::services::auth_service::{AuthError, AuthService};
use crate::auth::config::AuthConfig;

struct MockUserRepo {
    user: Option<User>,
}

#[async_trait::async_trait]
impl UserRepoTrait for MockUserRepo {
    async fn find_by_email(&self, _email: &str) -> Result<Option<User>, DbErr> {
        Ok(self.user.clone())
    }
    async fn find_by_id(&self, _id: Uuid) -> Result<Option<User>, DbErr> {
        Ok(self.user.clone())
    }
    async fn find_by_ids(&self, _ids: &[Uuid]) -> Result<Vec<User>, DbErr> {
        Ok(vec![])
    }
    async fn find_by_pseudo(&self, _pseudo: &str) -> Result<Option<User>, DbErr> {
        Ok(self.user.clone())
    }
    async fn find_by_pseudo_prefix(&self, _prefix: &str, _limit: u64) -> Result<Vec<User>, DbErr> {
        Ok(vec![])
    }
    async fn create_user_with_profile(
        &self,
        _pseudo: &str,
        _email: &str,
        _password_hash: &str,
        _ip_hash: Option<&str>,
    ) -> Result<(User, PlayerProfile), DbErr> {
        Err(DbErr::Custom("stub".into()))
    }
    async fn update_password_hash(&self, _id: Uuid, _hash: &str) -> Result<User, DbErr> {
        Err(DbErr::Custom("stub".into()))
    }
    async fn update_last_ip_hash(&self, _id: Uuid, _hash: &str) -> Result<User, DbErr> {
        Err(DbErr::Custom("stub".into()))
    }
    async fn update_language(&self, _id: Uuid, _language: &str) -> Result<User, DbErr> {
        Err(DbErr::Custom("stub".into()))
    }
    async fn update_email(&self, _id: Uuid, _email: &str) -> Result<User, DbErr> {
        Err(DbErr::Custom("stub".into()))
    }
}

fn auth_config() -> AuthConfig {
    AuthConfig {
        jwt_secret: "test-secret-key-for-testing-only-1234567890".to_string(),
        jwt_expiry_hours: 24,
        ip_hash_pepper: "test-pepper".to_string(),
        frontend_url: "http://localhost".to_string(),
    }
}

fn make_user(deleted_at: Option<chrono::DateTime<chrono::Utc>>) -> User {
    let now = chrono::Utc::now();
    User {
        id: Uuid::now_v7(),
        pseudo: "alice".to_string(),
        email: "alice@example.com".to_string(),
        password_hash: jambo_core::password::hash_password("password1").unwrap(),
        last_ip_hash: None,
        language: "en".to_string(),
        stripe_connect_account_id: None,
        stripe_connect_account_status: ConnectAccountStatus::NotCreated,
        created_at: now,
        updated_at: now,
        deletion_requested_at: None,
        deleted_at,
    }
}

fn service(repo: Arc<MockUserRepo>) -> AuthService<MockUserRepo> {
    AuthService::new(
        repo,
        auth_config(),
        jambo_mailer::EmailQueue::channel().0,
        Arc::new(Translator::new()),
    )
}

#[tokio::test]
async fn login_rejects_soft_deleted_user() {
    let user = make_user(Some(chrono::Utc::now()));
    let service = service(Arc::new(MockUserRepo { user: Some(user) }));

    let result = service
        .login(
            LoginRequest {
                email: "alice@example.com".to_string(),
                password: "password1".to_string(),
            },
            None,
            Lang::En,
        )
        .await;

    match result {
        Err(AuthError::Unauthorized { key }) => assert_eq!(key, "auth.account_deleted"),
        Err(other) => panic!("expected unauthorized account_deleted, got {other:?}"),
        Ok(_) => panic!("expected soft-deleted user to be rejected"),
    }
}

#[tokio::test]
async fn login_succeeds_for_active_user() {
    let user = make_user(None);
    let service = service(Arc::new(MockUserRepo { user: Some(user) }));

    let result = service
        .login(
            LoginRequest {
                email: "alice@example.com".to_string(),
                password: "password1".to_string(),
            },
            None,
            Lang::En,
        )
        .await
        .expect("active user must log in");
    assert!(result.response.user.is_some());
}

struct AccountUserRepo {
    users: Mutex<Vec<User>>,
}

impl AccountUserRepo {
    fn new(user: User) -> Self {
        Self {
            users: Mutex::new(vec![user]),
        }
    }

    fn snapshot(&self) -> Vec<User> {
        self.users.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl UserRepoTrait for AccountUserRepo {
    async fn find_by_email(&self, email: &str) -> Result<Option<User>, DbErr> {
        Ok(self
            .users
            .lock()
            .unwrap()
            .iter()
            .find(|u| u.email == email)
            .cloned())
    }
    async fn find_by_id(&self, id: Uuid) -> Result<Option<User>, DbErr> {
        Ok(self
            .users
            .lock()
            .unwrap()
            .iter()
            .find(|u| u.id == id)
            .cloned())
    }
    async fn find_by_ids(&self, _ids: &[Uuid]) -> Result<Vec<User>, DbErr> {
        Ok(vec![])
    }
    async fn find_by_pseudo(&self, _pseudo: &str) -> Result<Option<User>, DbErr> {
        Ok(None)
    }
    async fn find_by_pseudo_prefix(&self, _prefix: &str, _limit: u64) -> Result<Vec<User>, DbErr> {
        Ok(vec![])
    }
    async fn create_user_with_profile(
        &self,
        _pseudo: &str,
        _email: &str,
        _password_hash: &str,
        _ip_hash: Option<&str>,
    ) -> Result<(User, PlayerProfile), DbErr> {
        Err(DbErr::Custom("stub".into()))
    }
    async fn update_password_hash(&self, id: Uuid, hash: &str) -> Result<User, DbErr> {
        let mut users = self.users.lock().unwrap();
        let user = users
            .iter_mut()
            .find(|u| u.id == id)
            .ok_or_else(|| DbErr::Custom("not found".into()))?;
        user.password_hash = hash.to_string();
        Ok(user.clone())
    }
    async fn update_last_ip_hash(&self, _id: Uuid, _hash: &str) -> Result<User, DbErr> {
        Err(DbErr::Custom("stub".into()))
    }
    async fn update_language(&self, id: Uuid, language: &str) -> Result<User, DbErr> {
        let mut users = self.users.lock().unwrap();
        let user = users
            .iter_mut()
            .find(|u| u.id == id)
            .ok_or_else(|| DbErr::Custom("not found".into()))?;
        user.language = language.to_string();
        Ok(user.clone())
    }
    async fn update_email(&self, id: Uuid, email: &str) -> Result<User, DbErr> {
        let mut users = self.users.lock().unwrap();
        let user = users
            .iter_mut()
            .find(|u| u.id == id)
            .ok_or_else(|| DbErr::Custom("not found".into()))?;
        user.email = email.to_string();
        Ok(user.clone())
    }
}

fn account_service(repo: Arc<AccountUserRepo>) -> AuthService<AccountUserRepo> {
    AuthService::new(
        repo,
        auth_config(),
        jambo_mailer::EmailQueue::channel().0,
        Arc::new(Translator::new()),
    )
}

#[tokio::test]
async fn change_password_updates_hash() {
    let user = make_user(None);
    let repo = Arc::new(AccountUserRepo::new(user.clone()));
    let service = account_service(repo.clone());

    let result = service
        .change_password(
            user.id,
            ChangePasswordRequest {
                current_password: "password1".to_string(),
                new_password: "newpassword2".to_string(),
                new_password_confirm: "newpassword2".to_string(),
            },
            Lang::En,
        )
        .await
        .expect("password change must succeed");
    assert!(result.success);

    let updated = &repo.snapshot()[0];
    assert!(jambo_core::password::verify_password("newpassword2", &updated.password_hash).unwrap());
}

#[tokio::test]
async fn change_password_rejects_wrong_current_password() {
    let user = make_user(None);
    let repo = Arc::new(AccountUserRepo::new(user.clone()));
    let service = account_service(repo);

    let result = service
        .change_password(
            user.id,
            ChangePasswordRequest {
                current_password: "wrong-password".to_string(),
                new_password: "newpassword2".to_string(),
                new_password_confirm: "newpassword2".to_string(),
            },
            Lang::En,
        )
        .await;

    match result {
        Err(AuthError::Unauthorized { key }) => assert_eq!(key, "auth.current_password_invalid"),
        other => panic!("expected unauthorized, got {other:?}"),
    }
}

#[tokio::test]
async fn change_password_rejects_short_password() {
    let user = make_user(None);
    let repo = Arc::new(AccountUserRepo::new(user.clone()));
    let service = account_service(repo);

    let result = service
        .change_password(
            user.id,
            ChangePasswordRequest {
                current_password: "password1".to_string(),
                new_password: "short".to_string(),
                new_password_confirm: "short".to_string(),
            },
            Lang::En,
        )
        .await;

    match result {
        Err(AuthError::Validation { key, .. }) => assert_eq!(key, "auth.password_too_short"),
        other => panic!("expected validation, got {other:?}"),
    }
}

#[tokio::test]
async fn change_email_updates_email() {
    let user = make_user(None);
    let repo = Arc::new(AccountUserRepo::new(user.clone()));
    let service = account_service(repo.clone());

    let result = service
        .change_email(
            user.id,
            ChangeEmailRequest {
                new_email: "alice.new@example.com".to_string(),
                current_password: "password1".to_string(),
            },
            Lang::En,
        )
        .await
        .expect("email change must succeed");

    assert!(result.success);
    assert_eq!(result.user.unwrap().email, "alice.new@example.com");
    assert_eq!(repo.snapshot()[0].email, "alice.new@example.com");
}

#[tokio::test]
async fn change_email_rejects_duplicate() {
    let user = make_user(None);
    let mut other = make_user(None);
    other.id = Uuid::now_v7();
    other.email = "taken@example.com".to_string();

    let repo = Arc::new(AccountUserRepo {
        users: Mutex::new(vec![user.clone(), other]),
    });
    let service = account_service(repo);

    let result = service
        .change_email(
            user.id,
            ChangeEmailRequest {
                new_email: "taken@example.com".to_string(),
                current_password: "password1".to_string(),
            },
            Lang::En,
        )
        .await;

    match result {
        Err(AuthError::Conflict { key, .. }) => assert_eq!(key, "auth.email_in_use"),
        other => panic!("expected conflict, got {other:?}"),
    }
}

#[tokio::test]
async fn set_language_persists_valid_language() {
    let user = make_user(None);
    let repo = Arc::new(AccountUserRepo::new(user.clone()));
    let service = account_service(repo.clone());

    let result = service
        .set_language(
            user.id,
            ChangeLanguageRequest {
                language: "fr".to_string(),
            },
            Lang::En,
        )
        .await
        .expect("language change must succeed");

    assert!(result.success);
    assert_eq!(result.language, "fr");
    assert_eq!(repo.snapshot()[0].language, "fr");
}

#[tokio::test]
async fn set_language_rejects_invalid_language() {
    let user = make_user(None);
    let repo = Arc::new(AccountUserRepo::new(user.clone()));
    let service = account_service(repo);

    let result = service
        .set_language(
            user.id,
            ChangeLanguageRequest {
                language: "de".to_string(),
            },
            Lang::En,
        )
        .await;

    match result {
        Err(AuthError::Validation { key, .. }) => assert_eq!(key, "language.invalid_lang"),
        other => panic!("expected validation, got {other:?}"),
    }
}
