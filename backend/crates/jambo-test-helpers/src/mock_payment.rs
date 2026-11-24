use std::sync::Mutex;

use async_trait::async_trait;
use jambo_core::payment::{
    AccountDetails, CaptureResult, CheckoutContext, CheckoutCreated, ConnectAccountParams,
    PaymentProviderKind, PaymentServiceTrait,
};

/// Configurable `PaymentServiceTrait` mock used across API handler tests.
///
/// Defaults to the PayPal provider (to match the legacy capture flow) and a
/// successful `create_checkout` result; tests override per-method results or
/// switch the provider with [`MockPaymentService::set_provider`].
pub struct MockPaymentService {
    configured: bool,
    provider: Mutex<PaymentProviderKind>,
    create_checkout_result: Mutex<Option<Result<CheckoutCreated, String>>>,
    capture_order_result: Mutex<Option<Result<CaptureResult, String>>>,
    create_payout_result: Mutex<Option<Result<String, String>>>,
    create_connected_account_result: Mutex<Option<Result<String, String>>>,
    create_account_link_result: Mutex<Option<Result<String, String>>>,
    create_transfer_result: Mutex<Option<Result<String, String>>>,
    retrieve_account_result: Mutex<Option<Result<AccountDetails, String>>>,
}

impl MockPaymentService {
    pub fn configured() -> Self {
        Self {
            configured: true,
            provider: Mutex::new(PaymentProviderKind::Paypal),
            create_checkout_result: Mutex::new(None),
            capture_order_result: Mutex::new(None),
            create_payout_result: Mutex::new(None),
            create_connected_account_result: Mutex::new(None),
            create_account_link_result: Mutex::new(None),
            create_transfer_result: Mutex::new(None),
            retrieve_account_result: Mutex::new(None),
        }
    }

    pub fn unconfigured() -> Self {
        Self {
            configured: false,
            ..Self::configured()
        }
    }

    pub fn set_provider(&self, p: PaymentProviderKind) {
        *self.provider.lock().unwrap() = p;
    }

    pub fn set_capture_order_result(&self, r: Result<CaptureResult, String>) {
        *self.capture_order_result.lock().unwrap() = Some(r);
    }

    pub fn set_create_checkout_result(&self, r: Result<CheckoutCreated, String>) {
        *self.create_checkout_result.lock().unwrap() = Some(r);
    }

    pub fn set_create_payout_result(&self, r: Result<String, String>) {
        *self.create_payout_result.lock().unwrap() = Some(r);
    }

    pub fn set_create_transfer_result(&self, r: Result<String, String>) {
        *self.create_transfer_result.lock().unwrap() = Some(r);
    }

    pub fn set_create_connected_account_result(&self, r: Result<String, String>) {
        *self.create_connected_account_result.lock().unwrap() = Some(r);
    }

    pub fn set_create_account_link_result(&self, r: Result<String, String>) {
        *self.create_account_link_result.lock().unwrap() = Some(r);
    }

    pub fn set_retrieve_account_result(&self, r: Result<AccountDetails, String>) {
        *self.retrieve_account_result.lock().unwrap() = Some(r);
    }
}

fn default_checkout() -> CheckoutCreated {
    CheckoutCreated {
        order_id: "ORDER_1".into(),
        redirect_url: "https://checkout.stripe.com/approve".into(),
    }
}

#[async_trait]
impl PaymentServiceTrait for MockPaymentService {
    fn provider(&self) -> PaymentProviderKind {
        *self.provider.lock().unwrap()
    }

    fn is_configured(&self) -> bool {
        self.configured
    }

    async fn create_checkout(&self, _ctx: &CheckoutContext) -> Result<CheckoutCreated, String> {
        self.create_checkout_result
            .lock()
            .unwrap()
            .take()
            .unwrap_or(Ok(default_checkout()))
    }

    async fn capture_order(
        &self,
        _order_id: &str,
        _idempotency_key: Option<&str>,
    ) -> Result<CaptureResult, String> {
        self.capture_order_result
            .lock()
            .unwrap()
            .take()
            .unwrap_or(Ok(CaptureResult {
                success: true,
                order_id: "ORDER_1".into(),
            }))
    }

    async fn create_payout(
        &self,
        _destination: &str,
        _amount_eur_cents: i32,
        _item_id: &str,
    ) -> Result<String, String> {
        self.create_payout_result
            .lock()
            .unwrap()
            .take()
            .unwrap_or(Ok("batch-id".into()))
    }

    async fn create_connected_account(
        &self,
        _params: &ConnectAccountParams,
    ) -> Result<String, String> {
        self.create_connected_account_result
            .lock()
            .unwrap()
            .take()
            .unwrap_or(Ok("acct_mock".into()))
    }

    async fn retrieve_account(&self, _account_id: &str) -> Result<AccountDetails, String> {
        self.retrieve_account_result
            .lock()
            .unwrap()
            .take()
            .unwrap_or(Ok(AccountDetails {
                details_submitted: true,
                charges_enabled: true,
                payouts_enabled: true,
                ..Default::default()
            }))
    }

    async fn create_account_link(
        &self,
        _account_id: &str,
        _return_url: &str,
        _refresh_url: &str,
    ) -> Result<String, String> {
        self.create_account_link_result
            .lock()
            .unwrap()
            .take()
            .unwrap_or(Ok("https://connect.stripe.com/onboard".into()))
    }

    async fn create_transfer(
        &self,
        _account_id: &str,
        _amount_eur_cents: i32,
        _item_id: &str,
    ) -> Result<String, String> {
        self.create_transfer_result
            .lock()
            .unwrap()
            .take()
            .unwrap_or(Ok("tr_mock".into()))
    }
}
