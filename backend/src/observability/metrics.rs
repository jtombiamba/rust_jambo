use once_cell::sync::Lazy;
use prometheus::{
    register_counter, register_counter_vec, register_gauge, register_gauge_vec,
    register_histogram_vec, Counter, CounterVec, Gauge, GaugeVec, HistogramVec,
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

pub static DB_POOL_SIZE: Lazy<GaugeVec> = Lazy::new(|| {
    register_gauge_vec!(
        "db_pool_size",
        "Total number of database connections in the pool",
        &["process"]
    )
    .unwrap()
});

pub static DB_POOL_IDLE: Lazy<GaugeVec> = Lazy::new(|| {
    register_gauge_vec!(
        "db_pool_idle",
        "Number of idle database connections in the pool",
        &["process"]
    )
    .unwrap()
});

pub static DB_POOL_ACTIVE: Lazy<GaugeVec> = Lazy::new(|| {
    register_gauge_vec!(
        "db_pool_active",
        "Number of active (in-use) database connections",
        &["process"]
    )
    .unwrap()
});

#[allow(dead_code)]
pub fn update_db_pool_metrics(db: &sea_orm::DatabaseConnection, process: &str) {
    let pool = db.get_postgres_connection_pool();
    let total = pool.size();
    let idle = pool.num_idle();
    DB_POOL_SIZE.with_label_values(&[process]).set(total as f64);
    DB_POOL_IDLE.with_label_values(&[process]).set(idle as f64);
    DB_POOL_ACTIVE
        .with_label_values(&[process])
        .set(total.saturating_sub(idle as u32) as f64);
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

pub static GAME_CREATION_DURATION_SECONDS: Lazy<HistogramVec> = Lazy::new(|| {
    register_histogram_vec!(
        "game_creation_duration_seconds",
        "Duration of game creation in seconds",
        &["game_mode"],
        vec![0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0]
    )
    .unwrap()
});

pub static GAME_DURATION_SECONDS: Lazy<HistogramVec> = Lazy::new(|| {
    register_histogram_vec!(
        "game_duration_seconds",
        "Total game duration in seconds",
        &["game_mode"],
        vec![1.0, 2.5, 5.0, 10.0, 25.0, 50.0, 100.0, 250.0, 500.0]
    )
    .unwrap()
});

pub static CARD_PLAY_DURATION_SECONDS: Lazy<HistogramVec> = Lazy::new(|| {
    register_histogram_vec!(
        "card_play_duration_seconds",
        "Duration of card play operations in seconds",
        &["operation"],
        vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0]
    )
    .unwrap()
});

pub static ROUND_EVAL_DURATION_SECONDS: Lazy<HistogramVec> = Lazy::new(|| {
    register_histogram_vec!(
        "round_eval_duration_seconds",
        "Duration of round evaluation in seconds",
        &[],
        vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0]
    )
    .unwrap()
});

pub static DB_QUERY_DURATION_SECONDS: Lazy<HistogramVec> = Lazy::new(|| {
    register_histogram_vec!(
        "db_query_duration_seconds",
        "Duration of database queries in seconds",
        &["query_type"],
        vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0]
    )
    .unwrap()
});

pub static DB_TRANSACTION_DURATION_SECONDS: Lazy<HistogramVec> = Lazy::new(|| {
    register_histogram_vec!(
        "db_transaction_duration_seconds",
        "Duration of database transactions in seconds",
        &["operation"],
        vec![0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0]
    )
    .unwrap()
});

pub static REDIS_PUBLISH_DURATION_SECONDS: Lazy<HistogramVec> = Lazy::new(|| {
    register_histogram_vec!(
        "redis_publish_duration_seconds",
        "Duration of Redis publish operations in seconds",
        &[],
        vec![0.0001, 0.0005, 0.001, 0.005, 0.01, 0.05, 0.1]
    )
    .unwrap()
});

