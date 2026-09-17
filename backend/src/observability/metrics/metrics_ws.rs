use once_cell::sync::Lazy;
use prometheus::{
    register_counter, register_gauge, register_gauge_vec, register_histogram, Counter, Gauge,
    GaugeVec, Histogram,
};

pub static WS_MESSAGES_SENT_TOTAL: Lazy<Counter> = Lazy::new(|| {
    register_counter!(
        "ws_messages_sent_total",
        "Total number of WebSocket messages sent to clients"
    )
    .unwrap()
});

pub static WS_CONNECTIONS_ACTIVE: Lazy<Gauge> = Lazy::new(|| {
    register_gauge!(
        "ws_connections_active",
        "Current number of active WebSocket connections"
    )
    .unwrap()
});

pub static WS_DISCONNECTS_TOTAL: Lazy<Counter> = Lazy::new(|| {
    register_counter!(
        "ws_disconnects_total",
        "Total number of WebSocket disconnections"
    )
    .unwrap()
});

pub static WS_HEARTBEAT_TIMEOUTS_TOTAL: Lazy<Counter> = Lazy::new(|| {
    register_counter!(
        "ws_heartbeat_timeouts_total",
        "Total number of WebSocket connections timed out on heartbeat"
    )
    .unwrap()
});

/// Number of spectator connections for a single game, labelled by game_id.
///
/// This is a state gauge: the series is set when a game has at least one
/// spectator and removed when the last spectator leaves (or the game ends) to
/// bound label cardinality. Alerting uses `sum by (game_id)` to aggregate
/// across instances, since a single game's spectators may be spread over
/// multiple backend replicas.
pub static WS_SPECTATORS_PER_GAME: Lazy<GaugeVec> = Lazy::new(|| {
    register_gauge_vec!(
        "ws_spectators_per_game",
        "Current number of spectator connections for a single game",
        &["game_id"]
    )
    .unwrap()
});

/// Number of WebSocket sends that failed because the receiving channel was
/// already closed (a dead client). Meaningful with both unbounded and bounded
/// channels, so it is the earliest lagging indicator of a disconnected peer.
pub static WS_SEND_FAILED_TOTAL: Lazy<Counter> = Lazy::new(|| {
    register_counter!(
        "ws_send_failed_total",
        "Total number of WebSocket send failures due to a closed receiver"
    )
    .unwrap()
});

/// Number of messages dropped because the per-connection send queue was full.
/// Only becomes non-zero once the bounded-channel backpressure change is in
/// place; registered early so dashboards and alerts are wired ahead of time.
pub static WS_MESSAGES_DROPPED_TOTAL: Lazy<Counter> = Lazy::new(|| {
    register_counter!(
        "ws_messages_dropped_total",
        "Total number of WebSocket messages dropped due to a full send queue"
    )
    .unwrap()
});

/// Number of connections torn down because a `Control` message hit a full
/// queue. Only becomes non-zero once the bounded-channel backpressure change
/// is in place; registered early so alerts are wired ahead of time.
pub static WS_SLOW_CONSUMER_DISCONNECTS_TOTAL: Lazy<Counter> = Lazy::new(|| {
    register_counter!(
        "ws_slow_consumer_disconnects_total",
        "Total number of WebSocket connections disconnected for a full send queue"
    )
    .unwrap()
});

/// Observed depth of a per-connection send queue at receive time. Recorded in
/// the forwarding task before each `recv()`. Only meaningful once bounded
/// channels are in place (unbounded queues are almost always near zero).
///
/// Registered ahead of the bounded-channel backpressure change so dashboards
/// and alerts can reference it before the observation sites land.
#[allow(dead_code)]
pub static WS_SEND_QUEUE_DEPTH: Lazy<Histogram> = Lazy::new(|| {
    register_histogram!(
        "ws_send_queue_depth",
        "Observed depth of a per-connection WebSocket send queue",
        vec![1.0, 4.0, 16.0, 64.0, 256.0, 1024.0, 4096.0]
    )
    .unwrap()
});
