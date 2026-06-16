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
    pub jti: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ResetClaims {
    pub email: String,
    pub exp: usize,
    pub iat: usize,
}

/// Claims for a one-time game token used by anonymous users
/// to authenticate their WebSocket connection to a specific game.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GameTokenClaims {
    /// The game_id this token grants access to
    pub sub: Uuid,
    /// Always "ws:game" to distinguish from regular auth tokens
    pub purpose: String,
    pub exp: usize,
    pub iat: usize,
    /// Unique token ID for single-use enforcement via Redis
    pub jti: String,
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
        jti: Uuid::new_v4().to_string(),
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

/// Result type for game token generation: the encoded token string and its claims.
pub type GenerateGameTokenResult = (String, GameTokenClaims);

/// Generate a one-time game token for anonymous WebSocket authentication.
/// The token is signed with the same JWT secret and expires after `ttl_secs`.
/// Returns both the encoded token string and the claims (so the caller can
/// use the jti for the Redis key without re-decoding the token).
pub fn generate_game_token(
    game_id: Uuid,
    config: &AuthConfig,
    ttl_secs: u64,
) -> Result<GenerateGameTokenResult, jsonwebtoken::errors::Error> {
    let now = chrono::Utc::now();
    let claims = GameTokenClaims {
        sub: game_id,
        purpose: "ws:game".to_string(),
        exp: (now + chrono::Duration::seconds(ttl_secs as i64)).timestamp() as usize,
        iat: now.timestamp() as usize,
        jti: Uuid::new_v4().to_string(),
    };
    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(config.jwt_secret.as_bytes()),
    )?;
    Ok((token, claims))
}

/// Validate a game token and return its claims.
pub fn validate_game_token(
    token: &str,
    config: &AuthConfig,
) -> Result<GameTokenClaims, jsonwebtoken::errors::Error> {
    let validation = Validation::default();
    let token_data = decode::<GameTokenClaims>(
        token,
        &DecodingKey::from_secret(config.jwt_secret.as_bytes()),
        &validation,
    )?;
    // Verify purpose field
    if token_data.claims.purpose != "ws:game" {
        return Err(jsonwebtoken::errors::Error::from(
            jsonwebtoken::errors::ErrorKind::InvalidSubject,
        ));
    }
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
