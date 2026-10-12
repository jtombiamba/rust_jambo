use actix_web::web;
use actix_ws::{self, Session};
use tracing::{error, info};

use super::connection::{ConnectionContext, WsMessage};
use super::manager::WebSocketManager;

/// Spawn the task that forwards messages from the manager's per-connection
/// channel to the WebSocket session.
pub(super) fn spawn_forwarding_task(
    manager: web::Data<WebSocketManager>,
    session: Session,
    rx: tokio::sync::mpsc::Receiver<WsMessage>,
    ctx: ConnectionContext,
) {
    let fwd_span = tracing::info_span!(
        "ws_forward",
        correlation_id = %ctx.correlation_id,
        game_id = %ctx.game_id,
        connection_id = %ctx.connection_id.uuid(),
    );
    actix_rt::spawn(async move {
        let _guard = fwd_span.enter();
        info!(
            "Message forwarding task started for connection {}",
            ctx.connection_id.uuid()
        );
        match forward_to_sink(session, rx).await {
            ForwardOutcome::SendFailed => {
                error!(
                    "Failed to send WebSocket message to connection {}",
                    ctx.connection_id.uuid()
                );
                manager
                    .force_disconnect(ctx.game_id, ctx.connection_id)
                    .await;
            }
            ForwardOutcome::ChannelClosed => {
                info!(
                    "Connection {} send channel closed, closing session so the client can reconnect",
                    ctx.connection_id.uuid()
                );
            }
        }
        info!(
            "Message forwarding task ended for connection {} (channel closed)",
            ctx.connection_id.uuid()
        );
    });
}

/// Outcome of a forwarding loop, used to drive post-loop cleanup in the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ForwardOutcome {
    /// The manager dropped the sender (connection culled or removed). The sink has
    /// already been closed so the client's `onclose` fires and it can reconnect.
    ChannelClosed,
    /// Sending a message failed, meaning the underlying socket is already broken.
    SendFailed,
}

/// Abstraction over the outbound half of a WebSocket connection so the forwarding
/// loop can be unit-tested without a real socket.
#[async_trait::async_trait]
trait ForwardSink {
    async fn send_text(&mut self, text: &str) -> Result<(), ()>;
    async fn close(self) -> Result<(), ()>;
}

#[async_trait::async_trait]
impl ForwardSink for Session {
    async fn send_text(&mut self, text: &str) -> Result<(), ()> {
        self.text(text).await.map_err(|_| ())
    }

    async fn close(self) -> Result<(), ()> {
        actix_ws::Session::close(self, None).await.map_err(|_| ())
    }
}

/// Forward messages from the manager's per-connection channel to the WebSocket
/// sink. When the channel is closed (the manager dropped the sender, e.g. a
/// stale/heartbeat cleanup) the sink is explicitly closed so the client is
/// notified and can reconnect instead of being left in a zombie state.
async fn forward_to_sink<S: ForwardSink>(
    mut sink: S,
    mut rx: tokio::sync::mpsc::Receiver<WsMessage>,
) -> ForwardOutcome {
    while let Some(msg) = rx.recv().await {
        jambo_core::observability::metrics::WS_SEND_QUEUE_DEPTH.observe(rx.len() as f64);
        if sink.send_text(msg.payload.as_ref()).await.is_err() {
            return ForwardOutcome::SendFailed;
        }
    }
    let _ = sink.close().await;
    ForwardOutcome::ChannelClosed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::websocket::connection::{MessageKind, WS_SEND_QUEUE_CAPACITY};
    use std::sync::{Arc, Mutex};
    use tokio::sync::mpsc;

    #[derive(Default)]
    struct MockState {
        sent: Vec<String>,
        closed: bool,
    }

    struct MockSink {
        state: Arc<Mutex<MockState>>,
        fail_on_send: bool,
    }

    impl MockSink {
        fn new(state: Arc<Mutex<MockState>>, fail_on_send: bool) -> Self {
            Self {
                state,
                fail_on_send,
            }
        }
    }

    #[async_trait::async_trait]
    impl ForwardSink for MockSink {
        async fn send_text(&mut self, text: &str) -> Result<(), ()> {
            if self.fail_on_send {
                return Err(());
            }
            self.state.lock().unwrap().sent.push(text.to_string());
            Ok(())
        }

        async fn close(self) -> Result<(), ()> {
            self.state.lock().unwrap().closed = true;
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_forward_to_sink_closes_sink_when_channel_closed() {
        let state = Arc::new(Mutex::new(MockState::default()));
        let (tx, rx) = mpsc::channel(WS_SEND_QUEUE_CAPACITY);

        tx.send(WsMessage {
            payload: std::sync::Arc::from("hello"),
            kind: MessageKind::Snapshot,
        })
        .await
        .unwrap();
        // Dropping the sender simulates the manager culling/removing the
        // connection (the forwarding task must close the client socket).
        drop(tx);

        let outcome = forward_to_sink(MockSink::new(state.clone(), false), rx).await;

        assert_eq!(outcome, ForwardOutcome::ChannelClosed);
        assert_eq!(state.lock().unwrap().sent, vec!["hello".to_string()]);
        assert!(
            state.lock().unwrap().closed,
            "a culled connection must be closed so the client's onclose fires and it reconnects"
        );
    }

    #[tokio::test]
    async fn test_forward_to_sink_does_not_close_when_send_fails() {
        let state = Arc::new(Mutex::new(MockState::default()));
        let (tx, rx) = mpsc::channel(WS_SEND_QUEUE_CAPACITY);

        tx.send(WsMessage {
            payload: std::sync::Arc::from("hello"),
            kind: MessageKind::Snapshot,
        })
        .await
        .unwrap();
        drop(tx);

        let outcome = forward_to_sink(MockSink::new(state.clone(), true), rx).await;

        assert_eq!(outcome, ForwardOutcome::SendFailed);
        assert!(
            !state.lock().unwrap().closed,
            "a send failure means the socket is already broken and must not be explicitly closed"
        );
    }
}
