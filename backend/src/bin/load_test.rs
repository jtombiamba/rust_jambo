use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use clap::Parser;
use futures::stream::{self, StreamExt};
use sea_orm::EntityTrait;
use serde::Serialize;
use sysinfo::{Pid, ProcessesToUpdate, System};
use tokio::sync::Semaphore;
use tracing::{error, info, warn};
use uuid::Uuid;

use jambo_backend::config::Config;
use jambo_backend::database;
use jambo_backend::database::models::{game, GameStatus};
use jambo_backend::game::orchestrator::{GameService, GameServiceTrait};
use jambo_backend::messaging::{self, RedisClient};
use jambo_backend::observability::{metrics, metrics_init};

#[derive(Parser, Debug)]
#[command(name = "load-test")]
#[command(about = "Bot-only solo game load test and benchmark")]
struct Cli {
    #[arg(long, default_value = "100")]
    games: usize,

    #[arg(long, default_value = "60")]
    duration: u64,

    #[arg(long, default_value = "10")]
    concurrency: usize,

    #[arg(long, default_value = "100")]
    bot_delay: u64,

    #[arg(long, default_value = "benchmark-results.json")]
    output: String,
}

#[derive(Debug, Clone)]
struct GameRecord {
    game_id: Uuid,
    created_at: Instant,
    finished_at: Option<Instant>,
    status: String,
}

#[derive(Debug, Serialize)]
struct BenchmarkReport {
    config: BenchmarkConfig,
    summary: BenchmarkSummary,
    timing: TimingStats,
    database: DatabaseStats,
    errors: ErrorStats,
    memory: MemoryStats,
    cpu: CpuStats,
}

#[derive(Debug, Serialize)]
struct BenchmarkConfig {
    games: usize,
    duration_secs: u64,
    concurrency: usize,
    bot_delay_ms: u64,
}

#[derive(Debug, Serialize)]
struct BenchmarkSummary {
    games_created: usize,
    games_completed: usize,
    completion_rate: f64,
    total_duration_secs: f64,
    games_per_second: f64,
}

#[derive(Debug, Serialize)]
struct TimingStats {
    game_creation_p50_ms: f64,
    game_creation_p95_ms: f64,
    game_creation_p99_ms: f64,
    game_duration_p50_secs: f64,
    game_duration_p95_secs: f64,
    game_duration_p99_secs: f64,
    avg_creation_time_ms: f64,
}

#[derive(Debug, Serialize)]
struct DatabaseStats {
    peak_active_connections: u32,
    avg_active_connections: f64,
    max_pool_size: u32,
}

#[derive(Debug, Serialize)]
struct ErrorStats {
    game_creation_errors: u64,
    bot_execution_errors: u64,
}

#[derive(Debug, Serialize)]
struct MemoryStats {
    backend_rss_mb: f64,
    peak_backend_rss_mb: f64,
}

#[derive(Debug, Serialize)]
struct CpuStats {
    backend_avg_percent: f64,
}

