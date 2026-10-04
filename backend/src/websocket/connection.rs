use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;

use crate::observability::CorrelationId;

/// Backpressure classification for an outbound WebSocket message.
///
/// The per-connection send queue is bounded (`WS_SEND_QUEUE_CAPACITY`). When a
/// queue is full, the overflow policy depends on the message kind:
///
/// - `Snapshot` — idempotent, latest-wins payload (e.g. `game_state_snapshot`).
///   Safe to drop: the next snapshot supersedes it.
/// - `Control` — stateful, one-shot payload (e.g. `game_started`, `player_kicked`,
///   errors). Dropping it would corrupt client state, so a full queue is treated
///   as a dead client and the connection is disconnected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageKind {
    Snapshot,
    Control,
}

/// A single outbound WebSocket message: a shared payload plus its backpressure
/// classification. The payload is `Arc<str>` so a broadcast serializes once and
/// shares the allocation across every recipient via cheap `Arc` clones.
#[derive(Debug, Clone)]
pub struct WsMessage {
    pub payload: Arc<str>,
    pub kind: MessageKind,
}

/// Capacity of each per-connection send queue.
///
/// A full queue is the fastest signal of a stalled client (mobile drop, suspended
/// tab, TCP zero-window). Bounding the queue caps per-connection memory at
/// `WS_SEND_QUEUE_CAPACITY × message_size`, turning the previous latent OOM risk
/// into an explicit, observable drop-vs-disconnect decision (see `MessageKind`).
pub const WS_SEND_QUEUE_CAPACITY: usize = 256;

/// A single WebSocket connection is represented by a bounded sender.
pub type WsSender = tokio::sync::mpsc::Sender<WsMessage>;

/// Connection identifier for tracking individual WebSocket connections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConnectionId(pub Uuid);

impl ConnectionId {
    /// Generate a new unique connection ID.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Get the underlying UUID.
    pub fn uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for ConnectionId {
    fn default() -> Self {
        Self::new()
    }
}

/// Represents a tracked WebSocket connection.
pub(crate) struct TrackedConnection {
    pub(crate) sender: WsSender,
    pub(crate) id: ConnectionId,
    #[allow(dead_code)]
    pub(crate) correlation_id: CorrelationId,
    pub(crate) last_activity: Instant,
    pub(crate) player_id: Option<Uuid>,
    pub(crate) player_position: Option<i32>,
    pub(crate) disconnected: bool,
    pub(crate) last_pong: Instant,
    pub(crate) spectator: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_id_generates_unique_ids() {
        let a = ConnectionId::new();
        let b = ConnectionId::new();
        assert_ne!(a, b);
    }

    #[test]
    fn connection_id_uuid_roundtrips() {
        let id = ConnectionId::new();
        assert_eq!(id.uuid(), id.0);
    }

    #[test]
    fn connection_id_default_is_new() {
        let a = ConnectionId::default();
        let b = ConnectionId::default();
        assert_ne!(a, b);
    }
}
