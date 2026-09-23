use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, DbErr, EntityTrait, PaginatorTrait,
    QueryFilter, Set,
};
use uuid::Uuid;

use crate::auth::password::verify_password;
use crate::database::models::{admin_key, AdminKey};

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct AdminKeyRepository {
    connection: DatabaseConnection,
}

#[allow(dead_code)]
impl AdminKeyRepository {
    pub fn new(connection: DatabaseConnection) -> Self {
        Self { connection }
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn list_active(&self) -> Result<Vec<AdminKey>, DbErr> {
        admin_key::Entity::find()
            .filter(admin_key::Column::RevokedAt.is_null())
            .all(&self.connection)
            .await
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn count_active(&self) -> Result<u64, DbErr> {
        admin_key::Entity::find()
            .filter(admin_key::Column::RevokedAt.is_null())
            .count(&self.connection)
            .await
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn create(&self, label: &str, key_hash: &str) -> Result<AdminKey, DbErr> {
        let now = chrono::Utc::now();
        admin_key::ActiveModel {
            id: Set(Uuid::now_v7()),
            label: Set(label.to_string()),
            key_hash: Set(key_hash.to_string()),
            created_at: Set(now),
            revoked_at: Set(None),
        }
        .insert(&self.connection)
        .await
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn revoke(&self, label: &str) -> Result<u64, DbErr> {
        let result = admin_key::Entity::update_many()
            .col_expr(
                admin_key::Column::RevokedAt,
                sea_orm::sea_query::Expr::value(chrono::Utc::now()),
            )
            .filter(admin_key::Column::Label.eq(label))
            .filter(admin_key::Column::RevokedAt.is_null())
            .exec(&self.connection)
            .await?;
        Ok(result.rows_affected)
    }

    /// Verify a keypass against all active keys. Returns the label of the first
    /// matching key, or `None` when no key matches. Every active hash is checked
    /// so the operation does not leak which key was (or was not) a candidate.
    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn verify(&self, keypass: &str) -> Result<Option<String>, DbErr> {
        let active = self.list_active().await?;
        let mut matched: Option<String> = None;
        for key in active {
            let ok = verify_password(keypass, &key.key_hash).unwrap_or(false);
            if ok && matched.is_none() {
                matched = Some(key.label);
            }
        }
        Ok(matched)
    }
}
