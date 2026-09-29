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

fn is_valid_email(email: &str) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_accepts_exact_multiple() {
        assert!(validate_credits(250, 250, 250, 2000).is_ok());
        assert!(validate_credits(500, 250, 250, 2000).is_ok());
    }

    #[test]
    fn validate_rejects_below_minimum() {
        assert!(validate_credits(249, 250, 250, 2000).is_err());
    }

    #[test]
    fn validate_rejects_non_multiple() {
        assert!(validate_credits(600, 250, 250, 2000).is_err());
    }

    #[test]
    fn validate_rejects_above_max() {
        assert!(validate_credits(5250, 250, 250, 2000).is_err());
    }

    #[test]
    fn validate_accepts_max() {
        assert!(validate_credits(5000, 250, 250, 2000).is_ok());
    }

    #[test]
    fn validate_rejects_zero_credits_per_eur() {
        assert!(matches!(
            validate_credits(250, 250, 0, 2000),
            Err(AppError::Internal(_))
        ));
    }

    #[test]
    fn validate_rejects_negative_credits_per_eur() {
        assert!(matches!(
            validate_credits(250, 250, -250, 2000),
            Err(AppError::Internal(_))
        ));
    }

    #[test]
    fn email_validation() {
        assert!(is_valid_email("a@b.co"));
        assert!(is_valid_email("user.name+tag@example.com"));
        assert!(!is_valid_email("not-an-email"));
        assert!(!is_valid_email("@."));
        assert!(!is_valid_email("@"));
        assert!(!is_valid_email("a@"));
        assert!(!is_valid_email("@b.co"));
        assert!(!is_valid_email("a@b"));
        assert!(!is_valid_email("a@.co"));
        assert!(!is_valid_email("a@b."));
    }
}

#[cfg(test)]
mod service_tests {
    use super::*;
    use crate::database::models::{CashoutRequest, CashoutStatus};
    use crate::database::traits::{CashoutRepoTrait, CashoutRequestError};
    use async_trait::async_trait;
    use sea_orm::DbErr;
    use std::sync::Mutex;

    struct MockRepo {
        fail: Mutex<bool>,
        request: Mutex<Option<CashoutRequest>>,
        list: Mutex<Vec<CashoutRequest>>,
    }

    fn request(credits: i32, status: CashoutStatus) -> crate::database::models::CashoutRequest {
        crate::database::models::CashoutRequest {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            credits,
            amount_eur_cents: credits / 250 * 100,
            status,
            paypal_email: "a@b.co".to_string(),
            paypal_payout_batch_id: None,
            admin_note: None,
            processed_by: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            processed_at: None,
        }
    }

    #[async_trait]
    impl CashoutRepoTrait for MockRepo {
        async fn list_paginated(
            &self,
            _user_id: Uuid,
            _page: u64,
            _per_page: u64,
        ) -> Result<(Vec<crate::database::models::CashoutRequest>, u64), DbErr> {
            let items = self.list.lock().unwrap().clone();
            let total = items.len() as u64;
            Ok((items, total))
        }

        async fn request(
            &self,
            _user_id: Uuid,
            _credits: i32,
            _amount_eur_cents: i32,
            _paypal_email: &str,
        ) -> Result<crate::database::models::CashoutRequest, CashoutRequestError> {
            if *self.fail.lock().unwrap() {
                return Err(CashoutRequestError::Locked);
            }
            Ok(self.request.lock().unwrap().clone().unwrap())
        }
    }

    fn enabled_config() -> Config {
        let mut cfg = Config::from_env().expect("config");
        cfg.cashout_enabled = true;
        cfg
    }

    #[tokio::test]
    async fn request_cashout_disabled_returns_forbidden() {
        let mut cfg = Config::from_env().expect("config");
        cfg.cashout_enabled = false;
        let repo = Arc::new(MockRepo {
            fail: Mutex::new(false),
            request: Mutex::new(Some(request(250, CashoutStatus::Requested))),
            list: Mutex::new(vec![]),
        });
        let service = CashoutService::new(repo, cfg);
        let err = service
            .request_cashout(Uuid::new_v4(), 250, "a@b.co")
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::Forbidden(_)));
    }

    #[tokio::test]
    async fn request_cashout_success_maps_response() {
        let repo = Arc::new(MockRepo {
            fail: Mutex::new(false),
            request: Mutex::new(Some(request(250, CashoutStatus::Requested))),
            list: Mutex::new(vec![]),
        });
        let service = CashoutService::new(repo, enabled_config());
        let resp = service
            .request_cashout(Uuid::new_v4(), 250, "a@b.co")
            .await
            .unwrap();
        assert_eq!(resp.credits, 250);
        assert_eq!(resp.amount_eur_cents, 100);
        assert_eq!(resp.status, "requested");
    }

    #[tokio::test]
    async fn request_cashout_maps_locked_error() {
        let repo = Arc::new(MockRepo {
            fail: Mutex::new(true),
            request: Mutex::new(None),
            list: Mutex::new(vec![]),
        });
        let service = CashoutService::new(repo, enabled_config());
        let err = service
            .request_cashout(Uuid::new_v4(), 250, "a@b.co")
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::Conflict(_)));
    }

    #[tokio::test]
    async fn list_cashouts_returns_items() {
        let repo = Arc::new(MockRepo {
            fail: Mutex::new(true),
            request: Mutex::new(None),
            list: Mutex::new(vec![request(250, CashoutStatus::Requested)]),
        });
        let service = CashoutService::new(repo, enabled_config());
        let resp = service.list_cashouts(Uuid::new_v4(), 1, 10).await.unwrap();
        assert_eq!(resp.total, 1);
        assert_eq!(resp.items.len(), 1);
        assert_eq!(resp.items[0].status, "requested");
    }
}
