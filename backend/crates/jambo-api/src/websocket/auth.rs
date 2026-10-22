use actix_web::{web, HttpRequest};
use moka::sync::Cache;
use std::sync::LazyLock;
use std::time::Duration;
use uuid::Uuid;

use crate::auth::config::AuthConfig;
use crate::auth::jwt::{self, GameTokenClaims};
use jambo_domain::messaging::RedisClient;

/// Namespace constant used to derive a deterministic anonymous identity from a
/// game id. Arbitrary but fixed to avoid collisions with real user IDs.
const ANON_NAMESPACE: u128 = 0x006A_6F6E_6573_5F61_6E6F_6E5F_7575_6964_u128;

/// Namespace constant used to derive a deterministic spectator identity from a
/// game id. Arbitrary but fixed to avoid collisions with real user IDs.
const SPECTATE_NAMESPACE: u128 = 0x0000_5350_4543_5441_5445_5F41_4E4F_4E5F_4944_u128;

/// TTL for cached token-existence results. Tokens are *not* single-use-deleted
/// (they persist for their full TTL and are revocable by deletion), so caching a
/// positive result only delays revocation by this window. Keeping it short bounds
/// that delay while still collapsing the load test's burst of 100 identical
/// `EXISTS` calls per game into a single Redis round-trip.
const TOKEN_CACHE_TTL_SECS: u64 = 2;

/// Process-wide cache of token-key existence checks, keyed by the full Redis key
/// (`ws_token:{game_id}:{jti}` / `spectate_token:{game_id}:{jti}`). JWT signature,
/// expiry, and purpose are still verified on every request; only the Redis
/// `EXISTS` lookup is memoized.
fn token_cache() -> &'static Cache<String, bool> {
    static CACHE: LazyLock<Cache<String, bool>> = LazyLock::new(|| {
        Cache::builder()
            .time_to_live(Duration::from_secs(TOKEN_CACHE_TTL_SECS))
            .max_capacity(100_000)
            .build()
    });
    &CACHE
}

/// Derive a deterministic anonymous user id for a game, so unauthenticated
/// players always map to the same identity within a session.
pub(crate) fn derive_anonymous_id(game_id: Uuid) -> Uuid {
    Uuid::from_u128(game_id.as_u128() ^ ANON_NAMESPACE)
}

/// Derive a deterministic spectator id for a game.
pub(crate) fn derive_spectate_id(game_id: Uuid) -> Uuid {
    Uuid::from_u128(game_id.as_u128() ^ SPECTATE_NAMESPACE)
}

/// Result of authenticating a game WebSocket connection attempt.
pub(super) struct AuthContext {
    pub(super) user_id: Option<Uuid>,
    /// Whether the identity came from the `Authorization` cookie. Cookie-auth
    /// connections must pass the participant check; anonymous game-token and
    /// spectator connections are exempt.
    pub(super) is_cookie_auth: bool,
}

/// Authenticate a game WebSocket connection: try the auth cookie first, then
/// fall back to a one-time game token or spectator token in the query string.
///
/// The query-string token is decoded exactly once and its `purpose` selects the
/// validator, avoiding a wasted HMAC verification when a spectator token is first
/// rejected as a game token.
pub(super) async fn authenticate_ws_request(
    req: &HttpRequest,
    game_id: Uuid,
    auth_config: Option<web::Data<AuthConfig>>,
    redis_client: Option<RedisClient>,
) -> AuthContext {
    let token = req.cookie("Authorization").map(|c| c.value().to_string());

    if let Some(token) = token {
        tracing::debug!("WebSocket connection via token cookie for game {}", game_id);
        return AuthContext {
            user_id: validate_ws_token(Some(token), auth_config, redis_client).await,
            is_cookie_auth: true,
        };
    }

    // Check for one-time game token in query parameter
    let game_token = req
        .query_string()
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(k, _)| *k == "token")
        .map(|(_, v)| v.to_string());

    let user_id = if let Some(gt) = game_token {
        let Some(config) = auth_config else {
            return AuthContext {
                user_id: None,
                is_cookie_auth: false,
            };
        };
        match jwt::decode_game_token(&gt, &config) {
            Ok(claims) if claims.purpose == "ws:game" => {
                validate_game_token_with_claims(&claims, game_id, redis_client).await
            }
            Ok(claims) if claims.purpose == "ws:spectate" => {
                validate_spectate_token_with_claims(&claims, game_id, redis_client).await
            }
            _ => None,
        }
    } else {
        tracing::debug!(
            "WebSocket connection via token: no query params for game {}",
            game_id
        );
        None
    };

    AuthContext {
        user_id,
        is_cookie_auth: false,
    }
}

pub async fn validate_ws_token(
    token: Option<String>,
    auth_config: Option<web::Data<AuthConfig>>,
    mut redis_client: Option<RedisClient>,
) -> Option<Uuid> {
    let config = auth_config?;
    let t = token?;
    let claims = jwt::validate_token(&t, &config).ok()?;

    if let Some(ref mut r) = redis_client {
        match r.exists(&format!("token:blacklist:{}", claims.jti)).await {
            Ok(true) => {
                return None;
            }
            Ok(false) => {}
            Err(e) => {
                // Fail-open: if Redis is unavailable, allow the connection.
                // The JWT signature and expiry have already been validated;
                // the Redis check is only for token revocation.
                tracing::warn!(
                    "Redis unavailable during blacklist check, allowing connection: {}",
                    e
                );
                jambo_core::observability::metrics::WS_AUTH_BLACKLIST_REDIS_ERRORS_TOTAL.inc();
            }
        }
    }

    Some(claims.sub)
}

