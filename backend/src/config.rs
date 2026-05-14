use std::env;

use config::{Config as ConfigBuilder, ConfigError, Environment};
use serde::Deserialize;

#[derive(Deserialize, Clone)]
#[allow(dead_code)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub database_url: String,
    pub rabbitmq_url: String,
    pub redis_url: Option<String>,
    pub log_level: String,
    pub max_rabbitmq_connection_retries: u32,
    pub jwt_secret: String,
    pub jwt_expiry_hours: i64,
    pub ip_hash_pepper: String,
    pub frontend_url: String,
    pub rabbitmq_publish_max_retries: u32,
    pub rabbitmq_publish_initial_retry_delay_ms: u64,
    pub rabbitmq_publish_max_retry_delay_ms: u64,
    pub circuit_breaker_failure_threshold: u32,
    pub circuit_breaker_cooldown_secs: u64,
    pub game_staleness_threshold_secs: u64,
    pub db_pool_max_connections: u32,
    pub db_pool_min_connections: u32,
    pub db_pool_connect_timeout_secs: u64,
    pub db_pool_acquire_timeout_secs: u64,
    pub db_pool_idle_timeout_secs: u64,
    pub db_pool_max_lifetime_secs: u64,
    pub db_pool_metrics_interval_secs: u64,
    pub benchmark_mode: bool,
    pub benchmark_bot_delay_ms: u64,
    pub benchmark_skip_credit_check: bool,
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("database_url", &self.database_url)
            .field("rabbitmq_url", &self.rabbitmq_url)
            .field("redis_url", &self.redis_url)
            .field("log_level", &self.log_level)
            .field(
                "max_rabbitmq_connection_retries",
                &self.max_rabbitmq_connection_retries,
            )
            .field("jwt_secret", &"***")
            .field("jwt_expiry_hours", &self.jwt_expiry_hours)
            .field("ip_hash_pepper", &"***")
            .field("frontend_url", &self.frontend_url)
            .field(
                "rabbitmq_publish_max_retries",
                &self.rabbitmq_publish_max_retries,
            )
            .field(
                "rabbitmq_publish_initial_retry_delay_ms",
                &self.rabbitmq_publish_initial_retry_delay_ms,
            )
            .field(
                "rabbitmq_publish_max_retry_delay_ms",
                &self.rabbitmq_publish_max_retry_delay_ms,
            )
            .field(
                "circuit_breaker_failure_threshold",
                &self.circuit_breaker_failure_threshold,
            )
            .field(
                "circuit_breaker_cooldown_secs",
                &self.circuit_breaker_cooldown_secs,
            )
            .field(
                "game_staleness_threshold_secs",
                &self.game_staleness_threshold_secs,
            )
            .field("db_pool_max_connections", &self.db_pool_max_connections)
            .field("db_pool_min_connections", &self.db_pool_min_connections)
            .field(
                "db_pool_connect_timeout_secs",
                &self.db_pool_connect_timeout_secs,
            )
            .field(
                "db_pool_acquire_timeout_secs",
                &self.db_pool_acquire_timeout_secs,
            )
            .field("db_pool_idle_timeout_secs", &self.db_pool_idle_timeout_secs)
            .field("db_pool_max_lifetime_secs", &self.db_pool_max_lifetime_secs)
            .field(
                "db_pool_metrics_interval_secs",
                &self.db_pool_metrics_interval_secs,
            )
            .field("benchmark_mode", &self.benchmark_mode)
            .field("benchmark_bot_delay_ms", &self.benchmark_bot_delay_ms)
            .field(
                "benchmark_skip_credit_check",
                &self.benchmark_skip_credit_check,
            )
            .finish()
    }
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        dotenv::dotenv().ok();

        let cfg = ConfigBuilder::builder()
            .set_default("benchmark_mode", "false")?
            .set_default("benchmark_bot_delay_ms", "100")?
            .set_default("benchmark_skip_credit_check", "true")?
            .add_source(Environment::default())
            .build()?;

        cfg.try_deserialize()
    }

    #[allow(clippy::should_implement_trait)]
    #[allow(dead_code)]
    pub fn default() -> Self {
        Self {
            host: env::var("HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            port: env::var("PORT")
                .unwrap_or_else(|_| "8080".to_string())
                .parse()
                .unwrap_or(8080),
            database_url: env::var("DATABASE_URL").unwrap_or_else(|_| {
                "postgres://postgres:postgres@localhost:5432/jambo".to_string()
            }),
            rabbitmq_url: env::var("RABBITMQ_URL")
                .unwrap_or_else(|_| "amqp://guest:guest@localhost:5672/%2f".to_string()),
            redis_url: env::var("REDIS_URL").ok(),
            log_level: env::var("LOG_LEVEL").unwrap_or_else(|_| "info".to_string()),
            max_rabbitmq_connection_retries: env::var("MAX_RABBITMQ_CONNECTION_RETRIES")
                .unwrap_or_else(|_| "10".to_string())
                .parse()
                .unwrap_or(10),
            jwt_secret: env::var("JWT_SECRET").unwrap_or_else(|_| {
                "super-secret-key-change-me-in-production-please-do-it-now".to_string()
            }),
            jwt_expiry_hours: env::var("JWT_EXPIRY_HOURS")
                .unwrap_or_else(|_| "24".to_string())
                .parse()
                .unwrap_or(24),
            ip_hash_pepper: env::var("IP_HASH_PEPPER")
                .unwrap_or_else(|_| "ip-pepper-change-me-1234567890abcdef".to_string()),
            frontend_url: env::var("FRONTEND_URL")
                .unwrap_or_else(|_| "http://localhost:5173".to_string()),
            rabbitmq_publish_max_retries: env::var("RABBITMQ_PUBLISH_MAX_RETRIES")
                .unwrap_or_else(|_| "3".to_string())
                .parse()
                .unwrap_or(3),
            rabbitmq_publish_initial_retry_delay_ms: env::var(
                "RABBITMQ_PUBLISH_INITIAL_RETRY_DELAY_MS",
            )
            .unwrap_or_else(|_| "100".to_string())
            .parse()
            .unwrap_or(100),
            rabbitmq_publish_max_retry_delay_ms: env::var("RABBITMQ_PUBLISH_MAX_RETRY_DELAY_MS")
                .unwrap_or_else(|_| "5000".to_string())
                .parse()
                .unwrap_or(5000),
            circuit_breaker_failure_threshold: env::var("CIRCUIT_BREAKER_FAILURE_THRESHOLD")
                .unwrap_or_else(|_| "5".to_string())
                .parse()
                .unwrap_or(5),
            circuit_breaker_cooldown_secs: env::var("CIRCUIT_BREAKER_COOLDOWN_SECS")
                .unwrap_or_else(|_| "30".to_string())
                .parse()
                .unwrap_or(30),
            game_staleness_threshold_secs: env::var("GAME_STALENESS_THRESHOLD_SECS")
                .unwrap_or_else(|_| "60".to_string())
                .parse()
                .unwrap_or(60),
            db_pool_max_connections: env::var("DB_POOL_MAX_CONNECTIONS")
                .unwrap_or_else(|_| "100".to_string())
                .parse()
                .unwrap_or(100),
            db_pool_min_connections: env::var("DB_POOL_MIN_CONNECTIONS")
                .unwrap_or_else(|_| "5".to_string())
                .parse()
                .unwrap_or(5),
            db_pool_connect_timeout_secs: env::var("DB_POOL_CONNECT_TIMEOUT_SECS")
                .unwrap_or_else(|_| "8".to_string())
                .parse()
                .unwrap_or(8),
            db_pool_acquire_timeout_secs: env::var("DB_POOL_ACQUIRE_TIMEOUT_SECS")
                .unwrap_or_else(|_| "8".to_string())
                .parse()
                .unwrap_or(8),
            db_pool_idle_timeout_secs: env::var("DB_POOL_IDLE_TIMEOUT_SECS")
                .unwrap_or_else(|_| "300".to_string())
                .parse()
                .unwrap_or(300),
            db_pool_max_lifetime_secs: env::var("DB_POOL_MAX_LIFETIME_SECS")
                .unwrap_or_else(|_| "1800".to_string())
                .parse()
                .unwrap_or(1800),
            db_pool_metrics_interval_secs: env::var("DB_POOL_METRICS_INTERVAL_SECS")
                .unwrap_or_else(|_| "30".to_string())
                .parse()
                .unwrap_or(30),
            benchmark_mode: env::var("BENCHMARK_MODE")
                .unwrap_or_else(|_| "false".to_string())
                .parse()
                .unwrap_or(false),
            benchmark_bot_delay_ms: env::var("BENCHMARK_BOT_DELAY_MS")
                .unwrap_or_else(|_| "100".to_string())
                .parse()
                .unwrap_or(100),
            benchmark_skip_credit_check: env::var("BENCHMARK_SKIP_CREDIT_CHECK")
                .unwrap_or_else(|_| "true".to_string())
                .parse()
                .unwrap_or(true),
        }
    }
}
