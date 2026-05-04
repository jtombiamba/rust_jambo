use super::CorrelationId;

/// Create a tracing span for a WebSocket connection lifecycle.
/// The correlation_id is inherited from the HTTP request that established
/// the WebSocket upgrade.
#[allow(dead_code)]
pub fn ws_connection_span(
    correlation_id: CorrelationId,
    game_id: &uuid::Uuid,
) -> tracing::Span {
    tracing::info_span!(
        "ws_connection",
        correlation_id = %correlation_id,
        game_id = %game_id,
    )
}

/// Create a tracing span for handling an individual WebSocket message.
#[allow(dead_code)]
pub fn ws_message_span(
    correlation_id: CorrelationId,
    game_id: &uuid::Uuid,
    session_id: &CorrelationId,
) -> tracing::Span {
    tracing::info_span!(
        "ws_message",
        correlation_id = %correlation_id,
        game_id = %game_id,
        session_id = %session_id,
    )
}
