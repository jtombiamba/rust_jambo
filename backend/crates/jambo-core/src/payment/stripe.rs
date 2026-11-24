use hmac::{Hmac, Mac};
use serde::Deserialize;
use sha2::Sha256;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::{
    CheckoutContext, CheckoutCreated, ConnectAccountParams, PaymentProviderKind,
    PaymentServiceTrait,
};

const STRIPE_API_BASE: &str = "https://api.stripe.com";
/// Stripe's recommended tolerance for webhook timestamps, in seconds.
const SIGNATURE_TOLERANCE_SECS: u64 = 300;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone)]
pub struct StripeService {
    client: reqwest::Client,
    secret_key: String,
    connect_client_id: String,
}

#[derive(Debug, Deserialize)]
struct CheckoutSessionResponse {
    id: String,
    url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AccountResponse {
    id: String,
}

#[derive(Debug, Deserialize)]
struct AccountLinkResponse {
    url: String,
}

#[derive(Debug, Deserialize)]
struct TransferResponse {
    id: String,
}

/// The subset of a Stripe Connect Account that drives our onboarding/verification
/// status machine. Missing fields default to empty/false so a partial payload is
/// still parsed safely.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AccountDetails {
    pub details_submitted: bool,
    pub charges_enabled: bool,
    pub payouts_enabled: bool,
    pub currently_due: Vec<String>,
    pub eventually_due: Vec<String>,
    pub past_due: Vec<String>,
    pub errors: Vec<serde_json::Value>,
    pub disabled_reason: Option<String>,
}

