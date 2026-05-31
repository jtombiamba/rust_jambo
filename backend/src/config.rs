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
    pub cors_allowed_origins: String,
    pub cors_max_age: u64,
    pub rabbitmq_publish_max_retries: u32,
    pub rabbitmq_publish_initial_retry_delay_ms: u64,
    pub rabbitmq_publish_max_retry_delay_ms: u64,
    pub circuit_breaker_failure_threshold: u32,
    pub circuit_breaker_cooldown_secs: u64,
    pub game_staleness_threshold_secs: u64,
    pub game_human_staleness_alert_secs: u64,
    pub game_human_staleness_kick_secs: u64,
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
    pub freeze_duration_secs: u64,
    pub default_credit: i32,
    pub unfreeze_credit_no_payment: i32,
    pub unfreeze_credit_with_payment: i32,
    pub paypal_client_id: String,
    pub paypal_client_secret: String,
    pub paypal_mode: String,
    pub paypal_unfreeze_amount_eur: String,
    pub paypal_topup_amount_eur: String,
    pub paypal_sandbox_url: String,
    pub paypal_live_url: String,
    pub paypal_donate_url: String,
    pub topup_credit_threshold: i32,
    pub topup_credit_amount: i32,
    pub benchmark_api_token: String,
    pub rate_limit_default_max_requests: u64,
    pub rate_limit_default_window_seconds: u64,
    pub rate_limit_contact_max_requests: u64,
    pub rate_limit_contact_window_seconds: u64,
    pub rate_limit_register_max_requests: u64,
    pub rate_limit_register_window_seconds: u64,
    pub rate_limit_login_max_requests: u64,
    pub rate_limit_login_window_seconds: u64,
    pub rate_limit_forgot_password_max_requests: u64,
    pub rate_limit_forgot_password_window_seconds: u64,
    pub rate_limit_reset_password_max_requests: u64,
    pub rate_limit_reset_password_window_seconds: u64,
    pub room_max_players: i32,
    pub run_staleness_timeout_secs: u64,
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
            .field("cors_allowed_origins", &self.cors_allowed_origins)
            .field("cors_max_age", &self.cors_max_age)
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
            .field(
                "game_human_staleness_alert_secs",
                &self.game_human_staleness_alert_secs,
            )
            .field(
                "game_human_staleness_kick_secs",
                &self.game_human_staleness_kick_secs,
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
            .field("freeze_duration_secs", &self.freeze_duration_secs)
            .field("default_credit", &self.default_credit)
            .field(
                "unfreeze_credit_no_payment",
                &self.unfreeze_credit_no_payment,
            )
            .field(
                "unfreeze_credit_with_payment",
                &self.unfreeze_credit_with_payment,
            )
            .field("paypal_client_id", &"***")
            .field("paypal_client_secret", &"***")
            .field("paypal_mode", &self.paypal_mode)
            .field(
                "paypal_unfreeze_amount_eur",
                &self.paypal_unfreeze_amount_eur,
            )
            .field("paypal_topup_amount_eur", &self.paypal_topup_amount_eur)
            .field("paypal_sandbox_url", &self.paypal_sandbox_url)
            .field("paypal_live_url", &self.paypal_live_url)
            .field("topup_credit_threshold", &self.topup_credit_threshold)
            .field("topup_credit_amount", &self.topup_credit_amount)
            .field("benchmark_api_token", &"***")
            .field(
                "rate_limit_default_max_requests",
                &self.rate_limit_default_max_requests,
            )
            .field(
                "rate_limit_default_window_seconds",
                &self.rate_limit_default_window_seconds,
            )
            .field(
                "rate_limit_contact_max_requests",
                &self.rate_limit_contact_max_requests,
            )
            .field(
                "rate_limit_contact_window_seconds",
                &self.rate_limit_contact_window_seconds,
            )
            .field(
                "rate_limit_register_max_requests",
                &self.rate_limit_register_max_requests,
            )
            .field(
                "rate_limit_register_window_seconds",
                &self.rate_limit_register_window_seconds,
            )
            .field(
                "rate_limit_login_max_requests",
                &self.rate_limit_login_max_requests,
            )
            .field(
                "rate_limit_login_window_seconds",
                &self.rate_limit_login_window_seconds,
            )
            .field(
                "rate_limit_forgot_password_max_requests",
                &self.rate_limit_forgot_password_max_requests,
            )
            .field(
                "rate_limit_forgot_password_window_seconds",
                &self.rate_limit_forgot_password_window_seconds,
            )
            .field(
                "rate_limit_reset_password_max_requests",
                &self.rate_limit_reset_password_max_requests,
            )
            .field(
                "rate_limit_reset_password_window_seconds",
                &self.rate_limit_reset_password_window_seconds,
            )
            .field("room_max_players", &self.room_max_players)
            .field(
                "run_staleness_timeout_secs",
                &self.run_staleness_timeout_secs,
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
            .set_default("freeze_duration_secs", "86400")?
            .set_default("default_credit", "500")?
            .set_default("unfreeze_credit_no_payment", "250")?
            .set_default("unfreeze_credit_with_payment", "500")?
            .set_default("paypal_client_id", "")?
            .set_default("paypal_client_secret", "")?
            .set_default("paypal_mode", "sandbox")?
            .set_default("paypal_unfreeze_amount_eur", "1.00")?
            .set_default("paypal_topup_amount_eur", "1.00")?
            .set_default("paypal_sandbox_url", "https://api-m.sandbox.paypal.com")?
            .set_default("paypal_live_url", "https://api-m.paypal.com")?
            .set_default("paypal_donate_url", "https://www.paypal.me/jtombi")?
            .set_default("topup_credit_threshold", "50")?
            .set_default("topup_credit_amount", "500")?
            .set_default("benchmark_api_token", "")?
            .set_default("cors_allowed_origins", "http://localhost:5173")?
            .set_default("cors_max_age", "3600")?
            .set_default("rate_limit_default_max_requests", "120")?
            .set_default("rate_limit_default_window_seconds", "3600")?
            .set_default("rate_limit_contact_max_requests", "5")?
            .set_default("rate_limit_contact_window_seconds", "60")?
            .set_default("rate_limit_register_max_requests", "3")?
            .set_default("rate_limit_register_window_seconds", "3600")?
            .set_default("rate_limit_login_max_requests", "10")?
            .set_default("rate_limit_login_window_seconds", "60")?
            .set_default("game_human_staleness_alert_secs", "900")?
            .set_default("game_human_staleness_kick_secs", "1800")?
            .set_default("rate_limit_forgot_password_max_requests", "3")?
            .set_default("rate_limit_forgot_password_window_seconds", "3600")?
            .set_default("rate_limit_reset_password_max_requests", "10")?
            .set_default("rate_limit_reset_password_window_seconds", "60")?
            .set_default("room_max_players", "4")?
            .set_default("run_staleness_timeout_secs", "1800")?
            .add_source(Environment::default())
            .build()?;

        cfg.try_deserialize()
    }

    pub fn cors_middleware(&self) -> actix_cors::Cors {
        use actix_cors::Cors;

        let mut cors = Cors::default()
            .allow_any_method()
            .allow_any_header()
            .supports_credentials()
            .max_age(self.cors_max_age as usize);

        let origins: Vec<&str> = self
            .cors_allowed_origins
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();

        if origins.contains(&"*") {
            cors = cors.allow_any_origin();
        } else {
            for origin in origins {
                cors = cors.allowed_origin(origin);
            }
        }

        cors
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
            game_human_staleness_alert_secs: env::var("GAME_HUMAN_STALENESS_ALERT_SECS")
                .unwrap_or_else(|_| "900".to_string())
                .parse()
                .unwrap_or(900),
            game_human_staleness_kick_secs: env::var("GAME_HUMAN_STALENESS_KICK_SECS")
                .unwrap_or_else(|_| "1800".to_string())
                .parse()
                .unwrap_or(1800),
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
            freeze_duration_secs: env::var("FREEZE_DURATION_SECS")
                .unwrap_or_else(|_| "86400".to_string())
                .parse()
                .unwrap_or(86400),
            default_credit: env::var("DEFAULT_CREDIT")
                .unwrap_or_else(|_| "500".to_string())
                .parse()
                .unwrap_or(500),
            unfreeze_credit_no_payment: env::var("UNFREEZE_CREDIT_NO_PAYMENT")
                .unwrap_or_else(|_| "250".to_string())
                .parse()
                .unwrap_or(250),
            unfreeze_credit_with_payment: env::var("UNFREEZE_CREDIT_WITH_PAYMENT")
                .unwrap_or_else(|_| "500".to_string())
                .parse()
                .unwrap_or(500),
            paypal_client_id: env::var("PAYPAL_CLIENT_ID").unwrap_or_default(),
            paypal_client_secret: env::var("PAYPAL_CLIENT_SECRET").unwrap_or_default(),
            paypal_mode: env::var("PAYPAL_MODE").unwrap_or_else(|_| "sandbox".to_string()),
            paypal_unfreeze_amount_eur: env::var("PAYPAL_UNFREEZE_AMOUNT_EUR")
                .unwrap_or_else(|_| "1.00".to_string()),
            paypal_topup_amount_eur: env::var("PAYPAL_TOPUP_AMOUNT_EUR")
                .unwrap_or_else(|_| "1.00".to_string()),
            paypal_sandbox_url: env::var("PAYPAL_SANDBOX_URL")
                .unwrap_or_else(|_| "https://api-m.sandbox.paypal.com".to_string()),
            paypal_live_url: env::var("PAYPAL_LIVE_URL")
                .unwrap_or_else(|_| "https://api-m.paypal.com".to_string()),
            paypal_donate_url: env::var("PAYPAL_DONATE_URL")
                .unwrap_or_else(|_| "https://www.paypal.me/jtombi".to_string()),
            topup_credit_threshold: env::var("TOPUP_CREDIT_THRESHOLD")
                .unwrap_or_else(|_| "50".to_string())
                .parse()
                .unwrap_or(50),
            topup_credit_amount: env::var("TOPUP_CREDIT_AMOUNT")
                .unwrap_or_else(|_| "500".to_string())
                .parse()
                .unwrap_or(500),
            benchmark_api_token: env::var("BENCHMARK_API_TOKEN").unwrap_or_default(),
            cors_allowed_origins: env::var("CORS_ALLOWED_ORIGINS")
                .unwrap_or_else(|_| "http://localhost:5173".to_string()),
            cors_max_age: env::var("CORS_MAX_AGE")
                .unwrap_or_else(|_| "3600".to_string())
                .parse()
                .unwrap_or(3600),
            rate_limit_default_max_requests: env::var("RATE_LIMIT_DEFAULT_MAX_REQUESTS")
                .unwrap_or_else(|_| "120".to_string())
                .parse()
                .unwrap_or(120),
            rate_limit_default_window_seconds: env::var("RATE_LIMIT_DEFAULT_WINDOW_SECONDS")
                .unwrap_or_else(|_| "3600".to_string())
                .parse()
                .unwrap_or(3600),
            rate_limit_contact_max_requests: env::var("RATE_LIMIT_CONTACT_MAX_REQUESTS")
                .unwrap_or_else(|_| "5".to_string())
                .parse()
                .unwrap_or(5),
            rate_limit_contact_window_seconds: env::var("RATE_LIMIT_CONTACT_WINDOW_SECONDS")
                .unwrap_or_else(|_| "60".to_string())
                .parse()
                .unwrap_or(60),
            rate_limit_register_max_requests: env::var("RATE_LIMIT_REGISTER_MAX_REQUESTS")
                .unwrap_or_else(|_| "3".to_string())
                .parse()
                .unwrap_or(3),
            rate_limit_register_window_seconds: env::var("RATE_LIMIT_REGISTER_WINDOW_SECONDS")
                .unwrap_or_else(|_| "3600".to_string())
                .parse()
                .unwrap_or(3600),
            rate_limit_login_max_requests: env::var("RATE_LIMIT_LOGIN_MAX_REQUESTS")
                .unwrap_or_else(|_| "10".to_string())
                .parse()
                .unwrap_or(10),
            rate_limit_login_window_seconds: env::var("RATE_LIMIT_LOGIN_WINDOW_SECONDS")
                .unwrap_or_else(|_| "60".to_string())
                .parse()
                .unwrap_or(60),
            rate_limit_forgot_password_max_requests: env::var(
                "RATE_LIMIT_FORGOT_PASSWORD_MAX_REQUESTS",
            )
            .unwrap_or_else(|_| "3".to_string())
            .parse()
            .unwrap_or(3),
            rate_limit_forgot_password_window_seconds: env::var(
                "RATE_LIMIT_FORGOT_PASSWORD_WINDOW_SECONDS",
            )
            .unwrap_or_else(|_| "3600".to_string())
            .parse()
            .unwrap_or(3600),
            rate_limit_reset_password_max_requests: env::var(
                "RATE_LIMIT_RESET_PASSWORD_MAX_REQUESTS",
            )
            .unwrap_or_else(|_| "10".to_string())
            .parse()
            .unwrap_or(10),
            rate_limit_reset_password_window_seconds: env::var(
                "RATE_LIMIT_RESET_PASSWORD_WINDOW_SECONDS",
            )
            .unwrap_or_else(|_| "60".to_string())
            .parse()
            .unwrap_or(60),
            room_max_players: env::var("ROOM_MAX_PLAYERS")
                .unwrap_or_else(|_| "4".to_string())
                .parse()
                .unwrap_or(4),
            run_staleness_timeout_secs: env::var("RUN_STALENESS_TIMEOUT_SECS")
                .unwrap_or_else(|_| "1800".to_string())
                .parse()
                .unwrap_or(1800),
        }
    }
}
