mod paypal;
pub mod stripe;

use std::collections::HashMap;
use std::sync::Arc;

pub use paypal::{CaptureResult, PaymentService};
pub use stripe::{verify_stripe_signature, AccountDetails, StripeService};

use crate::config::Config;

/// Identifies which concrete payment provider backs the trait seam. Handlers use
/// this to branch only on surface differences (e.g. redirect vs webhook flow).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaymentProviderKind {
    Paypal,
    Stripe,
}

/// Provider-agnostic result of creating a payment/checkout session.
#[derive(Debug, Clone)]
pub struct CheckoutCreated {
    pub order_id: String,
    pub redirect_url: String,
}

/// Everything Stripe needs to create a Connect Express account for a user.
/// PayPal ignores these fields; only Stripe creates connected accounts.
#[derive(Debug, Clone)]
pub struct ConnectAccountParams {
    pub email: String,
    pub country: String,
    pub reference: String,
}

/// Everything a provider needs to create a checkout. PayPal ignores the Stripe
/// specific fields (`client_reference_id`, `metadata`); Stripe uses them to
/// carry the user id and the credit/amount resolution into the webhook so the
/// backend never needs a Redis lookup to finalize a payment.
#[derive(Debug, Clone)]
pub struct CheckoutContext {
    pub amount_eur_cents: i32,
    pub currency: String,
    pub description: String,
    pub return_url: String,
    pub cancel_url: String,
    pub client_reference_id: Option<String>,
    pub metadata: HashMap<String, String>,
}

impl CheckoutContext {
    pub fn new(
        amount_eur_cents: i32,
        description: impl Into<String>,
        return_url: impl Into<String>,
        cancel_url: impl Into<String>,
    ) -> Self {
        Self {
            amount_eur_cents,
            currency: "eur".to_string(),
            description: description.into(),
            return_url: return_url.into(),
            cancel_url: cancel_url.into(),
            client_reference_id: None,
            metadata: HashMap::new(),
        }
    }

    pub fn with_client_reference_id(mut self, id: impl Into<String>) -> Self {
        self.client_reference_id = Some(id.into());
        self
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}

/// Trait seam for payment providers, enabling handler-level testing. Every
/// provider implements the full surface; methods a provider does not support
/// return `Err("not supported by <provider>")`.
#[async_trait::async_trait]
pub trait PaymentServiceTrait: Send + Sync {
    fn provider(&self) -> PaymentProviderKind;

    fn is_configured(&self) -> bool;

    async fn create_checkout(&self, ctx: &CheckoutContext) -> Result<CheckoutCreated, String>;

    /// Authorize-and-capture for redirect-based providers (PayPal). Stripe does
    /// not support a server-side capture; it finalizes via webhook instead.
    async fn capture_order(
        &self,
        order_id: &str,
        idempotency_key: Option<&str>,
    ) -> Result<CaptureResult, String>;

    /// Pay out to a provider-held recipient address (PayPal email).
    async fn create_payout(
        &self,
        destination: &str,
        amount_eur_cents: i32,
        item_id: &str,
    ) -> Result<String, String>;

    /// Create a Stripe Connect account for a user. Returns the opaque account id.
    async fn create_connected_account(
        &self,
        params: &ConnectAccountParams,
    ) -> Result<String, String>;

    /// Fetch the current state of a Stripe Connect account (used to evaluate the
    /// onboarding/verification status on `account.updated` webhooks).
    async fn retrieve_account(&self, account_id: &str) -> Result<AccountDetails, String>;

    /// Create a Stripe-hosted onboarding link for an existing connected account.
    async fn create_account_link(
        &self,
        account_id: &str,
        return_url: &str,
        refresh_url: &str,
    ) -> Result<String, String>;

    /// Transfer funds to a Stripe Connect account.
    async fn create_transfer(
        &self,
        account_id: &str,
        amount_eur_cents: i32,
        item_id: &str,
    ) -> Result<String, String>;
}

/// Build the configured payment provider behind the trait seam.
pub fn payment_service_from_config(config: &Config) -> Arc<dyn PaymentServiceTrait> {
    tracing::info!("Configured payment provider: {}", config.payment_provider);
    if config.payment_provider.eq_ignore_ascii_case("stripe") {
        tracing::info!("Using Stripe as payment provider");
        Arc::new(StripeService::new(
            config.stripe_secret_key.clone(),
            config.stripe_connect_client_id.clone(),
            config.stripe_version.clone(),
        ))
    } else {
        tracing::info!("Using PayPal as payment provider");
        Arc::new(PaymentService::new(
            config.paypal_client_id.clone(),
            config.paypal_client_secret.clone(),
            config.paypal_mode.clone(),
            config.paypal_sandbox_url.clone(),
            config.paypal_live_url.clone(),
        ))
    }
}
