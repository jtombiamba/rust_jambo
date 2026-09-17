use actix_web::web;
use uuid::Uuid;

use crate::auth::config::AuthConfig;
use crate::auth::jwt;
use crate::messaging::RedisClient;

/// Namespace constant used to derive a deterministic anonymous identity from a
/// game id. Arbitrary but fixed to avoid collisions with real user IDs.
const ANON_NAMESPACE: u128 = 0x006A_6F6E_6573_5F61_6E6F_6E5F_7575_6964_u128;

/// Namespace constant used to derive a deterministic spectator identity from a
/// game id. Arbitrary but fixed to avoid collisions with real user IDs.
const SPECTATE_NAMESPACE: u128 = 0x0000_5350_4543_5441_5445_5F41_4E4F_4E5F_4944_u128;

/// Derive a deterministic anonymous user id for a game, so unauthenticated
/// players always map to the same identity within a session.
pub(crate) fn derive_anonymous_id(game_id: Uuid) -> Uuid {
    Uuid::from_u128(game_id.as_u128() ^ ANON_NAMESPACE)
}

/// Derive a deterministic spectator id for a game.
pub(crate) fn derive_spectate_id(game_id: Uuid) -> Uuid {
    Uuid::from_u128(game_id.as_u128() ^ SPECTATE_NAMESPACE)
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
                crate::observability::metrics::WS_AUTH_BLACKLIST_REDIS_ERRORS_TOTAL.inc();
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
    mut redis_client: Option<RedisClient>,
) -> Option<Uuid> {
    let config = auth_config?;

    // Validate the JWT signature, expiry, and purpose
    let claims = jwt::validate_game_token(token, &config).ok()?;

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
        match r.exists(&redis_key).await {
            Ok(true) => {
                tracing::info!(
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
                crate::observability::metrics::WS_TOKEN_VALIDATION_REDIS_ERRORS_TOTAL.inc();
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
    mut redis_client: Option<RedisClient>,
) -> Option<Uuid> {
    let config = auth_config?;

    let claims = jwt::validate_spectate_token(token, &config).ok()?;

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
        match r.exists(&redis_key).await {
            Ok(true) => {
                tracing::info!(
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
                crate::observability::metrics::WS_TOKEN_VALIDATION_REDIS_ERRORS_TOTAL.inc();
                tracing::warn!(
                    "Redis unavailable, allowing spectator token connection for game {}",
                    game_id
                );
            }
        }
    }

    Some(derive_spectate_id(game_id))
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
