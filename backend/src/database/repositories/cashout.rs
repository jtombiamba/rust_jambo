use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, DatabaseTransaction, DbErr, EntityTrait,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set, TransactionTrait,
};
use uuid::Uuid;

use crate::database::models::{cashout_request, player_profile, CashoutRequest, CashoutStatus};
use crate::database::traits::{CashoutRepoTrait, CashoutRequestError};

#[derive(Debug, Clone)]
pub struct CashoutRepository {
    connection: DatabaseConnection,
}

#[allow(dead_code)]
impl CashoutRepository {
    pub fn new(connection: DatabaseConnection) -> Self {
        Self { connection }
    }

    /// Atomically reserve credits, create the request, and lock the account.
    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn request(
        &self,
        user_id: Uuid,
        credits: i32,
        amount_eur_cents: i32,
        paypal_email: &str,
    ) -> Result<CashoutRequest, CashoutRequestError> {
        let txn = self.connection.begin().await?;

        let profile = player_profile::Entity::find()
            .filter(player_profile::Column::UserId.eq(user_id))
            .one(&txn)
            .await?
            .ok_or(CashoutRequestError::ProfileNotFound)?;

        // Early returns below drop `txn`, which rolls the transaction back
        // automatically (SeaORM's DatabaseTransaction::drop). No partial write
        // is ever persisted.
        if profile.cashout_locked {
            return Err(CashoutRequestError::Locked);
        }

        let now = chrono::Utc::now();
        let debited = self.debit_in_txn(&txn, user_id, credits, now).await?;
        if debited == 0 {
            return Err(CashoutRequestError::InsufficientCredits);
        }

        let request = self
            .create_in_txn(&txn, user_id, credits, amount_eur_cents, paypal_email)
            .await?;

        self.lock_user_in_txn(&txn, user_id, now).await?;

        txn.commit().await?;

        Ok(request)
    }

