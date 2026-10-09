use anyhow::{Context, Result};

use jambo_core::config::Config;

#[tokio::main]
async fn main() -> Result<()> {
    jambo_core::observability::init_tracing("jambo-ai-worker");

    let config = Config::from_env().context("Failed to load configuration")?;

    jambo_api::worker::run(config).await
}
