use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, DatabaseTransaction, DbErr, EntityTrait,
    QueryFilter, Set, TryInsertResult,
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
            order_id: Set(None),
            created_at: Set(now),
        }
        .insert(&self.connection)
        .await
    }

    /// Whether a transaction with the given PayPal `order_id` already exists.
    ///
    /// This is the fast-path idempotency guard for the unauthenticated return
    /// endpoint: it short-circuits replays before they ever reach PayPal or the
    /// credit ledger. The `UNIQUE` constraint on `order_id` remains the
    /// authoritative backstop for concurrent requests.
    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn exists_by_order_id(&self, order_id: &str) -> Result<bool, DbErr> {
        let row = topup_transaction::Entity::find()
            .filter(topup_transaction::Column::OrderId.eq(order_id))
            .one(&self.connection)
            .await?;
        Ok(row.is_some())
    }

    /// Insert a top-up transaction keyed by its PayPal `order_id`, skipping the
    /// insert (and returning `None`) if a row with the same `order_id` already
    /// exists.
    ///
    /// The `ON CONFLICT (order_id) DO NOTHING` clause relies on the unique
    /// index created in `m20260924_000001_topup_order_id_unique`. A `Some(id)`
    /// result means this call inserted the row (and is therefore responsible
    /// for crediting); `None` means another request already recorded the order.
    #[tracing::instrument(skip(txn), fields(db.statement, db.rows_affected))]
    pub async fn insert_order_if_absent_in_txn(
        &self,
        txn: &DatabaseTransaction,
        user_id: Uuid,
        kind: TopupTransactionKind,
        amount_eur_cents: i32,
        credits: i32,
        order_id: &str,
    ) -> Result<Option<Uuid>, DbErr> {
        let id = Uuid::now_v7();
        let now = chrono::Utc::now();
        let result = topup_transaction::Entity::insert(topup_transaction::ActiveModel {
            id: Set(id),
            user_id: Set(user_id),
            kind: Set(kind),
            amount_eur_cents: Set(amount_eur_cents),
            credits: Set(credits),
            order_id: Set(Some(order_id.to_string())),
            created_at: Set(now),
        })
        .on_conflict_do_nothing_on([topup_transaction::Column::OrderId])
        .exec_without_returning(txn)
        .await?;

        match result {
            TryInsertResult::Inserted(1) => Ok(Some(id)),
            TryInsertResult::Inserted(_) | TryInsertResult::Conflicted | TryInsertResult::Empty => {
                Ok(None)
            }
        }
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
    use sea_orm::{DatabaseBackend, MockDatabase, MockExecResult, TransactionTrait};

    fn make_tx(user_id: Uuid, amount_eur_cents: i32) -> TopupTransaction {
        TopupTransaction {
            id: Uuid::now_v7(),
            user_id,
            kind: TopupTransactionKind::Topup,
            amount_eur_cents,
            credits: amount_eur_cents,
            order_id: None,
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
                order_id: None,
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

    #[tokio::test]
    async fn exists_by_order_id_returns_true_when_row_present() {
        let db = MockDatabase::new(DatabaseBackend::Postgres)
            .append_query_results(vec![vec![make_tx(Uuid::now_v7(), 100)]])
            .into_connection();
        let repo = TopupTransactionRepository::new(db);

        let exists = repo.exists_by_order_id("ORDER_1").await.unwrap();

        assert!(exists);
    }

    #[tokio::test]
    async fn exists_by_order_id_returns_false_when_absent() {
        let db = MockDatabase::new(DatabaseBackend::Postgres)
            .append_query_results(vec![Vec::<TopupTransaction>::new()])
            .into_connection();
        let repo = TopupTransactionRepository::new(db);

        let exists = repo.exists_by_order_id("ORDER_1").await.unwrap();

        assert!(!exists);
    }

    #[tokio::test]
    async fn insert_order_if_absent_returns_id_on_first_insert() {
        let db = MockDatabase::new(DatabaseBackend::Postgres)
            .append_exec_results(vec![MockExecResult {
                last_insert_id: 0,
                rows_affected: 1,
            }])
            .into_connection();
        let repo = TopupTransactionRepository::new(db.clone());
        let txn = db.begin().await.unwrap();

        let inserted = repo
            .insert_order_if_absent_in_txn(
                &txn,
                Uuid::now_v7(),
                TopupTransactionKind::Topup,
                100,
                250,
                "ORDER_1",
            )
            .await
            .unwrap();

        assert!(inserted.is_some());
    }

    #[tokio::test]
    async fn insert_order_if_absent_returns_none_on_conflict() {
        let db = MockDatabase::new(DatabaseBackend::Postgres)
            .append_exec_results(vec![MockExecResult {
                last_insert_id: 0,
                rows_affected: 0,
            }])
            .into_connection();
        let repo = TopupTransactionRepository::new(db.clone());
        let txn = db.begin().await.unwrap();

        let inserted = repo
            .insert_order_if_absent_in_txn(
                &txn,
                Uuid::now_v7(),
                TopupTransactionKind::Topup,
                100,
                250,
                "ORDER_1",
            )
            .await
            .unwrap();

        assert!(inserted.is_none());
    }
}
