use async_trait::async_trait;
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, DatabaseConnection, DbErr, EntityTrait,
    QueryFilter, QuerySelect, Set, TransactionTrait,
};
use uuid::Uuid;

use crate::database::models::{player_profile, user, PlayerProfile, PlayerType, User};
use crate::database::traits::UserRepoTrait;

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

    pub async fn find_by_email(&self, email: &str) -> Result<Option<User>, DbErr> {
        user::Entity::find()
            .filter(user::Column::Email.eq(email))
            .one(&self.connection)
            .await
    }

    pub async fn find_by_id(&self, id: Uuid) -> Result<Option<User>, DbErr> {
        user::Entity::find_by_id(id).one(&self.connection).await
    }

    pub async fn find_by_ids(&self, ids: &[Uuid]) -> Result<Vec<User>, DbErr> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        user::Entity::find()
            .filter(user::Column::Id.is_in(ids.iter().copied()))
            .all(&self.connection)
            .await
    }

    pub async fn find_by_pseudo(&self, pseudo: &str) -> Result<Option<User>, DbErr> {
        user::Entity::find()
            .filter(user::Column::Pseudo.eq(pseudo))
            .one(&self.connection)
            .await
    }

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
}

#[async_trait]
impl UserRepoTrait for UserRepository {
    async fn find_by_email(&self, email: &str) -> Result<Option<User>, DbErr> {
        self.find_by_email(email).await
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<User>, DbErr> {
        self.find_by_id(id).await
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
