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

    #[tracing::instrument(skip(txn), fields(db.statement, db.rows_affected))]
    pub async fn find_by_user_id_in_txn(
        &self,
        txn: &DatabaseTransaction,
        user_id: Uuid,
    ) -> Result<Option<PlayerProfile>, DbErr> {
        player_profile::Entity::find()
            .filter(player_profile::Column::UserId.eq(user_id))
            .one(txn)
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
    ) -> Result<u64, DbErr> {
        use sea_orm::sea_query::Expr;
        use sea_orm::sea_query::ExprTrait;
        let result = player_profile::Entity::update_many()
            .col_expr(
                player_profile::Column::Credit,
                Expr::col(player_profile::Column::Credit).sub(amount),
            )
            .col_expr(player_profile::Column::UpdatedAt, Expr::value(now))
            .filter(player_profile::Column::UserId.eq(user_id))
            .filter(player_profile::Column::Credit.gte(amount))
            .exec(txn)
            .await?;
        Ok(result.rows_affected)
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

    /// Atomically set a profile's credit and freeze state, guarding on the
    /// credit value read before the transaction. Returns the number of rows
    /// affected so the caller can detect a concurrent modification (0 rows).
    #[tracing::instrument(skip(txn), fields(db.statement, db.rows_affected))]
    pub async fn update_credit_optimistic_in_txn(
        &self,
        txn: &DatabaseTransaction,
        user_id: Uuid,
        final_credit: i32,
        frozen_until: Option<chrono::DateTime<chrono::Utc>>,
        read_credit: i32,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<u64, DbErr> {
        use sea_orm::sea_query::Expr;
        use sea_orm::Value;

        let result = player_profile::Entity::update_many()
            .col_expr(
                player_profile::Column::Credit,
                Expr::value(Value::Int(Some(final_credit))),
            )
            .col_expr(
                player_profile::Column::FrozenUntil,
                Expr::value(Value::ChronoDateTimeUtc(frozen_until)),
            )
            .col_expr(
                player_profile::Column::UpdatedAt,
                Expr::value(Value::ChronoDateTimeUtc(Some(now))),
            )
            .filter(player_profile::Column::UserId.eq(user_id))
            .filter(player_profile::Column::Credit.eq(read_credit))
            .exec(txn)
            .await?;
        Ok(result.rows_affected)
    }

    /// Apply a single game settlement to a live profile inside a transaction.
    ///
    /// The credit change is a pure atomic SQL increment (`credit = credit + delta`),
    /// so concurrent settlements never lose updates. The freeze decision is driven by
    /// the live resulting credit (`profile.credit + delta`) read inside the transaction,
    /// never by the stale `player.credits` snapshot.
    #[tracing::instrument(skip(txn), fields(db.statement, db.rows_affected))]
    pub async fn apply_game_settlement_in_txn(
        &self,
        txn: &DatabaseTransaction,
        user_id: Uuid,
        delta: i32,
        won: bool,
        is_kora: bool,
        freeze_duration_secs: u64,
    ) -> Result<Option<i32>, DbErr> {
        use sea_orm::sea_query::{Expr, ExprTrait};
        use sea_orm::Value;

        let now = chrono::Utc::now();

        let profile = player_profile::Entity::find()
            .filter(player_profile::Column::UserId.eq(user_id))
            .one(txn)
            .await?;

        let Some(profile) = profile else {
            return Ok(None);
        };

        let new_credit = profile.credit + delta;
        let frozen_until = settle_frozen_until(new_credit, now, freeze_duration_secs);

        player_profile::Entity::update_many()
            .col_expr(
                player_profile::Column::Credit,
                Expr::col(player_profile::Column::Credit).add(delta),
            )
            .col_expr(
                player_profile::Column::GamePlayed,
                Expr::col(player_profile::Column::GamePlayed).add(1),
            )
            .col_expr(
                player_profile::Column::Wins,
                Expr::col(player_profile::Column::Wins).add(if won { 1 } else { 0 }),
            )
            .col_expr(
                player_profile::Column::KoraWins,
                Expr::col(player_profile::Column::KoraWins).add(if won && is_kora { 1 } else { 0 }),
            )
            .col_expr(
                player_profile::Column::WinningStreak,
                if won {
                    Expr::col(player_profile::Column::WinningStreak).add(1)
                } else {
                    Expr::value(0)
                },
            )
            .col_expr(
                player_profile::Column::FrozenUntil,
                Expr::value(Value::ChronoDateTimeUtc(frozen_until)),
            )
            .col_expr(
                player_profile::Column::UpdatedAt,
                Expr::value(Value::ChronoDateTimeUtc(Some(now))),
            )
            .filter(player_profile::Column::UserId.eq(user_id))
            .exec(txn)
            .await?;

        Ok(Some(new_credit))
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

    async fn apply_game_settlement_in_txn(
        &self,
        txn: &DatabaseTransaction,
        user_id: Uuid,
        delta: i32,
        won: bool,
        is_kora: bool,
        freeze_duration_secs: u64,
    ) -> Result<Option<i32>, DbErr> {
        self.apply_game_settlement_in_txn(txn, user_id, delta, won, is_kora, freeze_duration_secs)
            .await
    }
}

/// Compute the `frozen_until` timestamp for a settlement, driven by the live
/// resulting credit. A resulting credit of `<= 0` freezes the profile; any
/// positive credit leaves it unfrozen.
fn settle_frozen_until(
    new_credit: i32,
    now: chrono::DateTime<chrono::Utc>,
    freeze_duration_secs: u64,
) -> Option<chrono::DateTime<chrono::Utc>> {
    if new_credit <= 0 {
        Some(now + chrono::Duration::seconds(freeze_duration_secs as i64))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::settle_frozen_until;

    fn now() -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::from_timestamp(1_700_000_000, 0).unwrap()
    }

    #[test]
    fn freeze_when_credit_hits_zero() {
        let until = settle_frozen_until(0, now(), 86_400);
        assert_eq!(until, Some(now() + chrono::Duration::seconds(86_400)));
    }

    #[test]
    fn freeze_when_credit_goes_negative() {
        let until = settle_frozen_until(-10, now(), 86_400);
        assert_eq!(until, Some(now() + chrono::Duration::seconds(86_400)));
    }

    #[test]
    fn no_freeze_when_credit_stays_positive() {
        assert_eq!(settle_frozen_until(1, now(), 86_400), None);
        assert_eq!(settle_frozen_until(100, now(), 86_400), None);
    }

    #[test]
    fn freeze_duration_is_respected() {
        let until = settle_frozen_until(-5, now(), 3_600).unwrap();
        assert_eq!(until, now() + chrono::Duration::seconds(3_600));
    }
}