fn str_list(value: &serde_json::Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

/// Parse a Stripe Account JSON object into the minimal [`AccountDetails`] used by
/// the status evaluation. Pure so it is unit-testable without network access.
pub fn parse_account_details(account: &serde_json::Value) -> AccountDetails {
    let requirements = account.get("requirements");
    AccountDetails {
        details_submitted: account
            .get("details_submitted")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        charges_enabled: account
            .get("charges_enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        payouts_enabled: account
            .get("payouts_enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        currently_due: requirements
            .map(|r| str_list(r, "currently_due"))
            .unwrap_or_default(),
        eventually_due: requirements
            .map(|r| str_list(r, "eventually_due"))
            .unwrap_or_default(),
        past_due: requirements
            .map(|r| str_list(r, "past_due"))
            .unwrap_or_default(),
        errors: requirements
            .and_then(|r| r.get("errors"))
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default(),
        disabled_reason: requirements
            .and_then(|r| r.get("disabled_reason"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
    }
}

/// Build the Stripe account-creation form. Extracted as a pure function so the
/// required fields (notably the `transfers` capability) can be unit-tested.
pub fn connect_account_form(params: &ConnectAccountParams) -> Vec<(String, String)> {
    vec![
        ("type".to_string(), "express".to_string()),
        ("email".to_string(), params.email.clone()),
        ("country".to_string(), params.country.clone()),
        ("business_type".to_string(), "individual".to_string()),
        ("default_currency".to_string(), "eur".to_string()),
        (
            "capabilities[transfers][requested]".to_string(),
            "true".to_string(),
        ),
        ("metadata[reference]".to_string(), params.reference.clone()),
    ]
}

impl StripeService {
    pub fn new(secret_key: String, connect_client_id: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            secret_key,
            connect_client_id,
        }
    }

    pub fn is_configured(&self) -> bool {
        !self.secret_key.is_empty()
    }

    pub async fn create_checkout(&self, ctx: &CheckoutContext) -> Result<CheckoutCreated, String> {
        let mut form: Vec<(String, String)> = vec![
            ("mode".to_string(), "payment".to_string()),
            (
                "line_items[0][price_data][currency]".to_string(),
                ctx.currency.to_lowercase(),
            ),
            (
                "line_items[0][price_data][unit_amount]".to_string(),
                ctx.amount_eur_cents.to_string(),
            ),
            (
                "line_items[0][price_data][product_data][name]".to_string(),
                ctx.description.clone(),
            ),
            ("line_items[0][quantity]".to_string(), "1".to_string()),
            ("success_url".to_string(), ctx.return_url.clone()),
            ("cancel_url".to_string(), ctx.cancel_url.clone()),
        ];
        if let Some(reference) = &ctx.client_reference_id {
            form.push(("client_reference_id".to_string(), reference.clone()));
        }
        for (key, value) in &ctx.metadata {
            form.push((format!("metadata[{key}]"), value.clone()));
        }

        let response = self
            .client
            .post(format!("{STRIPE_API_BASE}/v1/checkout/sessions"))
            .bearer_auth(&self.secret_key)
            .form(&form)
            .send()
            .await
            .map_err(|e| format!("Stripe create checkout session failed: {e}"))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("Stripe create checkout failed ({status}): {body}"));
        }

        let session: CheckoutSessionResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse Stripe checkout response: {e}"))?;

        let redirect_url = session
            .url
            .ok_or_else(|| "Stripe checkout session missing URL".to_string())?;

        Ok(CheckoutCreated {
            order_id: session.id,
            redirect_url,
        })
    }

    pub async fn create_connected_account(
        &self,
        params: &ConnectAccountParams,
    ) -> Result<String, String> {
        let form = connect_account_form(params);
        let response = self
            .client
            .post(format!("{STRIPE_API_BASE}/v1/accounts"))
            .bearer_auth(&self.secret_key)
            .form(&form)
            .send()
            .await
            .map_err(|e| format!("Stripe create account failed: {e}"))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("Stripe create account failed ({status}): {body}"));
        }

        let account: AccountResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse Stripe account response: {e}"))?;

        Ok(account.id)
    }

    pub async fn create_account_link(
        &self,
        account_id: &str,
        return_url: &str,
        refresh_url: &str,
    ) -> Result<String, String> {
        let response = self
            .client
            .post(format!("{STRIPE_API_BASE}/v1/account_links"))
            .bearer_auth(&self.secret_key)
            .form(&[
                ("account", account_id),
                ("refresh_url", refresh_url),
                ("return_url", return_url),
                ("type", "account_onboarding"),
            ])
            .send()
            .await
            .map_err(|e| format!("Stripe create account link failed: {e}"))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!(
                "Stripe create account link failed ({status}): {body}"
            ));
        }

        let link: AccountLinkResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse Stripe account link response: {e}"))?;

        Ok(link.url)
    }

    pub async fn create_transfer(
        &self,
        account_id: &str,
        amount_eur_cents: i32,
        item_id: &str,
    ) -> Result<String, String> {
        let form: Vec<(String, String)> = vec![
            ("amount".to_string(), amount_eur_cents.to_string()),
            ("currency".to_string(), "eur".to_string()),
            ("destination".to_string(), account_id.to_string()),
            ("metadata[item_id]".to_string(), item_id.to_string()),
        ];

        let response = self
            .client
            .post(format!("{STRIPE_API_BASE}/v1/transfers"))
            .bearer_auth(&self.secret_key)
            .header("Idempotency-Key", item_id)
            .form(&form)
            .send()
            .await
            .map_err(|e| format!("Stripe create transfer failed: {e}"))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("Stripe create transfer failed ({status}): {body}"));
        }

        let transfer: TransferResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse Stripe transfer response: {e}"))?;

        Ok(transfer.id)
    }

    /// Accessor for the connected platform client id (used to build the OAuth /
    /// account-link return URLs and identify the platform account).
    pub fn connect_client_id(&self) -> &str {
        &self.connect_client_id
    }

    /// Fetch the current state of a Connect account so the onboarding/verification
    /// status can be re-evaluated from an `account.updated` webhook.
    pub async fn retrieve_account(&self, account_id: &str) -> Result<AccountDetails, String> {
        let response = self
            .client
            .get(format!("{STRIPE_API_BASE}/v1/accounts/{account_id}"))
            .bearer_auth(&self.secret_key)
            .send()
            .await
            .map_err(|e| format!("Stripe retrieve account failed: {e}"))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("Stripe retrieve account failed ({status}): {body}"));
        }

        let account: serde_json::Value = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse Stripe account response: {e}"))?;

        Ok(parse_account_details(&account))
    }
}

