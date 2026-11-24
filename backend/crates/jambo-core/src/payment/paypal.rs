use serde::{Deserialize, Serialize};
use std::time::Duration;

use super::{
    CheckoutContext, CheckoutCreated, ConnectAccountParams, PaymentProviderKind,
    PaymentServiceTrait,
};

// TODO: rename it as PayPalService to avoid confusion with the trait PaymentServiceTrait???
#[derive(Debug, Clone)]
pub struct PaymentService {
    client: reqwest::Client,
    base_url: String,
    client_id: String,
    client_secret: String,
}

#[derive(Debug, Serialize)]
struct CreateOrderRequest {
    intent: String,
    purchase_units: Vec<PurchaseUnit>,
    application_context: ApplicationContext,
}

#[derive(Debug, Serialize)]
struct PurchaseUnit {
    amount: Amount,
    description: String,
}

#[derive(Debug, Serialize)]
struct Amount {
    currency_code: String,
    value: String,
}

#[derive(Debug, Serialize)]
struct AmountV1 {
    currency: String,
    value: String,
}

#[derive(Debug, Serialize)]
struct ApplicationContext {
    return_url: String,
    cancel_url: String,
}

#[derive(Debug, Deserialize)]
struct AccessTokenResponse {
    access_token: String,
}

#[derive(Debug, Deserialize)]
struct CreateOrderResponse {
    id: String,
    #[allow(dead_code)]
    status: String,
    links: Vec<Link>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct Link {
    href: String,
    rel: String,
    method: String,
}

#[derive(Debug, Serialize)]
pub struct CaptureResult {
    pub success: bool,
    pub order_id: String,
}

#[derive(Debug, Serialize)]
#[allow(dead_code)]
struct PayoutItem {
    recipient_type: String,
    amount: AmountV1,
    note: String,
    receiver: String,
}

#[derive(Debug, Serialize)]
#[allow(dead_code)]
struct PayoutSenderBatchHeader {
    sender_batch_id: String,
    email_subject: String,
    email_message: String,
}

#[derive(Debug, Serialize)]
#[allow(dead_code)]
struct CreatePayoutRequest {
    sender_batch_header: PayoutSenderBatchHeader,
    items: Vec<PayoutItem>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct CreatePayoutResponse {
    batch_header: PayoutBatchHeader,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct PayoutBatchHeader {
    payout_batch_id: String,
}

fn format_euro(amount_eur_cents: i32) -> String {
    format!("{:.2}", amount_eur_cents as f64 / 100.0)
}

impl PaymentService {
    pub fn new(
        client_id: String,
        client_secret: String,
        mode: String,
        sandbox_url: String,
        live_url: String,
    ) -> Self {
        let base_url = match mode.as_str() {
            "live" => live_url,
            _ => sandbox_url,
        };

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            base_url,
            client_id,
            client_secret,
        }
    }

    pub fn is_configured(&self) -> bool {
        !self.client_id.is_empty() && !self.client_secret.is_empty()
    }

    async fn get_access_token(&self) -> Result<String, String> {
        let response = self
            .client
            .post(format!("{}/v1/oauth2/token", self.base_url))
            .basic_auth(&self.client_id, Some(&self.client_secret))
            .form(&[("grant_type", "client_credentials")])
            .send()
            .await
            .map_err(|e| format!("PayPal auth request failed: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("PayPal auth failed ({}): {}", status, body));
        }

        let token_response: AccessTokenResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse PayPal auth response: {}", e))?;

        Ok(token_response.access_token)
    }

    pub async fn create_checkout(&self, ctx: &CheckoutContext) -> Result<CheckoutCreated, String> {
        let access_token = self.get_access_token().await?;

        let order_request = CreateOrderRequest {
            intent: "CAPTURE".to_string(),
            purchase_units: vec![PurchaseUnit {
                amount: Amount {
                    currency_code: ctx.currency.clone().to_uppercase(),
                    value: format_euro(ctx.amount_eur_cents),
                },
                description: ctx.description.clone(),
            }],
            application_context: ApplicationContext {
                return_url: ctx.return_url.clone(),
                cancel_url: ctx.cancel_url.clone(),
            },
        };

        let response = self
            .client
            .post(format!("{}/v2/checkout/orders", self.base_url))
            .bearer_auth(&access_token)
            .json(&order_request)
            .send()
            .await
            .map_err(|e| format!("PayPal create order request failed: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("PayPal create order failed ({}): {}", status, body));
        }

        let order: CreateOrderResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse PayPal create order response: {}", e))?;

        let approval_url = order
            .links
            .iter()
            .find(|link| link.rel == "approve")
            .map(|link| link.href.clone())
            .ok_or_else(|| "No approval URL in PayPal response".to_string())?;

        Ok(CheckoutCreated {
            order_id: order.id,
            redirect_url: approval_url,
        })
    }

    pub async fn capture_order(
        &self,
        order_id: &str,
        idempotency_key: Option<&str>,
    ) -> Result<CaptureResult, String> {
        let access_token = self.get_access_token().await?;

        let mut request = self
            .client
            .post(format!(
                "{}/v2/checkout/orders/{}/capture",
                self.base_url, order_id
            ))
            .bearer_auth(&access_token)
            .header("Content-Type", "application/json");
        if let Some(key) = idempotency_key {
            request = request.header("PayPal-Request-Id", key);
        }

        let response = request
            .send()
            .await
            .map_err(|e| format!("PayPal capture order request failed: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("PayPal capture failed ({}): {}", status, body));
        }

        Ok(CaptureResult {
            success: true,
            order_id: order_id.to_string(),
        })
    }

    /// Create a single PayPal Payout to a recipient email. `item_id` is used as
    /// the idempotent `sender_batch_id`, so retrying the same request does not
    /// create a duplicate payout.
    pub async fn create_payout(
        &self,
        paypal_email: &str,
        amount_eur_cents: i32,
        item_id: &str,
    ) -> Result<String, String> {
        let access_token = self.get_access_token().await?;

        let request = CreatePayoutRequest {
            sender_batch_header: PayoutSenderBatchHeader {
                sender_batch_id: item_id.to_string(),
                email_subject: "Your Jambo cashout".to_string(),
                email_message: "Your Jambo cashout has been paid.".to_string(),
            },
            items: vec![PayoutItem {
                recipient_type: "EMAIL".to_string(),
                amount: AmountV1 {
                    currency: "EUR".to_string(),
                    value: format_euro(amount_eur_cents),
                },
                note: "Jambo cashout".to_string(),
                receiver: paypal_email.to_string(),
            }],
        };

        let response = self
            .client
            .post(format!("{}/v1/payments/payouts", self.base_url))
            .bearer_auth(&access_token)
            .json(&request)
            .send()
            .await
            .map_err(|e| format!("PayPal create payout request failed: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!(
                "PayPal create payout failed ({}): {}",
                status, body
            ));
        }

        let payout: CreatePayoutResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse PayPal payout response: {}", e))?;

        Ok(payout.batch_header.payout_batch_id)
    }
}

#[async_trait::async_trait]
impl PaymentServiceTrait for PaymentService {
    fn provider(&self) -> PaymentProviderKind {
        PaymentProviderKind::Paypal
    }

    fn is_configured(&self) -> bool {
        PaymentService::is_configured(self)
    }

    async fn create_checkout(&self, ctx: &CheckoutContext) -> Result<CheckoutCreated, String> {
        PaymentService::create_checkout(self, ctx).await
    }

    async fn capture_order(
        &self,
        order_id: &str,
        idempotency_key: Option<&str>,
    ) -> Result<CaptureResult, String> {
        PaymentService::capture_order(self, order_id, idempotency_key).await
    }

    async fn create_payout(
        &self,
        paypal_email: &str,
        amount_eur_cents: i32,
        item_id: &str,
    ) -> Result<String, String> {
        PaymentService::create_payout(self, paypal_email, amount_eur_cents, item_id).await
    }

    async fn create_connected_account(
        &self,
        _params: &ConnectAccountParams,
    ) -> Result<String, String> {
        Err("not supported by paypal".to_string())
    }

    async fn retrieve_account(&self, _account_id: &str) -> Result<super::AccountDetails, String> {
        Err("not supported by paypal".to_string())
    }

    async fn create_account_link(
        &self,
        _account_id: &str,
        _return_url: &str,
        _refresh_url: &str,
    ) -> Result<String, String> {
        Err("not supported by paypal".to_string())
    }

    async fn create_transfer(
        &self,
        _account_id: &str,
        _amount_eur_cents: i32,
        _item_id: &str,
    ) -> Result<String, String> {
        Err("not supported by paypal".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_service(client_id: &str, secret: &str, mode: &str) -> PaymentService {
        PaymentService::new(
            client_id.to_string(),
            secret.to_string(),
            mode.to_string(),
            "https://sandbox.paypal.com".to_string(),
            "https://live.paypal.com".to_string(),
        )
    }

    #[test]
    fn test_payment_service_not_configured_with_empty_credentials() {
        let service = new_service("", "", "sandbox");
        assert!(!service.is_configured());
    }

    #[test]
    fn test_payment_service_configured_with_credentials() {
        let service = new_service("client_id", "secret", "sandbox");
        assert!(service.is_configured());
    }

    #[test]
    fn test_sandbox_url() {
        let service = new_service("id", "secret", "sandbox");
        assert_eq!(service.base_url, "https://sandbox.paypal.com");
    }

    #[test]
    fn test_live_url() {
        let service = new_service("id", "secret", "live");
        assert_eq!(service.base_url, "https://live.paypal.com");
    }

    #[test]
    fn test_format_euro() {
        assert_eq!(format_euro(125), "1.25");
        assert_eq!(format_euro(500), "5.00");
        assert_eq!(format_euro(1000), "10.00");
    }
}
