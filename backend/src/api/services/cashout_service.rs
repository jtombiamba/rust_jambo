use std::sync::Arc;
use uuid::Uuid;

use crate::api::dto::responses::{
    CashoutHistoryItem, CashoutHistoryResponse, CashoutRequestResponse,
};
use crate::config::Config;
use crate::database::traits::{CashoutRepoTrait, CashoutRequestError};
use crate::error::AppError;

pub struct CashoutService<R: CashoutRepoTrait> {
    repo: Arc<R>,
    config: Config,
}

/// Trait seam for the cashout service, enabling handler-level testing.
#[async_trait::async_trait]
pub trait CashoutServiceTrait: Send + Sync {
    async fn request_cashout(
        &self,
        user_id: Uuid,
        credits: i32,
        paypal_email: &str,
    ) -> Result<CashoutRequestResponse, AppError>;

    async fn list_cashouts(
        &self,
        user_id: Uuid,
        page: u64,
        per_page: u64,
    ) -> Result<CashoutHistoryResponse, AppError>;
}

impl<R: CashoutRepoTrait> CashoutService<R> {
    pub fn new(repo: Arc<R>, config: Config) -> Self {
        Self { repo, config }
    }

    pub async fn request_cashout(
        &self,
        user_id: Uuid,
        credits: i32,
        paypal_email: &str,
    ) -> Result<CashoutRequestResponse, AppError> {
        if !self.config.cashout_enabled {
            return Err(AppError::Forbidden("cashout.unavailable"));
        }

        validate_cashout_credits(credits, &self.config)?;

        if !is_valid_email(paypal_email) {
            return Err(AppError::BadRequest("cashout.invalid_email"));
        }

        let amount_eur_cents = credits / self.config.cashout_credits_per_eur * 100;
        let request = self
            .repo
            .request(user_id, credits, amount_eur_cents, paypal_email)
            .await
            .map_err(map_request_error)?;

        Ok(CashoutRequestResponse {
            id: request.id,
            credits: request.credits,
            amount_eur_cents: request.amount_eur_cents,
            status: request.status.to_string(),
        })
    }

    pub async fn list_cashouts(
        &self,
        user_id: Uuid,
        page: u64,
        per_page: u64,
    ) -> Result<CashoutHistoryResponse, AppError> {
        let (requests, total) = self
            .repo
            .list_paginated(user_id, page, per_page)
            .await
            .map_err(AppError::Database)?;

        let items = requests
            .into_iter()
            .map(|r| CashoutHistoryItem {
                id: r.id,
                credits: r.credits,
                amount_eur_cents: r.amount_eur_cents,
                status: r.status.to_string(),
                paypal_email: r.paypal_email,
                created_at: r.created_at.to_rfc3339(),
                processed_at: r.processed_at.map(|t| t.to_rfc3339()),
            })
            .collect();

        Ok(CashoutHistoryResponse {
            items,
            total,
            page,
            per_page,
        })
    }
}

#[async_trait::async_trait]
impl<R: CashoutRepoTrait> CashoutServiceTrait for CashoutService<R> {
    async fn request_cashout(
        &self,
        user_id: Uuid,
        credits: i32,
        paypal_email: &str,
    ) -> Result<CashoutRequestResponse, AppError> {
        CashoutService::request_cashout(self, user_id, credits, paypal_email).await
    }

    async fn list_cashouts(
        &self,
        user_id: Uuid,
        page: u64,
        per_page: u64,
    ) -> Result<CashoutHistoryResponse, AppError> {
        CashoutService::list_cashouts(self, user_id, page, per_page).await
    }
}

fn map_request_error(e: CashoutRequestError) -> AppError {
    match e {
        CashoutRequestError::Locked => AppError::Conflict("cashout.locked"),
        CashoutRequestError::InsufficientCredits => {
            AppError::Conflict("cashout.insufficient_credits")
        }
        CashoutRequestError::ProfileNotFound => AppError::NotFound("payment.profile_not_found"),
        CashoutRequestError::Db(e) => AppError::Database(e),
    }
}

pub fn validate_cashout_credits(credits: i32, config: &Config) -> Result<(), AppError> {
    validate_credits(
        credits,
        config.cashout_min_credits,
        config.cashout_credits_per_eur,
        config.cashout_max_eur_cents,
    )
}

pub fn validate_credits(
    credits: i32,
    min_credits: i32,
    credits_per_eur: i32,
    max_eur_cents: i32,
) -> Result<(), AppError> {
    if credits_per_eur <= 0 {
        return Err(AppError::Internal("Invalid cashout configuration".into()));
    }
    if credits < min_credits {
        return Err(AppError::BadRequestParams {
            key: "cashout.minimum",
            params: vec![("{credits}", min_credits.to_string())],
        });
    }
    if credits % credits_per_eur != 0 {
        return Err(AppError::BadRequestParams {
            key: "cashout.multiple",
            params: vec![("{credits}", credits_per_eur.to_string())],
        });
    }
    let amount_eur_cents = credits / credits_per_eur * 100;
    if amount_eur_cents > max_eur_cents {
        return Err(AppError::BadRequestParams {
            key: "cashout.maximum",
            params: vec![("{euros}", (max_eur_cents / 100).to_string())],
        });
    }
    Ok(())
}

pub(crate) fn is_valid_email(email: &str) -> bool {
    let trimmed = email.trim();
    if trimmed.len() < 3 || trimmed.len() > 254 {
        return false;
    }
    let parts: Vec<&str> = trimmed.splitn(2, '@').collect();
    if parts.len() != 2 {
        return false;
    }
    let local = parts[0];
    let domain = parts[1];
    !local.is_empty()
        && !domain.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
}