pub static REDIS_CACHE_HIT_RATIO: Lazy<Gauge> = Lazy::new(|| {
    register_gauge!(
        "redis_cache_hit_ratio",
        "Ratio of Redis cache hits to total lookups"
    )
    .unwrap()
});

pub static BOT_MOVE_DURATION_SECONDS: Lazy<HistogramVec> = Lazy::new(|| {
    register_histogram_vec!(
        "bot_move_duration_seconds",
        "Duration of bot move execution in seconds",
        &["execution_method"],
        vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5]
    )
    .unwrap()
});

pub static BOT_ERRORS_TOTAL: Lazy<CounterVec> = Lazy::new(|| {
    register_counter_vec!(
        "bot_errors_total",
        "Total number of bot execution errors",
        &["error_type"]
    )
    .unwrap()
});

pub static SCHEDULER_TASK_DURATION: Lazy<HistogramVec> = Lazy::new(|| {
    register_histogram_vec!(
        "scheduler_task_duration_seconds",
        "Duration of scheduler background tasks in seconds",
        &["task"],
        vec![0.1, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0]
    )
    .unwrap()
});

pub static SCHEDULER_TASK_TIMEOUTS: Lazy<CounterVec> = Lazy::new(|| {
    register_counter_vec!(
        "scheduler_task_timeouts_total",
        "Total number of scheduler task timeouts",
        &["task"]
    )
    .unwrap()
});

pub static SCHEDULER_TASK_ERRORS: Lazy<CounterVec> = Lazy::new(|| {
    register_counter_vec!(
        "scheduler_task_errors_total",
        "Total number of scheduler task errors",
        &["task"]
    )
    .unwrap()
});

pub static SCHEDULER_LAST_RUN: Lazy<GaugeVec> = Lazy::new(|| {
    register_gauge_vec!(
        "scheduler_last_run_timestamp_seconds",
        "Unix timestamp of the last successful scheduler task run",
        &["task"]
    )
    .unwrap()
});

pub static MEMORY_USAGE_BYTES: Lazy<GaugeVec> = Lazy::new(|| {
    register_gauge_vec!("memory_usage_bytes", "Memory usage in bytes", &["process"]).unwrap()
});

