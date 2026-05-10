pub mod manager;
pub mod messages;

use actix_web::{web, HttpMessage, HttpRequest, HttpResponse};
use actix_ws::{self, Message, Session};
use futures_util::StreamExt;
use serde_json::Value;
use tracing::{debug, error, info, trace};
use uuid::Uuid;

use crate::observability::CorrelationId;
use manager::WebSocketManager;
use messages::{IncomingMessage, OutgoingMessage};

/// WebSocket endpoint for a specific game.
/// Path parameter: game_id (UUID)
#[allow(unused_mut)]
pub async fn ws_handler(
    req: HttpRequest,
    stream: web::Payload,
    game_id: web::Path<Uuid>,
    manager: web::Data<WebSocketManager>,
) -> Result<HttpResponse, actix_web::Error> {
    let game_id = game_id.into_inner();

    // Extract correlation ID from the HTTP request (set by CorrelationIdMiddleware)
    let correlation_id = req
        .extensions()
        .get::<CorrelationId>()
        .copied()
        .unwrap_or_else(CorrelationId::new);

    let ws_span = tracing::info_span!(
        "ws_connection",
        correlation_id = %correlation_id,
        game_id = %game_id,
    );
    let _span_guard = ws_span.enter();

    let (res, mut session, mut stream) = actix_ws::handle(&req, stream)?;

    // Create a channel to send messages from the manager to this WebSocket
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    // Register the connection with the manager and get a connection ID
    let connection_id = manager.add_connection(game_id, tx, correlation_id).await;

    // Send a welcome message immediately to keep connection alive
    match serde_json::to_string(&OutgoingMessage::GameJoined { game_id }) {
        Ok(welcome_message) => {
            if let Err(e) = session.text(welcome_message).await {
                error!(
                    "Failed to send welcome message to connection {}: {}",
                    connection_id.uuid(),
                    e
                );
                // Connection might be already closed, but we'll continue anyway
            } else {
                info!(
                    "Sent welcome message to connection {}",
                    connection_id.uuid()
                );
            }
        }
        Err(e) => {
            error!("Failed to serialize welcome message: {}", e);
            // Continue without welcome message
        }
    }

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
                    .remove_connection(game_id, connection_id_for_forwarding)
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
                        if let Err(e) =
                            handle_message(&mut session, &text, game_id, &manager_clone).await
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
                    };
                    session.text(serde_json::to_string(&response)?).await?;
                }
            }
        }
    }

    Ok(())
}

pub fn scope() -> actix_web::Scope {
    web::scope("/ws").service(web::resource("/{game_id}").route(web::get().to(ws_handler)))
}
