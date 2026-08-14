use async_trait::async_trait;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, DatabaseTransaction, DbErr, EntityTrait,
    QueryFilter, Set,
};
use uuid::Uuid;

use crate::database::models::{player_profile, PlayerProfile};
use crate::database::traits::PlayerProfileRepoTrait;

#[derive(Debug, Clone)]
pub struct PlayerProfileRepository {
    connection: DatabaseConnection,
}

impl PlayerProfileRepository {
    pub fn new(connection: DatabaseConnection) -> Self {
        Self { connection }
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    #[allow(dead_code)]
    pub async fn list_all(&self) -> Result<Vec<PlayerProfile>, DbErr> {
        player_profile::Entity::find().all(&self.connection).await
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn find_by_user_id(&self, user_id: Uuid) -> Result<Option<PlayerProfile>, DbErr> {
        player_profile::Entity::find()
            .filter(player_profile::Column::UserId.eq(user_id))
            .one(&self.connection)
            .await
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn find_by_user_ids(&self, user_ids: &[Uuid]) -> Result<Vec<PlayerProfile>, DbErr> {
        if user_ids.is_empty() {
            return Ok(vec![]);
        }
        player_profile::Entity::find()
            .filter(player_profile::Column::UserId.is_in(user_ids.iter().copied()))
            .all(&self.connection)
            .await
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn update_stats(
        &self,
        user_id: Uuid,
        wins_delta: i32,
        kora_wins_delta: i32,
    ) -> Result<PlayerProfile, DbErr> {
        let profile = player_profile::Entity::find()
            .filter(player_profile::Column::UserId.eq(user_id))
            .one(&self.connection)
            .await?
            .ok_or_else(|| DbErr::Custom("PlayerProfile not found".to_string()))?;

        let mut active: player_profile::ActiveModel = profile.into();
        active.game_played = Set(active.game_played.unwrap() + 1);
        active.wins = Set(active.wins.unwrap() + wins_delta);
        active.kora_wins = Set(active.kora_wins.unwrap() + kora_wins_delta);
        active.updated_at = Set(chrono::Utc::now());
        active.update(&self.connection).await
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn update_credit_and_frozen_until(
        &self,
        user_id: Uuid,
        credit: i32,
        frozen_until: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<PlayerProfile, DbErr> {
        let profile = player_profile::Entity::find()
            .filter(player_profile::Column::UserId.eq(user_id))
            .one(&self.connection)
            .await?
            .ok_or_else(|| DbErr::Custom("PlayerProfile not found".to_string()))?;
        let mut active: player_profile::ActiveModel = profile.into();
        active.credit = Set(credit);
        active.frozen_until = Set(frozen_until);
        active.updated_at = Set(chrono::Utc::now());
        active.update(&self.connection).await
    }

    #[tracing::instrument(skip(txn), fields(db.statement, db.rows_affected))]
    pub async fn debit_in_txn(
        &self,
        txn: &DatabaseTransaction,
        user_id: Uuid,
        amount: i32,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), DbErr> {
        use sea_orm::sea_query::Expr;
        use sea_orm::sea_query::ExprTrait;
        player_profile::Entity::update_many()
            .col_expr(
                player_profile::Column::Credit,
                Expr::col(player_profile::Column::Credit).sub(amount),
            )
            .col_expr(player_profile::Column::UpdatedAt, Expr::value(now))
            .filter(player_profile::Column::UserId.eq(user_id))
            .filter(player_profile::Column::Credit.gte(amount))
            .exec(txn)
            .await?;
        Ok(())
    }

    #[tracing::instrument(skip(txn), fields(db.statement, db.rows_affected))]
    pub async fn credit_in_txn(
        &self,
        txn: &DatabaseTransaction,
        user_id: Uuid,
        amount: i32,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), DbErr> {
        use sea_orm::sea_query::Expr;
        use sea_orm::sea_query::ExprTrait;
        player_profile::Entity::update_many()
            .col_expr(
                player_profile::Column::Credit,
                Expr::col(player_profile::Column::Credit).add(amount),
            )
            .col_expr(player_profile::Column::UpdatedAt, Expr::value(now))
            .filter(player_profile::Column::UserId.eq(user_id))
            .exec(txn)
            .await?;
        Ok(())
    }
}

#[async_trait]
#[allow(dead_code)]
impl PlayerProfileRepoTrait for PlayerProfileRepository {
    async fn find_by_user_id(&self, user_id: Uuid) -> Result<Option<PlayerProfile>, DbErr> {
        self.find_by_user_id(user_id).await
    }

    async fn find_by_user_ids(&self, user_ids: &[Uuid]) -> Result<Vec<PlayerProfile>, DbErr> {
        self.find_by_user_ids(user_ids).await
    }

    async fn update_stats(
        &self,
        user_id: Uuid,
        wins_delta: i32,
        kora_wins_delta: i32,
    ) -> Result<PlayerProfile, DbErr> {
        self.update_stats(user_id, wins_delta, kora_wins_delta)
            .await
    }
}
