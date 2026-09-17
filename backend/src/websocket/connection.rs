use std::time::Instant;
use uuid::Uuid;

use crate::observability::CorrelationId;

/// A single WebSocket connection is represented by a sender that can forward messages.
pub type WsSender = tokio::sync::mpsc::UnboundedSender<String>;

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
