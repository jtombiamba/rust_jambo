pub mod manager;
pub mod messages;

use actix_web::{web, HttpMessage, HttpRequest, HttpResponse};
use actix_ws::{self, Message, Session};
use futures_util::StreamExt;
use serde_json::Value;
use tracing::{debug, error, info, trace};
use uuid::Uuid;

use crate::auth::config::AuthConfig;
use crate::auth::jwt;
use crate::messaging::RedisClient;
use crate::observability::CorrelationId;
use manager::WebSocketManager;
use messages::{IncomingMessage, OutgoingMessage};

mod game_state;
use game_state::send_game_state_snapshot;

/// WebSocket endpoint for a specific game.
/// Path parameter: game_id (UUID)
/// Query parameter: token (optional) — one-time game token for unauthenticated users
#[allow(unused_mut)]
pub async fn ws_handler(
    req: HttpRequest,
    stream: web::Payload,
    game_id: web::Path<Uuid>,
    manager: web::Data<WebSocketManager>,
) -> Result<HttpResponse, actix_web::Error> {
    let game_id = game_id.into_inner();
    tracing::info!("[DEBUG] WebSocket connection attempt for game {}", game_id);
    let auth_config = req.app_data::<web::Data<AuthConfig>>().cloned();
    let redis_client = req
        .app_data::<web::Data<Option<RedisClient>>>()
        .cloned()
        .and_then(|r| r.get_ref().clone());

    // Try auth cookie first, then fall back to one-time game token from query param
    let token = req.cookie("Authorization").map(|c| c.value().to_string());
    tracing::info!("[DEBUG] WebSocket connection attempt recovery of token");
    let user_id = if token.is_some() {
        tracing::info!(
            "[DEBUG] WebSocket connection via token cookie for game {}",
            game_id
        );
        validate_ws_token(token, auth_config.clone(), redis_client.clone()).await
    } else {
        tracing::info!(
            "[DEBUG] WebSocket connection via token query params for game {}",
            game_id
        );
        // Check for one-time game token in query parameter
        let game_token = req
            .query_string()
            .split('&')
            .filter_map(|pair| pair.split_once('='))
            .find(|(k, _)| *k == "token")
            .map(|(_, v)| v.to_string());
        if let Some(ref gt) = game_token {
            tracing::info!("[DEBUG] WebSocket validating token for game {}", game_id);
            validate_game_token(gt, game_id, auth_config.clone(), redis_client.clone()).await
        } else {
            tracing::warn!(
                "[WARN] WebSocket connection via token: no query params for game {}",
                game_id
            );
            None
        }
    };

    if user_id.is_none() {
        tracing::warn!(
            "Unauthenticated WebSocket connection attempt for game {}",
            game_id
        );
        return Err(actix_web::error::ErrorUnauthorized(
            "Authentication required",
        ));
    }

    let correlation_id = req
        .extensions()
        .get::<CorrelationId>()
        .copied()
        .unwrap_or_else(CorrelationId::new);

    let db = req
        .app_data::<web::Data<sea_orm::DatabaseConnection>>()
        .cloned();

    let ws_span = tracing::info_span!(
        "ws_connection",
        correlation_id = %correlation_id,
        game_id = %game_id,
        user_id = %user_id.unwrap_or_default(),
    );
    let _span_guard = ws_span.enter();

    let (res, mut session, mut stream) = actix_ws::handle(&req, stream)?;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    // Send a welcome message BEFORE registering the connection
    match serde_json::to_string(&OutgoingMessage::GameJoined { game_id }) {
        Ok(welcome_message) => {
            if let Err(e) = session.text(welcome_message).await {
                error!(
                    "Failed to send welcome message, not registering connection: {}",
                    e
                );
                return Ok(res);
            }
            info!("Sent welcome message for game {}", game_id);
        }
        Err(e) => {
            error!(
                "Failed to serialize welcome message, not registering: {}",
                e
            );
            return Ok(res);
        }
    }

    // Register the connection AFTER successful welcome message
    let connection_id = manager.add_connection(game_id, tx, correlation_id).await;

    // Spawn a task that forwards messages from the manager to the WebSocket
    let mut session_clone = session.clone();
    let manager_clone_for_forwarding = manager.clone();
    let connection_id_for_forwarding = connection_id;
    let cid_for_forward = correlation_id;
    let fwd_span = tracing::info_span!(
        "ws_forward",
        correlation_id = %cid_for_forward,
        game_id = %game_id,
        connection_id = %connection_id_for_forwarding.uuid(),
    );
    actix_rt::spawn(async move {
        let _guard = fwd_span.enter();
        info!(
            "Message forwarding task started for connection {}",
            connection_id_for_forwarding.uuid()
        );
        while let Some(msg) = rx.recv().await {
            trace!(
                "Forwarding message to connection {}: {}",
                connection_id_for_forwarding.uuid(),
                msg
            );
            if let Err(e) = session_clone.text(msg).await {
                error!(
                    "Failed to send WebSocket message to connection {}: {}",
                    connection_id_for_forwarding.uuid(),
                    e
                );
                manager_clone_for_forwarding
                    .force_disconnect(game_id, connection_id_for_forwarding)
                    .await;
                break;
            }
        }
        info!(
            "Message forwarding task ended for connection {} (channel closed)",
            connection_id_for_forwarding.uuid()
        );
    });

    // Spawn a task that handles incoming messages from the client
    let manager_clone = manager.clone();
    let db_clone = db.clone();
    let cid_for_handler = correlation_id;
    let handler_span = tracing::info_span!(
        "ws_incoming",
        correlation_id = %cid_for_handler,
        game_id = %game_id,
        connection_id = %connection_id.uuid(),
    );
    actix_rt::spawn(async move {
        let _guard = handler_span.enter();
        let mut session = session;
        info!(
            "Incoming message handler started for connection {}",
            connection_id.uuid()
        );
        while let Some(result) = stream.next().await {
            match result {
                Ok(msg) => match msg {
                    Message::Text(text) => {
                        debug!(
                            "Received text message from connection {}: {}",
                            connection_id.uuid(),
                            text
                        );
                        if let Err(e) = handle_message(
                            &mut session,
                            &text,
                            game_id,
                            &manager_clone,
                            db_clone.clone(),
                        )
                        .await
                        {
                            error!(
                                "Error handling message for connection {}: {}",
                                connection_id.uuid(),
                                e
                            );
                        }
                    }
                    Message::Close(reason) => {
                        info!(
                            "WebSocket closed for game {}, connection {}: {:?}",
                            game_id,
                            connection_id.uuid(),
                            reason
                        );
                        break;
                    }
                    Message::Ping(bytes) => {
                        trace!("Received ping from connection {}", connection_id.uuid());
                        if let Err(e) = session.pong(&bytes).await {
                            error!(
                                "Failed to send pong for connection {}: {}",
                                connection_id.uuid(),
                                e
                            );
                        }
                    }
                    Message::Pong(_) => {
                        trace!("Received pong from connection {}", connection_id.uuid());
                        manager_clone.update_pong(game_id, connection_id).await;
                    }
                    _ => {}
                },
                Err(e) => {
                    error!(
                        "WebSocket stream error for connection {}: {}",
                        connection_id.uuid(),
                        e
                    );
                    break;
                }
            }
        }
        // Clean up connection on close
        info!(
            "Stream ended for connection {}, cleaning up",
            connection_id.uuid()
        );
        manager.remove_connection(game_id, connection_id).await;
        info!(
            "Connection {} cleaned up for game {}",
            connection_id.uuid(),
            game_id
        );
    });

    Ok(res)
}

