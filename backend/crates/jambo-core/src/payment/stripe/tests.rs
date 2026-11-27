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
fn test_connect_account_payload_requests_stripe_transfers() {
    let params = ConnectAccountParams {
        email: "patient@example.com".into(),
        country: "FR".into(),
        reference: "user-123".into(),
    };
    let payload = serde_json::to_value(connect_account_payload(&params)).unwrap();

    assert_eq!(payload["contact_email"], "patient@example.com");
    assert_eq!(payload["dashboard"], "express");
    assert_eq!(payload["identity"]["country"], "FR");
    assert_eq!(
        payload["defaults"]["responsibilities"]["fees_collector"],
        "application"
    );
    assert_eq!(
        payload["defaults"]["responsibilities"]["losses_collector"],
        "application"
    );
    assert_eq!(
        payload["configuration"]["recipient"]["capabilities"]["stripe_balance"]["stripe_transfers"]
            ["requested"],
        true
    );
    assert_eq!(payload["metadata"]["reference"], "user-123");
}

#[test]
fn test_parse_account_details_active() {
    let account = serde_json::json!({
        "configuration": {
            "recipient": {
                "capabilities": {
                    "stripe_balance": {
                        "stripe_transfers": { "status": "active" }
                    }
                }
            }
        },
        "requirements": {
            "entries": []
        }
    });
    let details = parse_account_details(&account);
    assert!(details.details_submitted);
    assert!(details.charges_enabled);
    assert!(details.payouts_enabled);
    assert!(details.currently_due.is_empty());
    assert!(details.eventually_due.is_empty());
    assert!(details.past_due.is_empty());
    assert!(details.errors.is_empty());
    assert!(details.disabled_reason.is_none());
}

#[test]
fn test_parse_account_details_pending_requirements() {
    let account = serde_json::json!({
        "configuration": {
            "recipient": {
                "capabilities": {
                    "stripe_balance": {
                        "stripe_transfers": { "status": "inactive" }
                    }
                }
            }
        },
        "requirements": {
            "entries": [
                {
                    "awaiting_action_from": "user",
                    "description": "individual.dob",
                    "errors": [{ "code": "invalid_dob_age_under_18" }],
                    "minimum_deadline": { "status": "currently_due" }
                },
                {
                    "awaiting_action_from": "user",
                    "description": "individual.email",
                    "errors": [],
                    "minimum_deadline": { "status": "eventually_due" }
                }
            ]
        }
    });
    let details = parse_account_details(&account);
    assert!(!details.details_submitted);
    assert!(details.charges_enabled);
    assert!(!details.payouts_enabled);
    assert_eq!(details.currently_due, vec!["individual.dob".to_string()]);
    assert_eq!(details.eventually_due, vec!["individual.email".to_string()]);
    assert!(details.past_due.is_empty());
    assert_eq!(details.errors.len(), 1);
}

#[test]
fn test_parse_account_details_empty_defaults() {
    let account = serde_json::json!({ "id": "acct_1" });
    let details = parse_account_details(&account);
    assert!(details.details_submitted);
    assert!(details.charges_enabled);
    assert!(!details.payouts_enabled);
    assert!(details.currently_due.is_empty());
    assert!(details.eventually_due.is_empty());
    assert!(details.past_due.is_empty());
    assert!(details.errors.is_empty());
    assert!(details.disabled_reason.is_none());
}
