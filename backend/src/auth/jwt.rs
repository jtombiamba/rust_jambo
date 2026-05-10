use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::config::AuthConfig;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: Uuid,
    pub pseudo: String,
    pub exp: usize,
    pub iat: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ResetClaims {
    pub email: String,
    pub exp: usize,
    pub iat: usize,
}

pub fn generate_token(
    user_id: Uuid,
    pseudo: &str,
    config: &AuthConfig,
) -> Result<String, jsonwebtoken::errors::Error> {
    let now = chrono::Utc::now();
    let claims = Claims {
        sub: user_id,
        pseudo: pseudo.to_string(),
        exp: (now + chrono::Duration::hours(config.jwt_expiry_hours)).timestamp() as usize,
        iat: now.timestamp() as usize,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(config.jwt_secret.as_bytes()),
    )
}

pub fn validate_token(
    token: &str,
    config: &AuthConfig,
) -> Result<Claims, jsonwebtoken::errors::Error> {
    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(config.jwt_secret.as_bytes()),
        &Validation::default(),
    )?;
    Ok(token_data.claims)
}

pub fn generate_reset_token(
    email: &str,
    config: &AuthConfig,
) -> Result<String, jsonwebtoken::errors::Error> {
    let now = chrono::Utc::now();
    let claims = ResetClaims {
        email: email.to_string(),
        exp: (now + chrono::Duration::minutes(30)).timestamp() as usize,
        iat: now.timestamp() as usize,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(config.jwt_secret.as_bytes()),
    )
}

pub fn validate_reset_token(
    token: &str,
    config: &AuthConfig,
) -> Result<ResetClaims, jsonwebtoken::errors::Error> {
    let token_data = decode::<ResetClaims>(
        token,
        &DecodingKey::from_secret(config.jwt_secret.as_bytes()),
        &Validation::default(),
    )?;
    Ok(token_data.claims)
}
