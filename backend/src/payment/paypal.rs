use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct PaymentService {
    client: reqwest::Client,
    base_url: String,
    client_id: String,
    client_secret: String,
    unfreeze_amount_eur: String,
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
pub struct OrderCreated {
    pub order_id: String,
    pub approval_url: String,
}

#[derive(Debug, Serialize)]
pub struct CaptureResult {
    pub success: bool,
    pub order_id: String,
}

impl PaymentService {
    pub fn new(
        client_id: String,
        client_secret: String,
        mode: String,
        unfreeze_amount_eur: String,
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
            unfreeze_amount_eur,
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

    pub async fn create_order(
        &self,
        return_url: &str,
        cancel_url: &str,
    ) -> Result<OrderCreated, String> {
        let access_token = self.get_access_token().await?;

        let order_request = CreateOrderRequest {
            intent: "CAPTURE".to_string(),
            purchase_units: vec![PurchaseUnit {
                amount: Amount {
                    currency_code: "EUR".to_string(),
                    value: self.unfreeze_amount_eur.clone(),
                },
                description: "Account unfreeze - 1 hour access".to_string(),
            }],
            application_context: ApplicationContext {
                return_url: return_url.to_string(),
                cancel_url: cancel_url.to_string(),
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

        Ok(OrderCreated {
            order_id: order.id,
            approval_url,
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_payment_service_not_configured_with_empty_credentials() {
        let service = PaymentService::new(
            "".to_string(),
            "".to_string(),
            "sandbox".to_string(),
            "1.00".to_string(),
            "https://sandbox.paypal.com".to_string(),
            "https://live.paypal.com".to_string(),
        );
        assert!(!service.is_configured());
    }

    #[test]
    fn test_payment_service_configured_with_credentials() {
        let service = PaymentService::new(
            "client_id".to_string(),
            "secret".to_string(),
            "sandbox".to_string(),
            "1.00".to_string(),
            "https://sandbox.paypal.com".to_string(),
            "https://live.paypal.com".to_string(),
        );
        assert!(service.is_configured());
    }

    #[test]
    fn test_sandbox_url() {
        let service = PaymentService::new(
            "id".to_string(),
            "secret".to_string(),
            "sandbox".to_string(),
            "1.00".to_string(),
            "https://sandbox.paypal.com".to_string(),
            "https://live.paypal.com".to_string(),
        );
        assert_eq!(service.base_url, "https://sandbox.paypal.com");
    }

    #[test]
    fn test_live_url() {
        let service = PaymentService::new(
            "id".to_string(),
            "secret".to_string(),
            "live".to_string(),
            "1.00".to_string(),
            "https://sandbox.paypal.com".to_string(),
            "https://live.paypal.com".to_string(),
        );
        assert_eq!(service.base_url, "https://live.paypal.com");
    }
}
