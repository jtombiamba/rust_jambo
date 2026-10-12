use actix_web::web;
use actix_ws::{self, Message, Session};
use futures_util::StreamExt;
use serde_json::Value;
use tracing::{debug, error, info, trace};
use uuid::Uuid;

use super::connection::{ConnectionContext, ConnectionId};
use super::game_state::{send_game_state_snapshot, send_spectator_snapshot};
use super::manager::WebSocketManager;
use super::membership::resolve_player_for_user;
use super::messages::{IncomingMessage, OutgoingMessage};

/// Spawn the task that reads incoming messages from the client and cleans up
/// the connection once the stream ends.
pub(super) fn spawn_incoming_handler(
    manager: web::Data<WebSocketManager>,
    db: Option<web::Data<sea_orm::DatabaseConnection>>,
    session: Session,
    stream: actix_ws::MessageStream,
    ctx: ConnectionContext,
) {
    let handler_span = tracing::info_span!(
        "ws_incoming",
        correlation_id = %ctx.correlation_id,
        game_id = %ctx.game_id,
        connection_id = %ctx.connection_id.uuid(),
    );
    actix_rt::spawn(async move {
        let _guard = handler_span.enter();
        let mut session = session;
        let mut stream = stream;
        info!(
            "Incoming message handler started for connection {}",
            ctx.connection_id.uuid()
        );
        while let Some(result) = stream.next().await {
            match result {
                Ok(msg) => match msg {
                    Message::Text(text) => {
                        debug!(
                            "Received text message from connection {}: {}",
                            ctx.connection_id.uuid(),
                            text
                        );
                        if let Err(e) = handle_message(
                            &mut session,
                            &text,
                            ctx.game_id,
                            ctx.connection_id,
                            &manager,
                            db.clone(),
                            ctx.user_id,
                        )
                        .await
                        {
                            error!(
                                "Error handling message for connection {}: {}",
                                ctx.connection_id.uuid(),
                                e
                            );
                        }
                    }
                    Message::Close(reason) => {
                        info!(
                            "WebSocket closed for game {}, connection {}: {:?}",
                            ctx.game_id,
                            ctx.connection_id.uuid(),
                            reason
                        );
                        break;
                    }
                    Message::Ping(bytes) => {
                        trace!("Received ping from connection {}", ctx.connection_id.uuid());
                        if let Err(e) = session.pong(&bytes).await {
                            error!(
                                "Failed to send pong for connection {}: {}",
                                ctx.connection_id.uuid(),
                                e
                            );
                        }
                    }
                    Message::Pong(_) => {
                        trace!("Received pong from connection {}", ctx.connection_id.uuid());
                        manager.update_pong(ctx.game_id, ctx.connection_id).await;
                    }
                    _ => {}
                },
                Err(e) => {
                    error!(
                        "WebSocket stream error for connection {}: {}",
                        ctx.connection_id.uuid(),
                        e
                    );
                    break;
                }
            }
        }
        // Clean up connection on close
        info!(
            "Stream ended for connection {}, cleaning up",
            ctx.connection_id.uuid()
        );
        manager
            .remove_connection(ctx.game_id, ctx.connection_id)
            .await;
        info!(
            "Connection {} cleaned up for game {}",
            ctx.connection_id.uuid(),
            ctx.game_id
        );
    });
}

async fn handle_message(
    session: &mut Session,
    text: &str,
    game_id: Uuid,
    connection_id: ConnectionId,
    manager: &web::Data<WebSocketManager>,
    db: Option<web::Data<sea_orm::DatabaseConnection>>,
    user_id: Option<Uuid>,
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
                    // Application-level pings are the client's keep-alive signal.
                    // Refresh both heartbeat tracks so `cleanup_stale_connections`
                    // never culls a live, pinging connection (the protocol-level
                    // pong is never sent because the server never initiates pings).
                    manager.update_activity(game_id, connection_id).await;
                    manager.update_pong(game_id, connection_id).await;
                    let response = OutgoingMessage::Pong;
                    session.text(serde_json::to_string(&response)?).await?;
                }
                IncomingMessage::JoinGame {
                    game_id: join_id,
                    player_id,
                    player_position,
                    spectator,
                } => {
                    trace!("Processing join game: {}", join_id);
                    if join_id != game_id {
                        tracing::warn!(
                            "Client attempted to join game {} but connected to {}",
                            join_id,
                            game_id
                        );
                    }
                    if spectator {
                        // Read-only spectator join: public state only, no player identity.
                        manager
                            .mark_spectator_for_connection(game_id, connection_id)
                            .await;
                        if let Some(db) = &db {
                            send_spectator_snapshot(manager.get_ref(), db, game_id).await;
                        }
                    } else {
                        // Resolve the effective player identity. Cookie-authenticated
                        // connections derive it from the user's player row (server-side)
                        // so a client cannot spoof another player's id/position. Anonymous
                        // game-token connections fall back to the client-supplied identity.
                        let effective = match (user_id, db.as_deref()) {
                            (Some(uid), Some(db_ref)) => {
                                match resolve_player_for_user(db_ref, game_id, uid).await {
                                    Ok(Some((pid, pos))) => Some((pid, pos)),
                                    Ok(None) => {
                                        tracing::warn!(
                                            "User {} is not a player in game {}, ignoring join",
                                            uid,
                                            game_id
                                        );
                                        None
                                    }
                                    Err(e) => {
                                        tracing::error!(
                                            "DB error resolving player for user {} in game {}: {}",
                                            uid,
                                            game_id,
                                            e
                                        );
                                        None
                                    }
                                }
                            }
                            _ => match (player_id, player_position) {
                                (Some(pid), Some(pos)) => Some((pid, pos)),
                                _ => None,
                            },
                        };

                        if let Some((pid, pos)) = effective {
                            // Associate the identity with THIS connection (not the most
                            // recently added one) so a player's join is never mis-attributed
                            // to a spectator or stale reconnect connection.
                            manager
                                .set_player_for_connection(game_id, connection_id, pid, pos)
                                .await;
                            // Send current game state snapshot to the newly connected player
                            if let Some(db) = &db {
                                send_game_state_snapshot(manager.get_ref(), db, game_id, pid, pos)
                                    .await;
                            }
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
