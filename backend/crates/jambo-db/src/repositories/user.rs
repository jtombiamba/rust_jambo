use async_trait::async_trait;
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, DatabaseConnection, DbErr, EntityTrait,
    QueryFilter, QuerySelect, Set, TransactionTrait,
};
use uuid::Uuid;

use crate::models::{player, player_profile, user, PlayerProfile, PlayerType, User};
use crate::traits::UserRepoTrait;

/// Display name substituted for every `players.name` row belonging to a
/// soft-deleted user. Keeps historical games intact while hiding the pseudo.
pub const DEACTIVATED_PLAYER_NAME: &str = "deactivated player";

/// Unique pseudo tombstone for a soft-deleted user. The `id` keeps the value
/// unique against the `users.pseudo` unique constraint while dropping the
/// original pseudo.
pub fn tombstone_pseudo(id: Uuid) -> String {
    format!("deleted_{id}")
}

/// Result of a soft-delete: the anonymized user row plus how many player rows
/// were relabeled.
#[derive(Debug, Clone)]
pub struct SoftDeleteOutcome {
    pub user: User,
    pub players_renamed: u64,
}

#[derive(Debug, Clone)]
pub struct UserRepository {
    connection: DatabaseConnection,
    default_credit: i32,
}

impl UserRepository {
    pub fn new(connection: DatabaseConnection, default_credit: i32) -> Self {
        Self {
            connection,
            default_credit,
        }
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn find_by_email(&self, email: &str) -> Result<Option<User>, DbErr> {
        user::Entity::find()
            .filter(user::Column::Email.eq(email))
            .one(&self.connection)
            .await
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn find_by_id(&self, id: Uuid) -> Result<Option<User>, DbErr> {
        user::Entity::find_by_id(id).one(&self.connection).await
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn find_by_ids(&self, ids: &[Uuid]) -> Result<Vec<User>, DbErr> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        user::Entity::find()
            .filter(user::Column::Id.is_in(ids.iter().copied()))
            .all(&self.connection)
            .await
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn find_by_pseudo(&self, pseudo: &str) -> Result<Option<User>, DbErr> {
        user::Entity::find()
            .filter(user::Column::Pseudo.eq(pseudo))
            .one(&self.connection)
            .await
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn find_by_pseudo_prefix(
        &self,
        prefix: &str,
        limit: u64,
    ) -> Result<Vec<User>, DbErr> {
        user::Entity::find()
            .filter(user::Column::Pseudo.starts_with(prefix))
            .limit(limit)
            .all(&self.connection)
            .await
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn create_user_with_profile(
        &self,
        pseudo: &str,
        email: &str,
        password_hash: &str,
        ip_hash: Option<&str>,
    ) -> Result<(User, PlayerProfile), DbErr> {
        let now = chrono::Utc::now();
        let user_id = Uuid::now_v7();
        let profile_id = Uuid::now_v7();
        let pseudo = pseudo.to_string();
        let email = email.to_string();
        let password_hash = password_hash.to_string();
        let ip_hash = ip_hash.map(|s| s.to_string());
        let default_credit = self.default_credit;

        self.connection
            .transaction(|txn| {
                Box::pin(async move {
                    let user_active = user::ActiveModel {
                        id: Set(user_id),
                        pseudo: Set(pseudo),
                        email: Set(email),
                        password_hash: Set(password_hash),
                        last_ip_hash: Set(ip_hash),
                        language: Set("en".to_string()),
                        created_at: Set(now),
                        updated_at: Set(now),
                        deletion_requested_at: ActiveValue::NotSet,
                        deleted_at: ActiveValue::NotSet,
                    };
                    let user = user_active.insert(txn).await?;

                    let profile_active = player_profile::ActiveModel {
                        id: Set(profile_id),
                        user_id: Set(user_id),
                        player_type: Set(PlayerType::Human),
                        credit: Set(default_credit),
                        game_played: Set(0),
                        wins: Set(0),
                        kora_wins: Set(0),
                        winning_streak: Set(0),
                        latitude: ActiveValue::NotSet,
                        longitude: ActiveValue::NotSet,
                        country_code: ActiveValue::NotSet,
                        city: ActiveValue::NotSet,
                        frozen_until: ActiveValue::NotSet,
                        cashout_locked: ActiveValue::NotSet,
                        created_at: Set(now),
                        updated_at: Set(now),
                    };
                    let profile = profile_active.insert(txn).await?;

                    Ok::<_, DbErr>((user, profile))
                })
            })
            .await
            .map_err(|e: sea_orm::TransactionError<DbErr>| {
                DbErr::Custom(format!("Transaction failed: {}", e))
            })
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn update_password_hash(&self, id: Uuid, hash: &str) -> Result<User, DbErr> {
        let user_model = user::Entity::find_by_id(id)
            .one(&self.connection)
            .await?
            .ok_or_else(|| DbErr::Custom("User not found".to_string()))?;
        let mut active: user::ActiveModel = user_model.into();
        active.password_hash = Set(hash.to_string());
        active.updated_at = Set(chrono::Utc::now());
        active.update(&self.connection).await
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn update_last_ip_hash(&self, id: Uuid, hash: &str) -> Result<User, DbErr> {
        let user_model = user::Entity::find_by_id(id)
            .one(&self.connection)
            .await?
            .ok_or_else(|| DbErr::Custom("User not found".to_string()))?;
        let mut active: user::ActiveModel = user_model.into();
        active.last_ip_hash = Set(Some(hash.to_string()));
        active.updated_at = Set(chrono::Utc::now());
        active.update(&self.connection).await
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn update_language(&self, id: Uuid, language: &str) -> Result<User, DbErr> {
        let user_model = user::Entity::find_by_id(id)
            .one(&self.connection)
            .await?
            .ok_or_else(|| DbErr::Custom("User not found".to_string()))?;
        let mut active: user::ActiveModel = user_model.into();
        active.language = Set(language.to_string());
        active.updated_at = Set(chrono::Utc::now());
        active.update(&self.connection).await
    }

    /// Mark a user as having requested deletion of their data. Idempotent: the
    /// first request timestamp is preserved and a second request is a no-op.
    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn request_deletion(&self, id: Uuid) -> Result<User, DbErr> {
        let user_model = user::Entity::find_by_id(id)
            .one(&self.connection)
            .await?
            .ok_or_else(|| DbErr::Custom("User not found".to_string()))?;

        if user_model.deletion_requested_at.is_some() {
            return Ok(user_model);
        }

        let now = chrono::Utc::now();
        let mut active: user::ActiveModel = user_model.into();
        active.deletion_requested_at = Set(Some(now));
        active.updated_at = Set(now);
        active.update(&self.connection).await
    }

    /// List users who requested deletion but have not yet been soft-deleted.
    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn list_deletion_requests(&self) -> Result<Vec<User>, DbErr> {
        user::Entity::find()
            .filter(user::Column::DeletionRequestedAt.is_not_null())
            .filter(user::Column::DeletedAt.is_null())
            .all(&self.connection)
            .await
    }

    /// Soft-delete a user and relabel their historical player rows. The user row
    /// stays (flagged by `deleted_at`) but the pseudo is replaced by a unique
    /// tombstone so uniqueness holds and no personal data leaks; `players.name`
    /// is set to [`DEACTIVATED_PLAYER_NAME`] so game history remains coherent.
    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn soft_delete(&self, id: Uuid) -> Result<SoftDeleteOutcome, DbErr> {
        let now = chrono::Utc::now();
        let tombstone = tombstone_pseudo(id);

        self.connection
            .transaction(|txn| {
                Box::pin(async move {
                    let user_model = user::Entity::find_by_id(id)
                        .one(txn)
                        .await?
                        .ok_or_else(|| DbErr::Custom("User not found".to_string()))?;

                    let already_deleted = user_model.deleted_at.is_some();
                    let mut active: user::ActiveModel = user_model.into();
                    active.pseudo = Set(tombstone);
                    if !already_deleted {
                        active.deleted_at = Set(Some(now));
                    }
                    active.updated_at = Set(now);
                    let user = active.update(txn).await?;

                    let rename = player::Entity::update_many()
                        .col_expr(
                            player::Column::Name,
                            Expr::value(DEACTIVATED_PLAYER_NAME.to_string()),
                        )
                        .filter(player::Column::UserId.eq(id))
                        .exec(txn)
                        .await?;

                    Ok::<_, DbErr>(SoftDeleteOutcome {
                        user,
                        players_renamed: rename.rows_affected,
                    })
                })
            })
            .await
            .map_err(|e: sea_orm::TransactionError<DbErr>| {
                DbErr::Custom(format!("Transaction failed: {}", e))
            })
    }
}

#[async_trait]
impl UserRepoTrait for UserRepository {
    async fn find_by_email(&self, email: &str) -> Result<Option<User>, DbErr> {
        self.find_by_email(email).await
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<User>, DbErr> {
        self.find_by_id(id).await
    }

    async fn find_by_ids(&self, ids: &[Uuid]) -> Result<Vec<User>, DbErr> {
        self.find_by_ids(ids).await
    }

    async fn find_by_pseudo(&self, pseudo: &str) -> Result<Option<User>, DbErr> {
        self.find_by_pseudo(pseudo).await
    }

    async fn find_by_pseudo_prefix(&self, prefix: &str, limit: u64) -> Result<Vec<User>, DbErr> {
        self.find_by_pseudo_prefix(prefix, limit).await
    }

    async fn create_user_with_profile(
        &self,
        pseudo: &str,
        email: &str,
        password_hash: &str,
        ip_hash: Option<&str>,
    ) -> Result<(User, PlayerProfile), DbErr> {
        self.create_user_with_profile(pseudo, email, password_hash, ip_hash)
            .await
    }

    async fn update_password_hash(&self, id: Uuid, hash: &str) -> Result<User, DbErr> {
        self.update_password_hash(id, hash).await
    }

    async fn update_last_ip_hash(&self, id: Uuid, hash: &str) -> Result<User, DbErr> {
        self.update_last_ip_hash(id, hash).await
    }

    async fn update_language(&self, id: Uuid, language: &str) -> Result<User, DbErr> {
        self.update_language(id, language).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm::{DatabaseBackend, MockDatabase};

    fn now() -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::from_timestamp(1_700_000_000, 0).unwrap()
    }

    fn make_user(id: Uuid) -> User {
        User {
            id,
            pseudo: "alice".to_string(),
            email: "alice@example.com".to_string(),
            password_hash: "hash".to_string(),
            last_ip_hash: None,
            language: "en".to_string(),
            created_at: now(),
            updated_at: now(),
            deletion_requested_at: None,
            deleted_at: None,
        }
    }

    #[test]
    fn tombstone_pseudo_is_unique_per_uuid() {
        let id = Uuid::now_v7();
        assert_eq!(tombstone_pseudo(id), format!("deleted_{id}"));
    }

    #[test]
    fn deactivated_placeholder_is_present() {
        assert_eq!(DEACTIVATED_PLAYER_NAME, "deactivated player");
    }

    #[tokio::test]
    async fn request_deletion_sets_timestamp() {
        let id = Uuid::now_v7();
        let updated = User {
            deletion_requested_at: Some(now()),
            ..make_user(id)
        };
        let db = MockDatabase::new(DatabaseBackend::Postgres)
            .append_query_results(vec![vec![make_user(id)], vec![updated.clone()]])
            .into_connection();
        let repo = UserRepository::new(db, 500);

        let result = repo.request_deletion(id).await.expect("request succeeds");
        assert_eq!(result.deletion_requested_at, updated.deletion_requested_at);
        assert!(result.deleted_at.is_none());
    }

    #[tokio::test]
    async fn request_deletion_is_idempotent_when_already_requested() {
        let id = Uuid::now_v7();
        let requested = User {
            deletion_requested_at: Some(now()),
            ..make_user(id)
        };
        // No exec result configured: an update would fail, so a successful
        // return proves the early no-op path was taken.
        let db = MockDatabase::new(DatabaseBackend::Postgres)
            .append_query_results(vec![vec![requested.clone()]])
            .into_connection();
        let repo = UserRepository::new(db, 500);

        let result = repo
            .request_deletion(id)
            .await
            .expect("idempotent request succeeds");
        assert_eq!(
            result.deletion_requested_at,
            requested.deletion_requested_at
        );
        assert!(result.deleted_at.is_none());
    }
}