#[async_trait::async_trait]
impl PaymentServiceTrait for StripeService {
    fn provider(&self) -> PaymentProviderKind {
        PaymentProviderKind::Stripe
    }

    fn is_configured(&self) -> bool {
        StripeService::is_configured(self)
    }

    async fn create_checkout(&self, ctx: &CheckoutContext) -> Result<CheckoutCreated, String> {
        StripeService::create_checkout(self, ctx).await
    }

    async fn capture_order(
        &self,
        _order_id: &str,
        _idempotency_key: Option<&str>,
    ) -> Result<super::CaptureResult, String> {
        Err("not supported by stripe".to_string())
    }

    async fn create_payout(
        &self,
        _destination: &str,
        _amount_eur_cents: i32,
        _item_id: &str,
    ) -> Result<String, String> {
        Err("not supported by stripe; use create_transfer".to_string())
    }

    async fn create_connected_account(
        &self,
        params: &ConnectAccountParams,
    ) -> Result<String, String> {
        StripeService::create_connected_account(self, params).await
    }

    async fn retrieve_account(&self, account_id: &str) -> Result<AccountDetails, String> {
        StripeService::retrieve_account(self, account_id).await
    }

    async fn create_account_link(
        &self,
        account_id: &str,
        return_url: &str,
        refresh_url: &str,
    ) -> Result<String, String> {
        StripeService::create_account_link(self, account_id, return_url, refresh_url).await
    }

    async fn create_transfer(
        &self,
        account_id: &str,
        amount_eur_cents: i32,
        item_id: &str,
    ) -> Result<String, String> {
        StripeService::create_transfer(self, account_id, amount_eur_cents, item_id).await
    }
}

