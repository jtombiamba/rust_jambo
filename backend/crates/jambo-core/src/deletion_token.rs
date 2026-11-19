//! Signed JWT tokens for the account-deactivation confirmation flow.
//!
//! A DPO mints one of these for a specific user from `admin-cli`, the token is
//! embedded in the emailed confirmation link, and the public API endpoint
//! validates it before recording the deletion request. Using a signed JWT (the
//! same `JWT_SECRET` as every other token) keeps the secret out of the email
//! and out of the CLI's storage.

use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// How long a deactivation confirmation link stays valid.
const DELETION_TOKEN_TTL_SECS: u64 = 72 * 60 * 60;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DeletionClaims {
    /// The user the token authorizes to be deactivated.
    pub sub: Uuid,
    pub exp: usize,
    pub iat: usize,
}

fn now_secs() -> usize {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as usize)
        .unwrap_or(0)
}

pub fn generate_deletion_token(
    user_id: Uuid,
    secret: &str,
) -> Result<String, jsonwebtoken::errors::Error> {
    let now = now_secs();
    let claims = DeletionClaims {
        sub: user_id,
        iat: now,
        exp: now + DELETION_TOKEN_TTL_SECS as usize,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
}

pub fn validate_deletion_token(
    token: &str,
    secret: &str,
) -> Result<DeletionClaims, jsonwebtoken::errors::Error> {
    let token_data = decode::<DeletionClaims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )?;
    Ok(token_data.claims)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_round_trips_to_same_user() {
        let id = Uuid::now_v7();
        let token = generate_deletion_token(id, "secret").unwrap();
        let claims = validate_deletion_token(&token, "secret").unwrap();
        assert_eq!(claims.sub, id);
        assert!(claims.exp > claims.iat);
    }

    #[test]
    fn token_rejects_wrong_secret() {
        let id = Uuid::now_v7();
        let token = generate_deletion_token(id, "secret").unwrap();
        assert!(validate_deletion_token(&token, "other").is_err());
    }
}
