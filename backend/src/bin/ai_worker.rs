use anyhow::{Context, Result};

use jambo_backend::config::Config;

#[tokio::main]
async fn main() -> Result<()> {
    jambo_backend::observability::init_tracing("jambo-ai-worker");

    let config = Config::from_env().context("Failed to load configuration")?;

    jambo_backend::worker::run(config).await
}
