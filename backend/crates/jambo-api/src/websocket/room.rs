use actix_web::{web, HttpMessage, HttpRequest, HttpResponse};
use actix_ws;
use futures_util::StreamExt;
use uuid::Uuid;

use crate::auth::config::AuthConfig;
use jambo_core::observability::CorrelationId;
use jambo_domain::messaging::RedisClient;

use super::auth::validate_ws_token;
use super::connection::WS_SEND_QUEUE_CAPACITY;
use super::manager::WebSocketManager;

pub(crate) async fn ws_room_handler(
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

    let (tx, mut rx) = tokio::sync::mpsc::channel(WS_SEND_QUEUE_CAPACITY);

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
            if let Err(e) = session_clone.text(msg.payload.as_ref()).await {
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
