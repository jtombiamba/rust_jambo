use migration::{Migrator, MigratorTrait};
use sea_orm::{ConnectOptions, Database, DatabaseConnection, DbErr};
use std::time::Duration;

pub async fn create_connection(database_url: &str) -> Result<DatabaseConnection, DbErr> {
    let mut opt = ConnectOptions::new(database_url);
    opt.max_connections(100) // to be defined in config
        .min_connections(5) // to be defined in config
        .connect_timeout(Duration::from_secs(8)) // to be defined in config
        .acquire_timeout(Duration::from_secs(8)) // to be defined in config
        .idle_timeout(Duration::from_secs(8)) // to be defined in config
        .max_lifetime(Duration::from_secs(8)) // to be defined in config
        .sqlx_logging(false); // disable SQLx logging
                              // .sqlx_logging_level(log::LevelFilter::Info);
                              //.set_schema_search_path("my_schema"); // set default Postgres schema
    Database::connect(opt).await
}

pub async fn run_migrations(connection: &DatabaseConnection) -> Result<(), DbErr> {
    // Ensure migrations table exists
    Migrator::up(connection, None).await?;
    Ok(())
}

pub mod models;
pub mod repositories;
pub mod traits;
