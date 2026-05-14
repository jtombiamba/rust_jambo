use once_cell::sync::Lazy;
use prometheus::{
    register_counter, register_counter_vec, register_gauge, register_histogram_vec, Counter,
    CounterVec, Gauge, HistogramVec,
};

pub static RABBITMQ_PUBLISH_TOTAL: Lazy<CounterVec> = Lazy::new(|| {
    register_counter_vec!(
        "rabbitmq_publish_total",
        "Total number of messages published to RabbitMQ",
        &["queue"]
    )
    .unwrap()
});

pub static RABBITMQ_PUBLISH_ERRORS_TOTAL: Lazy<CounterVec> = Lazy::new(|| {
    register_counter_vec!(
        "rabbitmq_publish_errors_total",
        "Total number of failed publish attempts",
        &["queue"]
    )
    .unwrap()
});

pub static RABBITMQ_CONSUME_TOTAL: Lazy<Counter> = Lazy::new(|| {
    register_counter!(
        "rabbitmq_consume_total",
        "Total number of consumers started"
    )
    .unwrap()
});

#[allow(dead_code)]
pub static RABBITMQ_HEALTHY: Lazy<Gauge> = Lazy::new(|| {
    register_gauge!(
        "rabbitmq_healthy",
        "1 if RabbitMQ connection is healthy, 0 otherwise"
    )
    .unwrap()
});

#[allow(dead_code)]
pub static RABBITMQ_QUEUE_LENGTH: Lazy<Gauge> = Lazy::new(|| {
    register_gauge!(
        "rabbitmq_queue_length",
        "Current number of messages in the ai_tasks queue"
    )
    .unwrap()
});

pub static GAMES_FINISHED_TOTAL: Lazy<CounterVec> = Lazy::new(|| {
    register_counter_vec!(
        "games_finished_total",
        "Total number of games finished",
        &["status"]
    )
    .unwrap()
});

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

pub static HTTP_REQUESTS_TOTAL: Lazy<CounterVec> = Lazy::new(|| {
    register_counter_vec!(
        "http_requests_total",
        "Total number of HTTP requests",
        &["method", "path", "status"]
    )
    .unwrap()
});

pub static HTTP_REQUEST_DURATION_SECONDS: Lazy<HistogramVec> = Lazy::new(|| {
    register_histogram_vec!(
        "http_request_duration_seconds",
        "HTTP request duration in seconds",
        &["method", "path"],
        vec![0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]
    )
    .unwrap()
});

pub static ACTIVE_GAMES: Lazy<Gauge> =
    Lazy::new(|| register_gauge!("active_games", "Current number of active games").unwrap());

pub static RATE_LIMIT_HITS_TOTAL: Lazy<Counter> = Lazy::new(|| {
    register_counter!(
        "rate_limit_hits_total",
        "Total number of rate limit rejections"
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

pub static BOT_CHAIN_FALLBACK_TOTAL: Lazy<Counter> = Lazy::new(|| {
    register_counter!(
        "bot_chain_fallback_total",
        "Total number of times the bot chain fell back to synchronous execution"
    )
    .unwrap()
});

pub static BOT_CHAIN_PUBLISH_FAILURES_TOTAL: Lazy<Counter> = Lazy::new(|| {
    register_counter!(
        "bot_chain_publish_failures_total",
        "Total number of publish failures within the bot chain"
    )
    .unwrap()
});

pub static GAMES_STALLED_TOTAL: Lazy<Counter> = Lazy::new(|| {
    register_counter!(
        "games_stalled_total",
        "Total number of games detected as stalled and recovered"
    )
    .unwrap()
});

pub static CIRCUIT_BREAKER_STATE: Lazy<Gauge> = Lazy::new(|| {
    register_gauge!(
        "circuit_breaker_state",
        "Circuit breaker state: 0 = closed, 1 = open, 2 = half-open"
    )
    .unwrap()
});

pub static DB_POOL_SIZE: Lazy<Gauge> = Lazy::new(|| {
    register_gauge!(
        "db_pool_size",
        "Total number of database connections in the pool"
    )
    .unwrap()
});

pub static DB_POOL_IDLE: Lazy<Gauge> = Lazy::new(|| {
    register_gauge!(
        "db_pool_idle",
        "Number of idle database connections in the pool"
    )
    .unwrap()
});

pub static DB_POOL_ACTIVE: Lazy<Gauge> = Lazy::new(|| {
    register_gauge!(
        "db_pool_active",
        "Number of active (in-use) database connections"
    )
    .unwrap()
});

pub fn update_db_pool_metrics(db: &sea_orm::DatabaseConnection) {
    let pool = db.get_postgres_connection_pool();
    let total = pool.size();
    let idle = pool.num_idle();
    DB_POOL_SIZE.set(total as f64);
    DB_POOL_IDLE.set(idle as f64);
    DB_POOL_ACTIVE.set(total.saturating_sub(idle as u32) as f64);
}

pub static AI_TASK_DURATION_SECONDS: Lazy<HistogramVec> = Lazy::new(|| {
    register_histogram_vec!(
        "ai_task_duration_seconds",
        "AI task processing duration in seconds",
        &["execution_method"],
        vec![0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]
    )
    .unwrap()
});

pub static AI_TASKS_IN_FLIGHT: Lazy<Gauge> = Lazy::new(|| {
    register_gauge!(
        "ai_tasks_in_flight",
        "Current number of AI tasks being processed concurrently"
    )
    .unwrap()
});

pub fn init_all() {
    RABBITMQ_PUBLISH_TOTAL.with_label_values(&["ai_tasks"]);
    RABBITMQ_PUBLISH_ERRORS_TOTAL.with_label_values(&["ai_tasks"]);
    RABBITMQ_CONSUME_TOTAL.inc_by(0.0);
    RABBITMQ_HEALTHY.set(0.0);
    RABBITMQ_QUEUE_LENGTH.set(0.0);
    GAMES_FINISHED_TOTAL.with_label_values(&["finished"]);
    GAMES_FINISHED_TOTAL.with_label_values(&["kora"]);
    GAMES_FINISHED_TOTAL.with_label_values(&["double_kora"]);
    WS_MESSAGES_SENT_TOTAL.inc_by(0.0);
    WS_CONNECTIONS_ACTIVE.set(0.0);
    HTTP_REQUESTS_TOTAL.with_label_values(&["GET", "/health", "200"]);
    HTTP_REQUEST_DURATION_SECONDS.with_label_values(&["GET", "/health"]);
    ACTIVE_GAMES.set(0.0);
    RATE_LIMIT_HITS_TOTAL.inc_by(0.0);
    WS_DISCONNECTS_TOTAL.inc_by(0.0);
    BOT_CHAIN_FALLBACK_TOTAL.inc_by(0.0);
    BOT_CHAIN_PUBLISH_FAILURES_TOTAL.inc_by(0.0);
    GAMES_STALLED_TOTAL.inc_by(0.0);
    CIRCUIT_BREAKER_STATE.set(0.0);
    DB_POOL_SIZE.set(0.0);
    DB_POOL_IDLE.set(0.0);
    DB_POOL_ACTIVE.set(0.0);
    AI_TASK_DURATION_SECONDS.with_label_values(&["ai_task"]);
    AI_TASK_DURATION_SECONDS.with_label_values(&["fallback_db"]);
    AI_TASKS_IN_FLIGHT.set(0.0);
}