/// Verify a Stripe webhook signature header against the raw request payload.
///
/// Stripe signs the concatenation of the timestamp and payload
/// (`"{t}.{payload}"`) with HMAC-SHA256 using the webhook signing secret. The
/// header carries one or more schemes as `t=<unix>,v1=<hex>,...`; we validate
/// `v1` and reject stale timestamps outside the tolerance window.
pub fn verify_stripe_signature(
    payload: &[u8],
    signature_header: &str,
    secret: &str,
) -> Result<(), String> {
    if secret.is_empty() {
        return Err("webhook secret not configured".to_string());
    }

    let mut timestamp: Option<u64> = None;
    let mut v1_signature: Option<&str> = None;

    for part in signature_header.split(',') {
        let (key, value) = part
            .split_once('=')
            .ok_or_else(|| "invalid signature header".to_string())?;
        match key {
            "t" => timestamp = value.parse::<u64>().ok(),
            "v1" => v1_signature = Some(value),
            _ => {}
        }
    }

    let timestamp = timestamp.ok_or_else(|| "signature header missing timestamp".to_string())?;
    let expected =
        v1_signature.ok_or_else(|| "signature header missing v1 signature".to_string())?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs();
    if now.abs_diff(timestamp) > SIGNATURE_TOLERANCE_SECS {
        return Err("signature timestamp outside tolerance".to_string());
    }

    let signed_payload = format!("{}.{}", timestamp, String::from_utf8_lossy(payload));

    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|_| "invalid webhook secret".to_string())?;
    mac.update(signed_payload.as_bytes());
    let computed = hex::encode(mac.finalize().into_bytes());

    // Constant-time comparison to avoid leaking byte matches.
    let expected_bytes = expected.as_bytes();
    let computed_bytes = computed.as_bytes();
    if expected_bytes.len() != computed_bytes.len() {
        return Err("signature mismatch".to_string());
    }
    let mut diff = 0u8;
    for (a, b) in expected_bytes.iter().zip(computed_bytes.iter()) {
        diff |= a ^ b;
    }
    if diff != 0 {
        return Err("signature mismatch".to_string());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compute_signature(secret: &str, timestamp: u64, payload: &str) -> String {
        let signed = format!("{}.{}", timestamp, payload);
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(signed.as_bytes());
        hex::encode(mac.finalize().into_bytes())
    }

    #[test]
    fn test_signature_verifies_valid_payload() {
        let secret = "whsec_test";
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let payload = r#"{"type":"checkout.session.completed"}"#;
        let sig = compute_signature(secret, now, payload);
        let header = format!("t={now},v1={sig}");

        assert!(verify_stripe_signature(payload.as_bytes(), &header, secret).is_ok());
    }

    #[test]
    fn test_signature_rejects_tampered_payload() {
        let secret = "whsec_test";
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let sig = compute_signature(secret, now, r#"{"type":"original"}"#);
        let header = format!("t={now},v1={sig}");

        assert!(verify_stripe_signature(br#"{"type":"tampered"}"#, &header, secret).is_err());
    }

    #[test]
    fn test_signature_rejects_wrong_secret() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let payload = r#"{"type":"checkout.session.completed"}"#;
        let sig = compute_signature("whsec_real", now, payload);
        let header = format!("t={now},v1={sig}");

        assert!(verify_stripe_signature(payload.as_bytes(), &header, "whsec_other").is_err());
    }

    #[test]
    fn test_signature_rejects_stale_timestamp() {
        let secret = "whsec_test";
        let stale = 1_000_000u64;
        let payload = r#"{"type":"checkout.session.completed"}"#;
        let sig = compute_signature(secret, stale, payload);
        let header = format!("t={stale},v1={sig}");

        assert!(verify_stripe_signature(payload.as_bytes(), &header, secret).is_err());
    }

    #[test]
    fn test_signature_requires_secret() {
        assert!(verify_stripe_signature(b"{}", "t=1,v1=aa", "").is_err());
    }

    #[test]
    fn test_connect_account_form_requests_transfers_capability() {
        let params = ConnectAccountParams {
            email: "patient@example.com".into(),
            country: "FR".into(),
            reference: "user-123".into(),
        };
        let form = connect_account_form(&params);

        let get = |key: &str| form.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str());

        assert_eq!(get("type"), Some("express"));
        assert_eq!(get("email"), Some("patient@example.com"));
        assert_eq!(get("country"), Some("FR"));
        assert_eq!(get("business_type"), Some("individual"));
        assert_eq!(get("default_currency"), Some("eur"));
        assert_eq!(get("capabilities[transfers][requested]"), Some("true"));
        assert_eq!(get("metadata[reference]"), Some("user-123"));
    }

    #[test]
    fn test_parse_account_details_verified() {
        let account = serde_json::json!({
            "details_submitted": true,
            "charges_enabled": true,
            "payouts_enabled": true,
            "requirements": {
                "currently_due": [],
                "eventually_due": [],
                "past_due": [],
                "errors": [],
                "disabled_reason": null
            }
        });
        let details = parse_account_details(&account);
        assert!(details.details_submitted);
        assert!(details.charges_enabled);
        assert!(details.payouts_enabled);
        assert!(details.currently_due.is_empty());
        assert!(details.disabled_reason.is_none());
    }

    #[test]
    fn test_parse_account_details_missing_requirements_defaults() {
        let account = serde_json::json!({ "id": "acct_1" });
        let details = parse_account_details(&account);
        assert!(!details.details_submitted);
        assert!(!details.charges_enabled);
        assert!(!details.payouts_enabled);
        assert!(details.currently_due.is_empty());
        assert!(details.errors.is_empty());
        assert!(details.disabled_reason.is_none());
    }
}