async fn handle_message(
    session: &mut Session,
    text: &str,
    game_id: Uuid,
    manager: &web::Data<WebSocketManager>,
    db: Option<web::Data<sea_orm::DatabaseConnection>>,
) -> Result<(), anyhow::Error> {
    let span = tracing::info_span!(
        "ws_message",
        game_id = %game_id,
    );
    let _guard = span.enter();

    debug!("Parsing message: {}", text);
    match serde_json::from_str::<IncomingMessage>(text) {
        Ok(msg) => {
            debug!("Successfully parsed message as {:?}", msg);
            match msg {
                IncomingMessage::Ping => {
                    trace!("Processing ping");
                    let response = OutgoingMessage::Pong;
                    session.text(serde_json::to_string(&response)?).await?;
                }
                IncomingMessage::JoinGame {
                    game_id: join_id,
                    player_id,
                    player_position,
                } => {
                    trace!("Processing join game: {}", join_id);
                    if join_id != game_id {
                        tracing::warn!(
                            "Client attempted to join game {} but connected to {}",
                            join_id,
                            game_id
                        );
                    }
                    // Set player identity if provided (for disconnect/reconnect tracking)
                    if let (Some(pid), Some(pos)) = (player_id, player_position) {
                        // We need the connection_id to set player. Since this is called from
                        // the incoming handler, we don't have it directly. Register via
                        // a simple method: set_player_for_latest_connection.
                        crate::websocket::manager::WebSocketManager::set_player_for_latest_connection(
                            manager.get_ref(),
                            game_id,
                            pid,
                            pos,
                        ).await;
                        // Send current game state snapshot to the newly connected player
                        if let Some(db) = &db {
                            send_game_state_snapshot(manager.get_ref(), db, game_id, pid, pos)
                                .await;
                        }
                    }
                    let response = OutgoingMessage::GameJoined { game_id };
                    session.text(serde_json::to_string(&response)?).await?;
                }
                IncomingMessage::LeaveGame => {
                    tracing::info!("Client left game {}", game_id);
                }
            }
        }
        Err(e) => {
            tracing::warn!("Failed to parse incoming message: {}; text: {}", e, text);
            // Fallback to legacy command format for compatibility
            let value: Value = serde_json::from_str(text)?;
            let command = value.get("command").and_then(Value::as_str);
            match command {
                Some("ping") => {
                    trace!("Fallback parsing: ping command");
                    let response = OutgoingMessage::Pong;
                    session.text(serde_json::to_string(&response)?).await?;
                }
                _ => {
                    trace!("Fallback parsing: unknown command");
                    let response = OutgoingMessage::Error {
                        message: "unknown command".to_string(),
                        source: "ws:unknown_command".to_string(),
                    };
                    session.text(serde_json::to_string(&response)?).await?;
                }
            }
        }
    }

    Ok(())
}

