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
        self.mark_paid_in_txn(&txn, &request, &reference, "stripe-webhook")
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
        cashout_request::Entity::update_many()
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
            .exec(txn)
            .await?;
        Ok(())
    }
}
