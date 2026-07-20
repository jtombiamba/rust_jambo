use crate::config::Config;
use migration::{Migrator, MigratorTrait};
use sea_orm::{ConnectOptions, Database, DatabaseConnection, DbErr};
use std::time::Duration;
use tracing::log::LevelFilter;

pub async fn create_connection(config: &Config) -> Result<DatabaseConnection, DbErr> {
    let mut opt = ConnectOptions::new(&config.database_url);
    opt.max_connections(config.db_pool_max_connections)
        .min_connections(config.db_pool_min_connections)
        .connect_timeout(Duration::from_secs(config.db_pool_connect_timeout_secs))
        .acquire_timeout(Duration::from_secs(config.db_pool_acquire_timeout_secs))
        .idle_timeout(Duration::from_secs(config.db_pool_idle_timeout_secs))
        .max_lifetime(Duration::from_secs(config.db_pool_max_lifetime_secs))
        .sqlx_logging(true)
        .sqlx_logging_level(LevelFilter::Debug);
    Database::connect(opt).await
}

#[allow(dead_code)]
pub async fn create_connection_with_pool_size(
    config: &Config,
    max_connections: u32,
) -> Result<DatabaseConnection, DbErr> {
    let mut opt = ConnectOptions::new(&config.database_url);
    opt.max_connections(max_connections)
        .min_connections(config.db_pool_min_connections.min(max_connections))
        .connect_timeout(Duration::from_secs(config.db_pool_connect_timeout_secs))
        .acquire_timeout(Duration::from_secs(config.db_pool_acquire_timeout_secs))
        .idle_timeout(Duration::from_secs(config.db_pool_idle_timeout_secs))
        .max_lifetime(Duration::from_secs(config.db_pool_max_lifetime_secs))
        .sqlx_logging(true)
        .sqlx_logging_level(LevelFilter::Debug);
    Database::connect(opt).await
}

pub async fn run_migrations(connection: &DatabaseConnection) -> Result<(), DbErr> {
    Migrator::up(connection, None).await?;
    Ok(())
}

pub mod models;
pub mod repositories;
pub mod traits;