fn get_process_metrics(pid: u32, system: &System) -> (f64, f64) {
    let mut rss = 0.0;
    let mut cpu = 0.0;
    if let Some(process) = system.process(Pid::from(pid as usize)) {
        rss = process.memory() as f64 / (1024.0 * 1024.0);
        cpu = process.cpu_usage() as f64;
    }
    (rss, cpu)
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = (p / 100.0 * (sorted.len() - 1) as f64).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("load_test=info".parse().unwrap())
                .add_directive("jambo_backend=warn".parse().unwrap()),
        )
        .json()
        .init();

    metrics_init::init_all();

    let cli = Cli::parse();
    info!(
        "Starting load test: {} games, {} concurrency, {} bot_delay_ms, {}s duration",
        cli.games, cli.concurrency, cli.bot_delay, cli.duration
    );

    // Apply CLI bot_delay as the BOT_THINKING_DELAY_MS so it actually controls bot speed
    std::env::set_var("BOT_THINKING_DELAY_MS", cli.bot_delay.to_string());

    let config = Config::default();
    info!("Using database: {}", config.database_url);

    let db = database::create_connection(&config)
        .await
        .context("Failed to create database connection")?;

    database::run_migrations(&db)
        .await
        .context("Failed to run migrations")?;

    let redis_client = match &config.redis_url {
        Some(url) => match RedisClient::new(url).await {
            Ok(client) => Some(client),
            Err(e) => {
                warn!("Failed to connect to Redis: {e}, proceeding without");
                None
            }
        },
        None => None,
    };

    let publish_config = messaging::RabbitMQPublishConfig {
        max_retries: config.rabbitmq_publish_max_retries,
        initial_retry_delay_ms: config.rabbitmq_publish_initial_retry_delay_ms,
        max_retry_delay_ms: config.rabbitmq_publish_max_retry_delay_ms,
        circuit_breaker_failure_threshold: config.circuit_breaker_failure_threshold,
        circuit_breaker_cooldown_secs: config.circuit_breaker_cooldown_secs,
    };

    let rabbitmq_client = match messaging::connect_to_rabbitmq_with_retry(
        &config.rabbitmq_url,
        config.max_rabbitmq_connection_retries,
        publish_config,
    )
    .await
    {
        Ok(client) => {
            info!("Connected to RabbitMQ for bot task dispatch");
            Some(client)
        }
        Err(e) => {
            warn!("Failed to connect to RabbitMQ: {e}, using sync bot chain");
            None
        }
    };

    let config = jambo_backend::Config::default();
    let mailer_config = jambo_backend::mailer::MailerConfig::from_env();
    let mailer =
        jambo_backend::mailer::create_mailer(mailer_config).expect("Failed to create mailer");

    let orchestrator: Arc<dyn GameServiceTrait> = Arc::new(GameService::new(
        db.clone(),
        redis_client.clone(),
        rabbitmq_client.clone(),
        config.clone(),
        mailer,
    ));

    let games_created = Arc::new(AtomicU64::new(0));
    let games_with_error = Arc::new(AtomicU64::new(0));
    let creation_errors = Arc::new(AtomicU64::new(0));

    let records = Arc::new(tokio::sync::Mutex::new(Vec::<GameRecord>::new()));
    let creation_times = Arc::new(tokio::sync::Mutex::new(Vec::<f64>::new()));

    let mut creation_handles = Vec::new();
    let concurrency_limit = Arc::new(Semaphore::new(cli.concurrency));
    let start_time = Instant::now();
    let deadline = start_time + Duration::from_secs(cli.duration);

    let games_to_create = cli.games;
    let mut created_count = 0usize;

    info!(
        "Starting game creation loop ({} total, {} concurrent)",
        games_to_create, cli.concurrency
    );

    while created_count < games_to_create && Instant::now() < deadline {
        let permit = concurrency_limit.clone().acquire_owned().await?;
        let orch = orchestrator.clone();
        let records_clone = records.clone();
        let times_clone = creation_times.clone();
        let created = games_created.clone();
        let errors = creation_errors.clone();

        let handle = tokio::spawn(async move {
            let _permit = permit;
            let create_start = Instant::now();
            match orch.create_bot_only_game().await {
                Ok(outcome) => {
                    let creation_ms = create_start.elapsed().as_secs_f64() * 1000.0;
                    times_clone.lock().await.push(creation_ms);
                    records_clone.lock().await.push(GameRecord {
                        game_id: outcome.game_id,
                        created_at: Instant::now(),
                        finished_at: None,
                        status: "active".to_string(),
                    });
                    created.fetch_add(1, Ordering::Relaxed);
                    info!("Game {} created in {:.1}ms", outcome.game_id, creation_ms);
                }
                Err(e) => {
                    errors.fetch_add(1, Ordering::Relaxed);
                    error!("Failed to create game: {e}");
                }
            }
        });
        creation_handles.push(handle);
        created_count += 1;
    }

    info!(
        "All {} games dispatched, waiting for creation...",
        created_count
    );
    stream::iter(creation_handles)
        .buffer_unordered(cli.concurrency)
        .collect::<Vec<_>>()
        .await;

    let total_created = games_created.load(Ordering::Relaxed) as usize;
    info!(
        "{} games created out of {} requested",
        total_created, games_to_create
    );

    let sleep_secs = Duration::from_secs(cli.duration.min(60));
    info!("Letting games run for at least {:?}...", sleep_secs);
    tokio::time::sleep(Duration::from_secs(10)).await;

    let mut sys = System::new_all();
    let pid = std::process::id();
    let mut peak_rss_mb: f64 = 0.0;
    let mut cpu_samples: Vec<f64> = Vec::new();
    let mut peak_db_active: u32 = 0;
    let mut db_active_samples: Vec<u32> = Vec::new();

    let poll_interval = Duration::from_secs(2);
    let poll_end = start_time + Duration::from_secs(cli.duration + 60);

    info!("Starting metrics collection...");
    while Instant::now() < poll_end {
        tokio::time::sleep(poll_interval).await;

        sys.refresh_processes(ProcessesToUpdate::Some(&[Pid::from(pid as usize)]));
        let (rss_mb, cpu_pct) = get_process_metrics(pid, &sys);
        if rss_mb > peak_rss_mb {
            peak_rss_mb = rss_mb;
        }
        cpu_samples.push(cpu_pct);

        {
            let mut recs = records.lock().await;
            let db_for_poll = db.clone();
            for record in recs.iter_mut().filter(|r| r.finished_at.is_none()) {
                if let Ok(Some(g)) = game::Entity::find_by_id(record.game_id)
                    .one(&db_for_poll)
                    .await
                {
                    if g.status == GameStatus::Finished
                        || g.status == GameStatus::Kora
                        || g.status == GameStatus::DoubleKora
                    {
                        record.finished_at = Some(Instant::now());
                        record.status = format!("{:?}", g.status);
                    }
                }
            }
        }

        metrics::update_db_pool_metrics(&db, "load_test");
        let active = metrics::DB_POOL_ACTIVE
            .with_label_values(&["load_test"])
            .get() as u32;
        if active > peak_db_active {
            peak_db_active = active;
        }
        db_active_samples.push(active);

        let done_count = {
            let recs = records.lock().await;
            recs.iter().filter(|r| r.finished_at.is_some()).count()
        };

        info!(
            "Progress: {}/{} games completed, RSS {:.1}MB (peak {:.1}MB), CPU {:.1}%, DB active {}",
            done_count, total_created, rss_mb, peak_rss_mb, cpu_pct, active
        );
    }

    let final_records = records.lock().await;
    let creation_times = creation_times.lock().await;

    let completed_games: Vec<&GameRecord> = final_records
        .iter()
        .filter(|r| r.finished_at.is_some())
        .collect();
    let completed_count = completed_games.len();

    let mut game_durations: Vec<f64> = Vec::new();
    let mut sorted_creation: Vec<f64> = creation_times.clone();
    sorted_creation.sort_by(|a, b| a.partial_cmp(b).unwrap());

    for record in &completed_games {
        if let (Some(finish), created) = (record.finished_at, record.created_at) {
            game_durations.push(finish.duration_since(created).as_secs_f64());
        }
    }
    game_durations.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let total_elapsed = start_time.elapsed().as_secs_f64();
    let completion_rate = if total_created > 0 {
        completed_count as f64 / total_created as f64
    } else {
        0.0
    };
    let games_per_sec = if total_elapsed > 0.0 {
        completed_count as f64 / total_elapsed
    } else {
        0.0
    };

    let avg_db_active = if !db_active_samples.is_empty() {
        db_active_samples.iter().sum::<u32>() as f64 / db_active_samples.len() as f64
    } else {
        0.0
    };

    let avg_cpu = if !cpu_samples.is_empty() {
        cpu_samples.iter().sum::<f64>() / cpu_samples.len() as f64
    } else {
        0.0
    };

    let avg_creation_ms = if !sorted_creation.is_empty() {
        sorted_creation.iter().sum::<f64>() / sorted_creation.len() as f64
    } else {
        0.0
    };

    let report = BenchmarkReport {
        config: BenchmarkConfig {
            games: cli.games,
            duration_secs: cli.duration,
            concurrency: cli.concurrency,
            bot_delay_ms: cli.bot_delay,
        },
        summary: BenchmarkSummary {
            games_created: total_created,
            games_completed: completed_count,
            completion_rate,
            total_duration_secs: total_elapsed,
            games_per_second: games_per_sec,
        },
        timing: TimingStats {
            game_creation_p50_ms: percentile(&sorted_creation, 50.0),
            game_creation_p95_ms: percentile(&sorted_creation, 95.0),
            game_creation_p99_ms: percentile(&sorted_creation, 99.0),
            game_duration_p50_secs: percentile(&game_durations, 50.0),
            game_duration_p95_secs: percentile(&game_durations, 95.0),
            game_duration_p99_secs: percentile(&game_durations, 99.0),
            avg_creation_time_ms: avg_creation_ms,
        },
        database: DatabaseStats {
            peak_active_connections: peak_db_active,
            avg_active_connections: avg_db_active,
            max_pool_size: config.db_pool_max_connections,
        },
        errors: ErrorStats {
            game_creation_errors: creation_errors.load(Ordering::Relaxed),
            bot_execution_errors: games_with_error.load(Ordering::Relaxed),
        },
        memory: MemoryStats {
            backend_rss_mb: {
                sys.refresh_processes(ProcessesToUpdate::Some(&[Pid::from(pid as usize)]));
                get_process_metrics(pid, &sys).0
            },
            peak_backend_rss_mb: peak_rss_mb,
        },
        cpu: CpuStats {
            backend_avg_percent: avg_cpu,
        },
    };

    let json = serde_json::to_string_pretty(&report)?;
    std::fs::write(&cli.output, &json)?;
    println!("Benchmark report written to {}", cli.output);
    println!("{}", json);

    Ok(())
}