/// Validate a one-time game token for unauthenticated WebSocket connections.
/// The token must:
/// 1. Be a valid JWT with purpose "ws:game" and matching game_id
/// 2. Exist in Redis (single-use enforcement) — consumed on first use
pub async fn validate_game_token(
    token: &str,
    game_id: Uuid,
    auth_config: Option<web::Data<AuthConfig>>,
    redis_client: Option<RedisClient>,
) -> Option<Uuid> {
    let config = auth_config?;
    let claims = jwt::validate_game_token(token, &config).ok()?;
    validate_game_token_with_claims(&claims, game_id, redis_client).await
}

/// Shared post-decode validation for game tokens: scope check + Redis existence.
async fn validate_game_token_with_claims(
    claims: &GameTokenClaims,
    game_id: Uuid,
    mut redis_client: Option<RedisClient>,
) -> Option<Uuid> {
    // Ensure the token is for this specific game
    if claims.sub != game_id {
        tracing::warn!(
            "Game token mismatch: token is for game {} but connection is for game {}",
            claims.sub,
            game_id
        );
        return None;
    }

    // Validate the token exists in Redis (inserted at generation time).
    // We don't delete it — it persists for its full TTL so anonymous users
    // can reconnect after transient disconnections. JWT signature, expiry,
    // and purpose checks already secure the token.
    let redis_key = format!("ws_token:{}:{}", game_id, claims.jti);
    if let Some(ref mut r) = redis_client {
        match cached_exists(r, &redis_key).await {
            Ok(true) => {
                tracing::debug!(
                    "Game token validated for game {}, jti: {}",
                    game_id,
                    claims.jti
                );
            }
            Ok(false) => {
                tracing::warn!(
                    "Game token not found in Redis (never issued or expired) for game {}",
                    game_id
                );
                return None;
            }
            Err(e) => {
                // Fail-open: if Redis is unavailable, allow the connection.
                // The JWT signature, expiry, and purpose have already been validated;
                // the Redis check is only for single-use token enforcement.
                tracing::error!("Redis error checking game token: {}", e);
                jambo_core::observability::metrics::WS_TOKEN_VALIDATION_REDIS_ERRORS_TOTAL.inc();
                tracing::warn!(
                    "Redis unavailable, allowing game token connection for game {}",
                    game_id
                );
            }
        }
    }

    // Use a deterministic UUID derived from the game_id for unauthenticated users.
    // This ensures the same "anonymous" user identity within a game session.
    Some(derive_anonymous_id(game_id))
}

/// Validate a read-only spectator token for the stream view WebSocket connection.
/// The token must:
/// 1. Be a valid JWT with purpose "ws:spectate" and matching game_id
/// 2. Exist in Redis (inserted at mint time, revocable by deletion)
pub async fn validate_spectate_token(
    token: &str,
    game_id: Uuid,
    auth_config: Option<web::Data<AuthConfig>>,
    redis_client: Option<RedisClient>,
) -> Option<Uuid> {
    let config = auth_config?;
    let claims = jwt::validate_spectate_token(token, &config).ok()?;
    validate_spectate_token_with_claims(&claims, game_id, redis_client).await
}

/// Shared post-decode validation for spectator tokens: scope check + Redis existence.
async fn validate_spectate_token_with_claims(
    claims: &GameTokenClaims,
    game_id: Uuid,
    mut redis_client: Option<RedisClient>,
) -> Option<Uuid> {
    if claims.sub != game_id {
        tracing::warn!(
            "Spectator token mismatch: token is for game {} but connection is for game {}",
            claims.sub,
            game_id
        );
        return None;
    }

    let redis_key = format!("spectate_token:{}:{}", game_id, claims.jti);
    if let Some(ref mut r) = redis_client {
        match cached_exists(r, &redis_key).await {
            Ok(true) => {
                tracing::debug!(
                    "Spectator token validated for game {}, jti: {}",
                    game_id,
                    claims.jti
                );
            }
            Ok(false) => {
                tracing::warn!(
                    "Spectator token not found in Redis (never issued or expired) for game {}",
                    game_id
                );
                return None;
            }
            Err(e) => {
                tracing::error!("Redis error checking spectator token: {}", e);
                jambo_core::observability::metrics::WS_TOKEN_VALIDATION_REDIS_ERRORS_TOTAL.inc();
                tracing::warn!(
                    "Redis unavailable, allowing spectator token connection for game {}",
                    game_id
                );
            }
        }
    }

    Some(derive_spectate_id(game_id))
}

/// Check whether a token's Redis key exists, memoized by an in-process TTL cache.
async fn cached_exists(redis_client: &mut RedisClient, key: &str) -> redis::RedisResult<bool> {
    if let Some(cached) = token_cache().get(key) {
        return Ok(cached);
    }
    let exists = redis_client.exists(key).await?;
    token_cache().insert(key.to_string(), exists);
    Ok(exists)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_anonymous_id_is_deterministic() {
        let game_id = Uuid::new_v4();
        assert_eq!(derive_anonymous_id(game_id), derive_anonymous_id(game_id));
    }

    #[test]
    fn derive_anonymous_id_differs_from_spectate_id() {
        let game_id = Uuid::new_v4();
        assert_ne!(derive_anonymous_id(game_id), derive_spectate_id(game_id));
    }

    #[test]
    fn derive_spectate_id_is_deterministic() {
        let game_id = Uuid::new_v4();
        assert_eq!(derive_spectate_id(game_id), derive_spectate_id(game_id));
    }
}
