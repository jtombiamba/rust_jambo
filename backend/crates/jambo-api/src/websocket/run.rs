use actix_web::{web, HttpMessage, HttpRequest, HttpResponse};
use actix_ws;
use futures_util::StreamExt;
use uuid::Uuid;

use crate::auth::config::AuthConfig;
use jambo_core::observability::CorrelationId;
use jambo_db::repositories::{GameRunGameRepository, GameRunRepository};
use jambo_domain::messaging::RedisClient;

use super::auth::validate_run_spectate_token;
use super::connection::{ConnectionId, WS_SEND_QUEUE_CAPACITY};
use super::manager::WebSocketManager;

/// Resolve the current (most recently started) game id for a run. Mirrors the
/// `get_room_detail` "current game" resolution: the run's `current_game_index`
/// is the count of games started so far, so the live game is at index
/// `current_game_index - 1`.
async fn resolve_current_game_id(db: &sea_orm::DatabaseConnection, run_id: Uuid) -> Option<Uuid> {
    let run = GameRunRepository::new(db.clone())
        .find_by_id(run_id)
        .await
        .ok()
        .flatten()?;
    if run.current_game_index <= 0 {
        return None;
    }
    GameRunGameRepository::new(db.clone())
        .find_by_run_and_index(run_id, run.current_game_index - 1)
        .await
        .ok()
        .flatten()
        .map(|rg| rg.game_id)
}

/// Dedicated WebSocket endpoint for a run spectator. A single connection follows
/// the entire run: the server re-points it to each new `game_id` when a game
/// starts, so a streamer can share one link for the whole run.
pub(crate) async fn ws_run_spectator_handler(
    req: HttpRequest,
    stream: web::Payload,
    run_id: web::Path<Uuid>,
    manager: web::Data<WebSocketManager>,
) -> Result<HttpResponse, actix_web::Error> {
    let run_id = run_id.into_inner();

    let auth_config = req.app_data::<web::Data<AuthConfig>>().cloned();
    let redis_client = req
        .app_data::<web::Data<Option<RedisClient>>>()
        .cloned()
        .and_then(|r| r.get_ref().clone());

    let token = req
        .query_string()
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(k, _)| *k == "token")
        .map(|(_, v)| v.to_string());

    let Some(token) = token else {
        tracing::warn!("Run spectator connection without token for run {}", run_id);
        return Err(actix_web::error::ErrorUnauthorized(
            "Authentication required",
        ));
    };

    if validate_run_spectate_token(&token, run_id, auth_config, redis_client)
        .await
        .is_none()
    {
        tracing::warn!("Invalid run spectate token for run {}", run_id);
        return Err(actix_web::error::ErrorUnauthorized(
            "Authentication required",
        ));
    }

    let Some(db) = req
        .app_data::<web::Data<sea_orm::DatabaseConnection>>()
        .cloned()
    else {
        return Err(actix_web::error::ErrorInternalServerError(
            "Database unavailable",
        ));
    };

    let current_game_id = resolve_current_game_id(db.get_ref(), run_id).await;

    let correlation_id = req
        .extensions()
        .get::<CorrelationId>()
        .copied()
        .unwrap_or_else(CorrelationId::new);

    let (res, mut session, mut stream) = actix_ws::handle(&req, stream)?;
    let (tx, mut rx) = tokio::sync::mpsc::channel(WS_SEND_QUEUE_CAPACITY);

    // Register the connection in the game map (when a game is already running)
    // so it receives public game events like any spectator, then mark it as a
    // spectator and track it in the run registry.
    let connection_id = match current_game_id {
        Some(game_id) => {
            let id = manager
                .add_connection(game_id, tx.clone(), correlation_id)
                .await;
            manager.mark_spectator_for_connection(game_id, id).await;
            id
        }
        None => ConnectionId::new(),
    };

    manager
        .register_run_spectator(run_id, connection_id, tx, correlation_id, current_game_id)
        .await;

    let welcome = serde_json::json!({
        "type": "run_joined",
        "run_id": run_id,
        "game_id": current_game_id,
    });
    if session.text(welcome.to_string()).await.is_err() {
        manager.remove_run_spectator(run_id, connection_id).await;
        return Ok(res);
    }

    if let Some(game_id) = current_game_id {
        super::game_state::send_spectator_snapshot(manager.get_ref(), db.get_ref(), game_id).await;
    }

    // Forwarding task: pull from the manager channel and write to the socket.
    let manager_fwd = manager.clone();
    let mut session_fwd = session.clone();
    actix_rt::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if let Err(e) = session_fwd.text(msg.payload.as_ref()).await {
                tracing::error!(
                    "Failed to forward to run spectator {}: {}",
                    connection_id.uuid(),
                    e
                );
                manager_fwd
                    .remove_run_spectator(run_id, connection_id)
                    .await;
                break;
            }
        }
    });

    // Incoming task: heartbeats only (spectators are read-only). Cleanup on close.
    let manager_in = manager.clone();
    actix_rt::spawn(async move {
        while let Some(result) = stream.next().await {
            match result {
                Ok(actix_ws::Message::Ping(bytes)) => {
                    let _ = session.pong(&bytes).await;
                }
                Ok(actix_ws::Message::Pong(_)) => {
                    manager_in.touch_run_spectator(run_id, connection_id).await;
                }
                Ok(actix_ws::Message::Text(text)) => {
                    if text.contains("\"ping\"") {
                        manager_in.touch_run_spectator(run_id, connection_id).await;
                        let _ = session.text("{\"type\":\"pong\"}").await;
                    }
                }
                Ok(actix_ws::Message::Close(reason)) => {
                    tracing::info!("Run spectator WS closed for run {}: {:?}", run_id, reason);
                    break;
                }
                Err(e) => {
                    tracing::error!("Run spectator WS stream error: {}", e);
                    break;
                }
                _ => {}
            }
        }
        manager_in.remove_run_spectator(run_id, connection_id).await;
    });

    Ok(res)
}
