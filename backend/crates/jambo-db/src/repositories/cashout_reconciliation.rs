//! Cashout payout reconciliation: transition a request to "processing" once the
//! transfer is submitted, then to "paid"/"rejected" based on the provider
//! webhook outcome. Split out of `cashout.rs` to keep that file within the line
//! limit.

use sea_orm::{
    sea_query::Expr, ColumnTrait, DatabaseTransaction, DbErr, EntityTrait, QueryFilter,
    TransactionTrait,
};
use uuid::Uuid;

use crate::models::{cashout_request, CashoutRequest, CashoutStatus};
use crate::repositories::cashout::CashoutRepository;

impl CashoutRepository {
    /// Transition an approved request to "processing" after the payout (transfer)
    /// has been submitted to the provider, recording the reference. The final
    /// "paid"/"failed" transition is driven by the provider webhook.
    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn mark_processing(
        &self,
        id: Uuid,
        payout_reference: &str,
        by: &str,
    ) -> Result<(), DbErr> {
        let txn = self.connection.begin().await?;
        let request = self
            .find_by_id_in_txn(&txn, id)
            .await?
            .ok_or_else(|| DbErr::RecordNotFound("cashout request".into()))?;
        if request.status != CashoutStatus::Approved {
            txn.rollback().await.ok();
            return Err(DbErr::Custom(
                "request must be 'approved' before it can be marked processing".to_string(),
            ));
        }
        self.mark_processing_in_txn(&txn, &request, payout_reference, by)
            .await?;
        txn.commit().await?;
        Ok(())
    }

    /// Mark a "processing" request as paid once the provider webhook confirms the
    /// transfer succeeded. Idempotent for already-paid requests.
    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn mark_paid_by_id(&self, id: Uuid) -> Result<(), DbErr> {
        let txn = self.connection.begin().await?;
        let request = self
            .find_by_id_in_txn(&txn, id)
            .await?
            .ok_or_else(|| DbErr::RecordNotFound("cashout request".into()))?;
        match request.status {
            CashoutStatus::Paid => {
                txn.rollback().await.ok();
                return Ok(());
            }
            CashoutStatus::Processing => {}
            _ => {
                txn.rollback().await.ok();
                return Err(DbErr::Custom(
                    "request must be 'processing' before it can be marked paid".to_string(),
                ));
            }
        }
        let reference = request.payout_reference.clone().unwrap_or_default();
        self.mark_paid_in_txn(
            &txn,
            &request,
            &reference,
            "stripe-webhook",
            CashoutStatus::Processing,
        )
        .await?;
        txn.commit().await?;
        Ok(())
    }

    /// Refund the reserved credits, unlock the account, and mark a "processing"
    /// request as rejected after the provider webhook reports a failed transfer.
    /// Idempotent for already-rejected requests.
    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn fail_and_refund(&self, id: Uuid, note: &str) -> Result<(), DbErr> {
        let txn = self.connection.begin().await?;
        let request = self
            .find_by_id_in_txn(&txn, id)
            .await?
            .ok_or_else(|| DbErr::RecordNotFound("cashout request".into()))?;
        match request.status {
            CashoutStatus::Rejected => {
                txn.rollback().await.ok();
                return Ok(());
            }
            CashoutStatus::Processing => {}
            _ => {
                txn.rollback().await.ok();
                return Err(DbErr::Custom(
                    "request must be 'processing' before it can be failed".to_string(),
                ));
            }
        }
        self.credit_user_in_txn(&txn, request.user_id, request.credits)
            .await?;
        self.finalize_in_txn(
            &txn,
            &request,
            CashoutStatus::Rejected,
            note,
            "stripe-webhook",
        )
        .await?;
        txn.commit().await?;
        Ok(())
    }

    /// Mark a request processing with the payout reference (Stripe transfer id).
    #[tracing::instrument(skip(txn), fields(db.statement, db.rows_affected))]
    pub async fn mark_processing_in_txn(
        &self,
        txn: &DatabaseTransaction,
        request: &CashoutRequest,
        payout_reference: &str,
        processed_by: &str,
    ) -> Result<(), DbErr> {
        let now = chrono::Utc::now();
        let result = cashout_request::Entity::update_many()
            .col_expr(
                cashout_request::Column::Status,
                Expr::cust_with_values(
                    "$1::cashout_status",
                    [sea_orm::Value::String(Some(
                        CashoutStatus::Processing.to_string(),
                    ))],
                ),
            )
            .col_expr(
                cashout_request::Column::PayoutReference,
                sea_orm::sea_query::Expr::value(payout_reference.to_string()),
            )
            .col_expr(
                cashout_request::Column::ProcessedBy,
                sea_orm::sea_query::Expr::value(processed_by.to_string()),
            )
            .col_expr(
                cashout_request::Column::ProcessedAt,
                sea_orm::sea_query::Expr::value(now),
            )
            .col_expr(
                cashout_request::Column::UpdatedAt,
                sea_orm::sea_query::Expr::value(now),
            )
            .filter(cashout_request::Column::Id.eq(request.id))
            .filter(cashout_request::Column::Status.eq(CashoutStatus::Approved))
            .exec(txn)
            .await?;
        // Optimistic lock: the row must still be in `approved` when the update
        // runs. If a concurrent request already transitioned it, no row matches
        // and we must not overwrite its payout reference.
        if result.rows_affected != 1 {
            return Err(DbErr::Custom(
                "request must be 'approved' before it can be marked processing".to_string(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::PayoutMethod;
    use sea_orm::{DatabaseBackend, MockDatabase, MockExecResult, TransactionTrait};

    fn make_request(status: CashoutStatus) -> CashoutRequest {
        CashoutRequest {
            id: Uuid::now_v7(),
            user_id: Uuid::now_v7(),
            credits: 100,
            amount_eur_cents: 1_000,
            status,
            payout_method: PayoutMethod::StripeConnect,
            payout_destination: "acct_123".to_string(),
            payout_reference: None,
            admin_note: None,
            processed_by: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            processed_at: None,
        }
    }

    #[tokio::test]
    async fn mark_processing_in_txn_succeeds_when_row_updated() {
        let db = MockDatabase::new(DatabaseBackend::Postgres)
            .append_exec_results(vec![MockExecResult {
                last_insert_id: 0,
                rows_affected: 1,
            }])
            .into_connection();
        let repo = CashoutRepository::new(db.clone());
        let txn = db.begin().await.unwrap();

        let result = repo
            .mark_processing_in_txn(
                &txn,
                &make_request(CashoutStatus::Approved),
                "tr_1",
                "admin",
            )
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn mark_processing_in_txn_errors_when_row_not_updated() {
        let db = MockDatabase::new(DatabaseBackend::Postgres)
            .append_exec_results(vec![MockExecResult {
                last_insert_id: 0,
                rows_affected: 0,
            }])
            .into_connection();
        let repo = CashoutRepository::new(db.clone());
        let txn = db.begin().await.unwrap();

        let result = repo
            .mark_processing_in_txn(
                &txn,
                &make_request(CashoutStatus::Approved),
                "tr_1",
                "admin",
            )
            .await;

        assert!(matches!(result, Err(DbErr::Custom(_))));
    }
}
