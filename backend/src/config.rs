use std::env;

use config::{Config as ConfigBuilder, ConfigError, Environment};
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
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
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        dotenv::dotenv().ok();

        let cfg = ConfigBuilder::builder()
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
        }
    }
}