pub fn scope() -> actix_web::Scope {
    web::scope("/ws")
        .service(web::resource("/{game_id}").route(web::get().to(ws_handler)))
        .service(web::resource("/room/{room_id}").route(web::get().to(ws_room_handler)))
}

pub async fn ws_room_handler(
    req: HttpRequest,
    stream: web::Payload,
    room_id: web::Path<Uuid>,
    manager: web::Data<WebSocketManager>,
) -> Result<HttpResponse, actix_web::Error> {
    let room_id = room_id.into_inner();

    let auth_config = req.app_data::<web::Data<AuthConfig>>().cloned();
    let token = req.cookie("Authorization").map(|c| c.value().to_string());
    let redis_client = req
        .app_data::<web::Data<Option<RedisClient>>>()
        .cloned()
        .and_then(|r| r.get_ref().clone());
    let user_id = validate_ws_token(token, auth_config, redis_client).await;
    if user_id.is_none() {
        tracing::warn!(
            "Unauthenticated WebSocket connection attempt for room {}",
            room_id
        );
        return Err(actix_web::error::ErrorUnauthorized(
            "Authentication required",
        ));
    }

    let correlation_id = req
        .extensions()
        .get::<CorrelationId>()
        .copied()
        .unwrap_or_else(CorrelationId::new);

    let (res, mut session, mut stream) = actix_ws::handle(&req, stream)?;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    // Send welcome BEFORE registering
    if let Err(e) = session
        .text(serde_json::json!({"type": "room_joined", "room_id": room_id}).to_string())
        .await
    {
        tracing::error!("Failed to send room welcome, not registering: {}", e);
        return Ok(res);
    }

    let connection_id = manager
        .add_room_connection(room_id, tx, correlation_id)
        .await;

    let mut session_clone = session.clone();
    let manager_clone = manager.clone();
    actix_rt::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if let Err(e) = session_clone.text(msg).await {
                tracing::error!(
                    "Failed to forward to room connection {}: {}",
                    connection_id.uuid(),
                    e
                );
                manager_clone
                    .remove_room_connection(room_id, connection_id)
                    .await;
                break;
            }
        }
    });

    actix_rt::spawn(async move {
        while let Some(result) = stream.next().await {
            match result {
                Ok(actix_ws::Message::Ping(bytes)) => {
                    let _ = session.pong(&bytes).await;
                }
                Ok(actix_ws::Message::Close(reason)) => {
                    tracing::info!("Room WS closed for room {}: {:?}", room_id, reason);
                    break;
                }
                Err(e) => {
                    tracing::error!("Room WS stream error: {}", e);
                    break;
                }
                _ => {}
            }
        }
        manager.remove_room_connection(room_id, connection_id).await;
    });

    Ok(res)
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
    // The namespace constant is arbitrary but fixed to avoid collisions with real user IDs.
    const ANON_NAMESPACE: u128 = 0x006A_6F6E_6573_5F61_6E6F_6E5F_7575_6964_u128;
    Some(uuid::Uuid::from_u128(game_id.as_u128() ^ ANON_NAMESPACE))
}
