use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter, Set,
};
use uuid::Uuid;

use crate::database::models::{topup_transaction, TopupTransaction, TopupTransactionKind};

/// Persistence for successful PayPal top-up / unfreeze payments. Kept separate
/// from the game repositories because it backs the monthly spending cap rather
/// than the in-game credit ledger.
#[derive(Debug, Clone)]
pub struct TopupTransactionRepository {
    connection: DatabaseConnection,
}

impl TopupTransactionRepository {
    pub fn new(connection: DatabaseConnection) -> Self {
        Self { connection }
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn create(
        &self,
        user_id: Uuid,
        kind: TopupTransactionKind,
        amount_eur_cents: i32,
        credits: i32,
    ) -> Result<TopupTransaction, DbErr> {
        let now = chrono::Utc::now();
        topup_transaction::ActiveModel {
            id: Set(Uuid::now_v7()),
            user_id: Set(user_id),
            kind: Set(kind),
            amount_eur_cents: Set(amount_eur_cents),
            credits: Set(credits),
            created_at: Set(now),
        }
        .insert(&self.connection)
        .await
    }

    /// Total amount (in euro cents) spent by the user on or after `since`.
    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn sum_amount_eur_cents_since(
        &self,
        user_id: Uuid,
        since: chrono::DateTime<chrono::Utc>,
    ) -> Result<i32, DbErr> {
        let rows = topup_transaction::Entity::find()
            .filter(topup_transaction::Column::UserId.eq(user_id))
            .filter(topup_transaction::Column::CreatedAt.gte(since))
            .all(&self.connection)
            .await?;
        Ok(rows.iter().map(|r| r.amount_eur_cents).sum())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm::{DatabaseBackend, MockDatabase};

    fn make_tx(user_id: Uuid, amount_eur_cents: i32) -> TopupTransaction {
        TopupTransaction {
            id: Uuid::now_v7(),
            user_id,
            kind: TopupTransactionKind::Topup,
            amount_eur_cents,
            credits: amount_eur_cents,
            created_at: chrono::Utc::now(),
        }
    }

    #[tokio::test]
    async fn sum_amount_eur_cents_since_sums_matching_rows() {
        let user_id = Uuid::now_v7();
        let db = MockDatabase::new(DatabaseBackend::Postgres)
            .append_query_results(vec![vec![
                make_tx(user_id, 100),
                make_tx(user_id, 250),
                make_tx(user_id, 50),
            ]])
            .into_connection();
        let repo = TopupTransactionRepository::new(db);

        let total = repo
            .sum_amount_eur_cents_since(user_id, chrono::Utc::now())
            .await
            .unwrap();

        assert_eq!(total, 400);
    }

    #[tokio::test]
    async fn sum_amount_eur_cents_since_returns_zero_when_empty() {
        let db = MockDatabase::new(DatabaseBackend::Postgres)
            .append_query_results(vec![Vec::<TopupTransaction>::new()])
            .into_connection();
        let repo = TopupTransactionRepository::new(db);

        let total = repo
            .sum_amount_eur_cents_since(Uuid::now_v7(), chrono::Utc::now())
            .await
            .unwrap();

        assert_eq!(total, 0);
    }

    #[tokio::test]
    async fn create_inserts_transaction_with_fields_preserved() {
        let user_id = Uuid::now_v7();
        let db = MockDatabase::new(DatabaseBackend::Postgres)
            .append_query_results(vec![vec![TopupTransaction {
                id: Uuid::now_v7(),
                user_id,
                kind: TopupTransactionKind::Unfreeze,
                amount_eur_cents: 100,
                credits: 250,
                created_at: chrono::Utc::now(),
            }]])
            .into_connection();
        let repo = TopupTransactionRepository::new(db);

        let result = repo
            .create(user_id, TopupTransactionKind::Unfreeze, 100, 250)
            .await;

        let tx = result.expect("insert should succeed");
        assert_eq!(tx.kind, TopupTransactionKind::Unfreeze);
        assert_eq!(tx.amount_eur_cents, 100);
        assert_eq!(tx.credits, 250);
    }
}
