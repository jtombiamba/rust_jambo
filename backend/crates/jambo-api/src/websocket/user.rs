use crate::auth::config::AuthConfig;
use actix_web::{web, HttpMessage, HttpRequest, HttpResponse};
use actix_ws;
use futures_util::StreamExt;
use jambo_core::observability::CorrelationId;
use jambo_domain::messaging::RedisClient;

use super::auth::validate_ws_token;
use super::connection::WS_SEND_QUEUE_CAPACITY;
use super::manager::WebSocketManager;
use super::messages::{IncomingMessage, OutgoingMessage};

/// WebSocket endpoint for a single authenticated user.
/// Used to push user-scoped events (e.g. game invitations) in real time.
pub(crate) async fn ws_user_handler(
    req: HttpRequest,
    stream: web::Payload,
    manager: web::Data<WebSocketManager>,
) -> Result<HttpResponse, actix_web::Error> {
    let auth_config = req.app_data::<web::Data<AuthConfig>>().cloned();
    let token = req.cookie("Authorization").map(|c| c.value().to_string());
    let redis_client = req
        .app_data::<web::Data<Option<RedisClient>>>()
        .cloned()
        .and_then(|r| r.get_ref().clone());
    let user_id = validate_ws_token(token, auth_config, redis_client).await;
    if user_id.is_none() {
        tracing::warn!("Unauthenticated user WebSocket connection attempt");
        return Err(actix_web::error::ErrorUnauthorized(
            "Authentication required",
        ));
    }
    let user_id = user_id.unwrap();
    tracing::debug!("User WebSocket connection established for user {}", user_id);

    let correlation_id = req
        .extensions()
        .get::<CorrelationId>()
        .copied()
        .unwrap_or_else(CorrelationId::new);

    let (res, mut session, mut stream) = actix_ws::handle(&req, stream)?;

    let (tx, mut rx) = tokio::sync::mpsc::channel(WS_SEND_QUEUE_CAPACITY);

    if let Err(e) = session
        .text(serde_json::json!({"type": "user_joined", "user_id": user_id}).to_string())
        .await
    {
        tracing::error!("Failed to send user welcome, not registering: {}", e);
        return Ok(res);
    }

    let connection_id = manager
        .add_user_connection(user_id, tx, correlation_id)
        .await;
    tracing::debug!(
        "User WebSocket connection registered for user {} with connection ID {}",
        user_id,
        connection_id.uuid()
    );

    let mut session_clone = session.clone();
    let manager_clone = manager.clone();
    actix_rt::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if let Err(e) = session_clone.text(msg.payload.as_ref()).await {
                tracing::error!(
                    "Failed to forward to user connection {}: {}",
                    connection_id.uuid(),
                    e
                );
                manager_clone
                    .remove_user_connection(user_id, connection_id)
                    .await;
                break;
            }
        }
    });

    actix_rt::spawn(async move {
        while let Some(result) = stream.next().await {
            match result {
                Ok(actix_ws::Message::Text(text)) => {
                    if let Ok(IncomingMessage::Ping) =
                        serde_json::from_str::<IncomingMessage>(&text)
                    {
                        let response = OutgoingMessage::Pong;
                        let _ = session
                            .text(serde_json::to_string(&response).unwrap_or_default())
                            .await;
                        manager.update_user_pong(user_id, connection_id).await;
                    }
                }
                Ok(actix_ws::Message::Ping(bytes)) => {
                    let _ = session.pong(&bytes).await;
                }
                Ok(actix_ws::Message::Pong(_)) => {
                    manager.update_user_pong(user_id, connection_id).await;
                }
                Ok(actix_ws::Message::Close(reason)) => {
                    tracing::debug!("User WS closed for user {}: {:?}", user_id, reason);
                    break;
                }
                Err(e) => {
                    tracing::error!("User WS stream error: {}", e);
                    break;
                }
                _ => {}
            }
        }
        manager.remove_user_connection(user_id, connection_id).await;
    });

    Ok(res)
}