    async fn debit_in_txn(
        &self,
        txn: &DatabaseTransaction,
        user_id: Uuid,
        amount: i32,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<u64, DbErr> {
        use sea_orm::sea_query::{Expr, ExprTrait};
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

    async fn lock_user_in_txn(
        &self,
        txn: &DatabaseTransaction,
        user_id: Uuid,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), DbErr> {
        use sea_orm::sea_query::Expr;
        player_profile::Entity::update_many()
            .col_expr(player_profile::Column::CashoutLocked, Expr::value(true))
            .col_expr(player_profile::Column::UpdatedAt, Expr::value(now))
            .filter(player_profile::Column::UserId.eq(user_id))
            .exec(txn)
            .await?;
        Ok(())
    }

    #[tracing::instrument(skip(txn), fields(db.statement, db.rows_affected))]
    pub async fn create_in_txn(
        &self,
        txn: &DatabaseTransaction,
        user_id: Uuid,
        credits: i32,
        amount_eur_cents: i32,
        paypal_email: &str,
    ) -> Result<CashoutRequest, DbErr> {
        let now = chrono::Utc::now();
        cashout_request::ActiveModel {
            id: Set(Uuid::now_v7()),
            user_id: Set(user_id),
            credits: Set(credits),
            amount_eur_cents: Set(amount_eur_cents),
            status: Set(CashoutStatus::Requested),
            paypal_email: Set(paypal_email.to_string()),
            paypal_payout_batch_id: Set(None),
            admin_note: Set(None),
            processed_by: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
            processed_at: Set(None),
        }
        .insert(txn)
        .await
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn find_by_id(&self, id: Uuid) -> Result<Option<CashoutRequest>, DbErr> {
        cashout_request::Entity::find_by_id(id)
            .one(&self.connection)
            .await
    }

    #[tracing::instrument(skip(txn), fields(db.statement, db.rows_affected))]
    pub async fn find_by_id_in_txn(
        &self,
        txn: &DatabaseTransaction,
        id: Uuid,
    ) -> Result<Option<CashoutRequest>, DbErr> {
        cashout_request::Entity::find_by_id(id).one(txn).await
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn list_paginated(
        &self,
        status: Option<CashoutStatus>,
        user_id: Option<Uuid>,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<CashoutRequest>, u64), DbErr> {
        let mut query = cashout_request::Entity::find();
        if let Some(status) = status {
            query = query.filter(cashout_request::Column::Status.eq(status));
        }
        if let Some(user_id) = user_id {
            query = query.filter(cashout_request::Column::UserId.eq(user_id));
        }

        let total = query.clone().count(&self.connection).await?;
        let offset = page.saturating_sub(1) * per_page;
        let items = query
            .order_by_desc(cashout_request::Column::CreatedAt)
            .offset(offset)
            .limit(per_page)
            .all(&self.connection)
            .await?;

        Ok((items, total))
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn list_requested_older_than(
        &self,
        cutoff: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<CashoutRequest>, DbErr> {
        cashout_request::Entity::find()
            .filter(cashout_request::Column::Status.eq(CashoutStatus::Requested))
            .filter(cashout_request::Column::CreatedAt.lte(cutoff))
            .all(&self.connection)
            .await
    }

    /// Auto-reject every requested cashout older than `cutoff`, refunding the
    /// reserved credits and unlocking the account. Returns the ids rejected.
    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn auto_reject_expired(
        &self,
        cutoff: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<Uuid>, DbErr> {
        let requests = self.list_requested_older_than(cutoff).await?;
        let mut rejected = Vec::new();
        for req in requests {
            let txn = self.connection.begin().await?;
            let Some(current) = self.find_by_id_in_txn(&txn, req.id).await? else {
                txn.rollback().await.ok();
                continue;
            };
            if current.status != CashoutStatus::Requested {
                txn.rollback().await.ok();
                continue;
            }
            if let Err(e) = self
                .reject_in_txn(
                    &txn,
                    &current,
                    "auto-rejected: no admin decision within the allowed window",
                    "system",
                )
                .await
            {
                txn.rollback().await.ok();
                tracing::error!("Failed to auto-reject cashout {}: {}", current.id, e);
                continue;
            }
            txn.commit().await?;
            rejected.push(current.id);
        }
        Ok(rejected)
    }

    /// Approve a request (unlock account, set status approved).
    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn approve(&self, id: Uuid, note: &str, by: &str) -> Result<(), DbErr> {
        let txn = self.connection.begin().await?;
        let request = self
            .find_by_id_in_txn(&txn, id)
            .await?
            .ok_or_else(|| DbErr::RecordNotFound("cashout request".into()))?;
        if request.status != CashoutStatus::Requested {
            txn.rollback().await.ok();
            return Err(DbErr::Custom(
                "request is not in 'requested' status".to_string(),
            ));
        }
        self.approve_in_txn(&txn, &request, note, by).await?;
        txn.commit().await?;
        Ok(())
    }

    /// Reject a request (refund credits, unlock account, set status rejected).
    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn reject(&self, id: Uuid, note: &str, by: &str) -> Result<(), DbErr> {
        let txn = self.connection.begin().await?;
        let request = self
            .find_by_id_in_txn(&txn, id)
            .await?
            .ok_or_else(|| DbErr::RecordNotFound("cashout request".into()))?;
        if !matches!(
            request.status,
            CashoutStatus::Requested | CashoutStatus::Approved
        ) {
            txn.rollback().await.ok();
            return Err(DbErr::Custom(
                "request cannot be rejected from its current status".to_string(),
            ));
        }
        self.reject_in_txn(&txn, &request, note, by).await?;
        txn.commit().await?;
        Ok(())
    }

    /// Mark a request paid with the PayPal payout batch id.
    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn mark_paid(&self, id: Uuid, payout_batch_id: &str, by: &str) -> Result<(), DbErr> {
        let txn = self.connection.begin().await?;
        let request = self
            .find_by_id_in_txn(&txn, id)
            .await?
            .ok_or_else(|| DbErr::RecordNotFound("cashout request".into()))?;
        if request.status != CashoutStatus::Approved {
            txn.rollback().await.ok();
            return Err(DbErr::Custom(
                "request must be 'approved' before it can be paid".to_string(),
            ));
        }
        self.mark_paid_in_txn(&txn, &request, payout_batch_id, by)
            .await?;
        txn.commit().await?;
        Ok(())
    }

    /// Approve a pending request and unlock the account.
    #[tracing::instrument(skip(txn), fields(db.statement, db.rows_affected))]
    pub async fn approve_in_txn(
        &self,
        txn: &DatabaseTransaction,
        request: &CashoutRequest,
        note: &str,
        processed_by: &str,
    ) -> Result<(), DbErr> {
        self.finalize_in_txn(txn, request, CashoutStatus::Approved, note, processed_by)
            .await
    }

    /// Reject a pending request, refund the reserved credits, and unlock the
    /// account.
    #[tracing::instrument(skip(txn), fields(db.statement, db.rows_affected))]
    pub async fn reject_in_txn(
        &self,
        txn: &DatabaseTransaction,
        request: &CashoutRequest,
        note: &str,
        processed_by: &str,
    ) -> Result<(), DbErr> {
        self.credit_user_in_txn(txn, request.user_id, request.credits)
            .await?;
        self.finalize_in_txn(txn, request, CashoutStatus::Rejected, note, processed_by)
            .await
    }

    /// Mark a request paid with the PayPal payout batch id.
    #[tracing::instrument(skip(txn), fields(db.statement, db.rows_affected))]
    pub async fn mark_paid_in_txn(
        &self,
        txn: &DatabaseTransaction,
        request: &CashoutRequest,
        payout_batch_id: &str,
        processed_by: &str,
    ) -> Result<(), DbErr> {
        let now = chrono::Utc::now();
        cashout_request::Entity::update_many()
            .col_expr(
                cashout_request::Column::Status,
                sea_orm::sea_query::Expr::value(CashoutStatus::Paid),
            )
            .col_expr(
                cashout_request::Column::PaypalPayoutBatchId,
                sea_orm::sea_query::Expr::value(payout_batch_id.to_string()),
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

    async fn finalize_in_txn(
        &self,
        txn: &DatabaseTransaction,
        request: &CashoutRequest,
        status: CashoutStatus,
        note: &str,
        processed_by: &str,
    ) -> Result<(), DbErr> {
        let now = chrono::Utc::now();
        cashout_request::Entity::update_many()
            .col_expr(
                cashout_request::Column::Status,
                sea_orm::sea_query::Expr::value(status),
            )
            .col_expr(
                cashout_request::Column::AdminNote,
                sea_orm::sea_query::Expr::value(note.to_string()),
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
            .filter(
                cashout_request::Column::Status
                    .is_in([CashoutStatus::Requested, CashoutStatus::Approved]),
            )
            .exec(txn)
            .await?;
        self.unlock_user_in_txn(txn, request.user_id).await?;
        Ok(())
    }

    async fn credit_user_in_txn(
        &self,
        txn: &DatabaseTransaction,
        user_id: Uuid,
        amount: i32,
    ) -> Result<(), DbErr> {
        use sea_orm::sea_query::{Expr, ExprTrait};
        player_profile::Entity::update_many()
            .col_expr(
                player_profile::Column::Credit,
                Expr::col(player_profile::Column::Credit).add(amount),
            )
            .col_expr(
                player_profile::Column::UpdatedAt,
                Expr::value(chrono::Utc::now()),
            )
            .filter(player_profile::Column::UserId.eq(user_id))
            .exec(txn)
            .await?;
        Ok(())
    }

    async fn unlock_user_in_txn(
        &self,
        txn: &DatabaseTransaction,
        user_id: Uuid,
    ) -> Result<(), DbErr> {
        use sea_orm::sea_query::Expr;
        player_profile::Entity::update_many()
            .col_expr(player_profile::Column::CashoutLocked, Expr::value(false))
            .col_expr(
                player_profile::Column::UpdatedAt,
                Expr::value(chrono::Utc::now()),
            )
            .filter(player_profile::Column::UserId.eq(user_id))
            .exec(txn)
            .await?;
        Ok(())
    }
}

#[async_trait::async_trait]
#[allow(dead_code)]
impl CashoutRepoTrait for CashoutRepository {
    async fn list_paginated(
        &self,
        user_id: Uuid,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<CashoutRequest>, u64), DbErr> {
        self.list_paginated(None, Some(user_id), page, per_page)
            .await
    }

    async fn request(
        &self,
        user_id: Uuid,
        credits: i32,
        amount_eur_cents: i32,
        paypal_email: &str,
    ) -> Result<CashoutRequest, CashoutRequestError> {
        self.request(user_id, credits, amount_eur_cents, paypal_email)
            .await
    }
}
