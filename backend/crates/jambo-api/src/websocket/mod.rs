pub mod manager;
pub mod messages;

mod auth;
pub(crate) mod connection;
mod forwarding;
mod game_state;
mod incoming;
mod membership;
mod room;
mod routing;
mod user;

pub use auth::{validate_game_token, validate_spectate_token, validate_ws_token};

use actix_web::{web, HttpMessage, HttpRequest, HttpResponse};
use actix_ws;
use tracing::info;
use uuid::Uuid;

use crate::auth::config::AuthConfig;
use auth::{authenticate_ws_request, AuthContext};
use connection::{ConnectionContext, WS_SEND_QUEUE_CAPACITY};
use forwarding::spawn_forwarding_task;
use incoming::spawn_incoming_handler;
use jambo_core::observability::CorrelationId;
use jambo_domain::messaging::RedisClient;
use manager::WebSocketManager;
use membership::enforce_participation;
use messages::OutgoingMessage;
use room::ws_room_handler;
use user::ws_user_handler;

/// WebSocket endpoint for a specific game.
/// Path parameter: game_id (UUID)
/// Query parameter: token (optional) — one-time game token for unauthenticated users
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

    let AuthContext {
        user_id,
        is_cookie_auth,
    } = authenticate_ws_request(&req, game_id, auth_config, redis_client).await;

    if user_id.is_none() {
        tracing::warn!(
            "Unauthenticated WebSocket connection attempt for game {}",
            game_id
        );
        return Err(actix_web::error::ErrorUnauthorized(
            "Authentication required",
        ));
    }

    // Cookie-authenticated connections must belong to this game (creator, accepted
    // player, or pending invitee). Anonymous game-token and spectator connections
    // are exempt from the participant check.
    let authenticated_user_id = if is_cookie_auth { user_id } else { None };
    let db = req
        .app_data::<web::Data<sea_orm::DatabaseConnection>>()
        .cloned();
    enforce_participation(db.clone(), authenticated_user_id, game_id).await?;

    let correlation_id = req
        .extensions()
        .get::<CorrelationId>()
        .copied()
        .unwrap_or_else(CorrelationId::new);

    let ws_span = tracing::info_span!(
        "ws_connection",
        correlation_id = %correlation_id,
        game_id = %game_id,
        user_id = %user_id.unwrap_or_default(),
    );
    let _span_guard = ws_span.enter();

    let (res, mut session, stream) = actix_ws::handle(&req, stream)?;

    let (tx, rx) = tokio::sync::mpsc::channel(WS_SEND_QUEUE_CAPACITY);

    // Send a welcome message BEFORE registering the connection
    match serde_json::to_string(&OutgoingMessage::GameJoined { game_id }) {
        Ok(welcome_message) => {
            if let Err(e) = session.text(welcome_message).await {
                tracing::error!(
                    "Failed to send welcome message, not registering connection: {}",
                    e
                );
                return Ok(res);
            }
            info!("Sent welcome message for game {}", game_id);
        }
        Err(e) => {
            tracing::error!(
                "Failed to serialize welcome message, not registering: {}",
                e
            );
            return Ok(res);
        }
    }

    // Register the connection AFTER successful welcome message
    let connection_id = manager.add_connection(game_id, tx, correlation_id).await;

    // Record the authenticated identity on the connection (used for ownership and
    // single-session enforcement in `set_player_for_connection`).
    if let Some(uid) = authenticated_user_id {
        manager
            .set_connection_user(game_id, connection_id, uid)
            .await;
    }

    let ctx = ConnectionContext {
        game_id,
        connection_id,
        correlation_id,
        user_id: authenticated_user_id,
    };

    // Spawn a task that forwards messages from the manager to the WebSocket
    spawn_forwarding_task(manager.clone(), session.clone(), rx, ctx);
    // Spawn a task that handles incoming messages from the client
    spawn_incoming_handler(manager.clone(), db, session, stream, ctx);

    Ok(res)
}

pub fn scope() -> actix_web::Scope {
    web::scope("/ws")
        .service(web::resource("/me").route(web::get().to(ws_user_handler)))
        .service(web::resource("/room/{room_id}").route(web::get().to(ws_room_handler)))
        .service(web::resource("/{game_id}").route(web::get().to(ws_handler)))
}