pub static CPU_USAGE_PERCENT: Lazy<GaugeVec> = Lazy::new(|| {
    register_gauge_vec!("cpu_usage_percent", "CPU usage percentage", &["process"]).unwrap()
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
    DB_POOL_SIZE.with_label_values(&["backend"]).set(0.0);
    DB_POOL_SIZE.with_label_values(&["ai_worker"]).set(0.0);
    DB_POOL_SIZE
        .with_label_values(&["scheduler_worker"])
        .set(0.0);
    DB_POOL_IDLE.with_label_values(&["backend"]).set(0.0);
    DB_POOL_IDLE.with_label_values(&["ai_worker"]).set(0.0);
    DB_POOL_IDLE
        .with_label_values(&["scheduler_worker"])
        .set(0.0);
    DB_POOL_ACTIVE.with_label_values(&["backend"]).set(0.0);
    DB_POOL_ACTIVE.with_label_values(&["ai_worker"]).set(0.0);
    DB_POOL_ACTIVE
        .with_label_values(&["scheduler_worker"])
        .set(0.0);
    AI_TASK_DURATION_SECONDS.with_label_values(&["ai_task"]);
    AI_TASK_DURATION_SECONDS.with_label_values(&["fallback_db"]);
    AI_TASKS_IN_FLIGHT.set(0.0);
    GAME_CREATION_DURATION_SECONDS.with_label_values(&["quick"]);
    GAME_CREATION_DURATION_SECONDS.with_label_values(&["bot_only"]);
    GAME_DURATION_SECONDS.with_label_values(&["quick"]);
    GAME_DURATION_SECONDS.with_label_values(&["bot_only"]);
    CARD_PLAY_DURATION_SECONDS.with_label_values(&["update_card_play"]);
    ROUND_EVAL_DURATION_SECONDS.with_label_values(&[]);
    DB_QUERY_DURATION_SECONDS.with_label_values(&["generic"]);
    DB_TRANSACTION_DURATION_SECONDS.with_label_values(&["generic"]);
    REDIS_PUBLISH_DURATION_SECONDS.with_label_values(&[]);
    REDIS_CACHE_HIT_RATIO.set(0.0);
    BOT_MOVE_DURATION_SECONDS.with_label_values(&["sync_chain"]);
    BOT_MOVE_DURATION_SECONDS.with_label_values(&["ai_task"]);
    BOT_ERRORS_TOTAL.with_label_values(&["strategy"]);
    BOT_ERRORS_TOTAL.with_label_values(&["execution"]);
    SCHEDULER_TASK_DURATION.with_label_values(&["cancel_expired_games"]);
    SCHEDULER_TASK_DURATION.with_label_values(&["detect_stalled_games"]);
    SCHEDULER_TASK_DURATION.with_label_values(&["check_human_staleness"]);
    SCHEDULER_TASK_DURATION.with_label_values(&["check_expired_freezes"]);
    SCHEDULER_TASK_DURATION.with_label_values(&["refresh_leaderboard"]);
    SCHEDULER_TASK_DURATION.with_label_values(&["check_stalled_runs"]);
    SCHEDULER_TASK_DURATION.with_label_values(&["db_pool_metrics"]);
    SCHEDULER_TASK_TIMEOUTS.with_label_values(&["cancel_expired_games"]);
    SCHEDULER_TASK_TIMEOUTS.with_label_values(&["detect_stalled_games"]);
    SCHEDULER_TASK_TIMEOUTS.with_label_values(&["check_human_staleness"]);
    SCHEDULER_TASK_TIMEOUTS.with_label_values(&["check_expired_freezes"]);
    SCHEDULER_TASK_TIMEOUTS.with_label_values(&["refresh_leaderboard"]);
    SCHEDULER_TASK_TIMEOUTS.with_label_values(&["check_stalled_runs"]);
    SCHEDULER_TASK_TIMEOUTS.with_label_values(&["db_pool_metrics"]);
    SCHEDULER_TASK_ERRORS.with_label_values(&["cancel_expired_games"]);
    SCHEDULER_TASK_ERRORS.with_label_values(&["detect_stalled_games"]);
    SCHEDULER_TASK_ERRORS.with_label_values(&["check_human_staleness"]);
    SCHEDULER_TASK_ERRORS.with_label_values(&["check_expired_freezes"]);
    SCHEDULER_TASK_ERRORS.with_label_values(&["refresh_leaderboard"]);
    SCHEDULER_TASK_ERRORS.with_label_values(&["check_stalled_runs"]);
    SCHEDULER_TASK_ERRORS.with_label_values(&["db_pool_metrics"]);
    SCHEDULER_LAST_RUN.with_label_values(&["cancel_expired_games"]);
    SCHEDULER_LAST_RUN.with_label_values(&["detect_stalled_games"]);
    SCHEDULER_LAST_RUN.with_label_values(&["check_human_staleness"]);
    SCHEDULER_LAST_RUN.with_label_values(&["check_expired_freezes"]);
    SCHEDULER_LAST_RUN.with_label_values(&["refresh_leaderboard"]);
    SCHEDULER_LAST_RUN.with_label_values(&["check_stalled_runs"]);
    SCHEDULER_LAST_RUN.with_label_values(&["db_pool_metrics"]);
    MEMORY_USAGE_BYTES.with_label_values(&["backend"]);
    MEMORY_USAGE_BYTES.with_label_values(&["ai_worker"]);
    MEMORY_USAGE_BYTES.with_label_values(&["scheduler_worker"]);
    CPU_USAGE_PERCENT.with_label_values(&["backend"]);
    CPU_USAGE_PERCENT.with_label_values(&["ai_worker"]);
    CPU_USAGE_PERCENT.with_label_values(&["scheduler_worker"]);
}
