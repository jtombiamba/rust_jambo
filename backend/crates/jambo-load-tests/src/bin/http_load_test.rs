mod common;
#[path = "common/game_task.rs"]
mod game_task;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use clap::Parser;
use serde::Serialize;
use tracing::{error, info, warn};
use uuid::Uuid;

use common::{compute_percentile_stats, PercentileStats};

#[derive(Parser, Debug)]
#[command(name = "http-load-test")]
#[command(about = "Human-only multiplayer HTTP load test")]
struct Cli {
    #[arg(long, default_value = "http://localhost:5000")]
    target_url: String,

    #[arg(long, default_value = "50")]
    concurrent_games: usize,

    #[arg(long, default_value = "500")]
    total_games: usize,

    #[arg(long, default_value = "10")]
    bet: i32,

    #[arg(long, default_value = "200")]
    think_time_ms: u64,

    #[arg(long, default_value = "120")]
    duration_secs: u64,

    #[arg(long, default_value = "10")]
    ramp_up_secs: u64,

    #[arg(long, default_value = "10")]
    warm_up_games: usize,

    #[arg(long, default_value = "5000")]
    client_timeout_ms: u64,

    #[arg(long)]
    cleanup: bool,

    #[arg(long, default_value = "http-benchmark.json")]
    output: String,
}

#[derive(Debug, Serialize)]
struct BenchmarkReport {
    config: BenchmarkConfig,
    summary: BenchmarkSummary,
    latency: LatencyStats,
    warm_up_latency: Option<LatencyStats>,
    errors: ErrorStats,
}

#[derive(Debug, Serialize)]
struct BenchmarkConfig {
    target_url: String,
    concurrent_games: usize,
    total_games: usize,
    bet: i32,
    think_time_ms: u64,
    duration_secs: u64,
    ramp_up_secs: u64,
    warm_up_games: usize,
}

#[derive(Debug, Serialize)]
struct BenchmarkSummary {
    users_registered: usize,
    warm_up_games: u64,
    warm_up_completed: u64,
    real_games_created: u64,
    real_games_completed: u64,
    creation_failures: u64,
    total_duration_secs: f64,
    real_games_per_second: f64,
    total_card_plays: u64,
    http_errors: u64,
}

#[derive(Debug, Serialize)]
struct LatencyStats {
    user_registration: PercentileStats,
    game_creation: PercentileStats,
    card_play: PercentileStats,
    game_duration_secs: PercentileStats,
}

#[derive(Debug, Serialize)]
struct ErrorStats {
    registration: u64,
    game_creation: u64,
    card_play: u64,
}

fn build_client(timeout_ms: u64) -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_millis(timeout_ms))
        .connect_timeout(Duration::from_millis(timeout_ms))
        .cookie_store(true)
        .build()
        .expect("Failed to create HTTP client")
}

async fn register_user(
    client: &reqwest::Client,
    target_url: &str,
    index: usize,
) -> Result<game_task::UserSession> {
    let pseudo = format!("loadtest_user_{}", index);
    let email = format!("loadtest_{}@benchmark.local", index);
    let password = "benchmark123";

    let resp = client
        .post(format!("{}/api/auth/register", target_url))
        .json(&serde_json::json!({
            "pseudo": pseudo,
            "email": email,
            "password": password,
            "password_confirm": password,
        }))
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!(
            "Registration failed ({}): {}",
            status,
            body
        ));
    }

    let auth_cookie = resp
        .headers()
        .get("set-cookie")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let json: serde_json::Value = resp.json().await?;
    let user_id = json["user"]["id"]
        .as_str()
        .context("Missing user_id")?
        .parse::<Uuid>()?;

    Ok(game_task::UserSession {
        user_id,
        auth_cookie,
    })
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("http_load_test=info".parse().unwrap()),
        )
        .json()
        .init();

    let cli = Cli::parse();
    let benchmark_token = std::env::var("BENCHMARK_API_TOKEN").unwrap_or_default();
    let benchmark_mode = std::env::var("BENCHMARK_MODE")
        .unwrap_or_else(|_| "true".to_string())
        .parse()
        .unwrap_or(true);
    info!(
        "Starting HTTP load test:benchmark token: {}, benchmark mode {}",
        benchmark_token, benchmark_mode
    );
    info!(
        "Starting HTTP load test:{} concurrent, {} total, {}s, ramp-up {}s",
        cli.concurrent_games, cli.total_games, cli.duration_secs, cli.ramp_up_secs
    );

    let client = build_client(cli.client_timeout_ms);
    let target_url = cli.target_url.trim_end_matches('/').to_string();

    if cli.cleanup {
        info!("Cleanup mode: deleting all benchmark data...");
        common::run_cleanup(&client, &target_url, &benchmark_token).await?;
        return Ok(());
    }

    let game_creation_times: Arc<tokio::sync::Mutex<Vec<f64>>> = default_vec();
    let card_play_times: Arc<tokio::sync::Mutex<Vec<f64>>> = default_vec();
    let game_duration_times: Arc<tokio::sync::Mutex<Vec<f64>>> = default_vec();
    let reg_times: Arc<tokio::sync::Mutex<Vec<f64>>> = default_vec();

    let warmup_creation_times: Arc<tokio::sync::Mutex<Vec<f64>>> = default_vec();
    let warmup_card_play_times: Arc<tokio::sync::Mutex<Vec<f64>>> = default_vec();
    let warmup_duration_times: Arc<tokio::sync::Mutex<Vec<f64>>> = default_vec();

    let real_games_completed = Arc::new(AtomicU64::new(0));
    let real_games_created = Arc::new(AtomicU64::new(0));
    let warmup_games_created = Arc::new(AtomicU64::new(0));
    let warmup_games_completed = Arc::new(AtomicU64::new(0));
    let creation_failures = Arc::new(AtomicU64::new(0));
    let card_play_errors = Arc::new(AtomicU64::new(0));
    let reg_errors = Arc::new(AtomicU64::new(0));

    let total_users = cli.total_games * 4;
    let mut user_sessions = Vec::new();

    info!("Phase 1: Registering {} users...", total_users);
    let reg_start = Instant::now();
    for i in 0..total_users {
        if i > 0 && i % 100 == 0 {
            info!("Registered {}/{} users", i, total_users);
        }
        let t0 = Instant::now();
        match register_user(&client, &target_url, i).await {
            Ok(session) => {
                reg_times
                    .lock()
                    .await
                    .push(t0.elapsed().as_secs_f64() * 1000.0);
                user_sessions.push(session);
            }
            Err(e) => {
                error!("Registration error: {}", e);
                reg_errors.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
    info!(
        "Registered {} users in {:.1}s",
        user_sessions.len(),
        reg_start.elapsed().as_secs_f64()
    );

    if user_sessions.is_empty() {
        return Err(anyhow::anyhow!("No user sessions registered, aborting"));
    }
    if user_sessions.len() < total_users {
        warn!(
            "Only {}/{} users registered, some games will have fewer players",
            user_sessions.len(),
            total_users
        );
    }

    let benchmark_start = Instant::now();
    let deadline = benchmark_start + Duration::from_secs(cli.duration_secs + cli.ramp_up_secs + 60);

    let active_in_flight = Arc::new(AtomicU64::new(0));

    if cli.warm_up_games > 0 {
        info!(
            "Phase 2a: Running {} warm-up games at low concurrency...",
            cli.warm_up_games
        );
        for w in 0..cli.warm_up_games {
            let start_idx = w * 4;
            if start_idx + 4 > user_sessions.len() {
                warn!("Not enough users for warm-up game {}", w);
                break;
            }
            while active_in_flight.load(Ordering::Relaxed) >= 4 {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            let sessions: Vec<game_task::UserSession> =
                user_sessions[start_idx..start_idx + 4].to_vec();

            active_in_flight.fetch_add(1, Ordering::Relaxed);
            game_task::spawn_game_task(
                sessions,
                client.clone(),
                target_url.clone(),
                benchmark_token.clone(),
                cli.bet,
                cli.think_time_ms,
                true,
                &warmup_creation_times,
                &warmup_card_play_times,
                &warmup_duration_times,
                &warmup_games_created,
                &warmup_games_completed,
                &creation_failures,
                &card_play_errors,
                &active_in_flight,
            );
        }
        info!("Warm-up phase dispatched {} games", cli.warm_up_games);
    }

    let warmup_after_start = benchmark_start.elapsed();
    let remaining_for_deadline = if cli.ramp_up_secs > 0 {
        Duration::from_secs(cli.ramp_up_secs + cli.duration_secs).saturating_sub(warmup_after_start)
    } else {
        Duration::from_secs(cli.duration_secs).saturating_sub(warmup_after_start)
    };
    let main_deadline = Instant::now() + remaining_for_deadline;

    let target_concurrency = cli.concurrent_games as u32;

    let (limit_tx, limit_rx) = tokio::sync::watch::channel(1u32);
    if cli.ramp_up_secs > 0 {
        let ramp_dur = Duration::from_secs(cli.ramp_up_secs);
        tokio::spawn(async move {
            let start = Instant::now();
            loop {
                let frac = (start.elapsed().as_secs_f64() / ramp_dur.as_secs_f64()).min(1.0);
                let limit = 1u32
                    .max((1.0 + frac * (target_concurrency as f64 - 1.0)).round() as u32)
                    .min(target_concurrency);
                let _ = limit_tx.send(limit);
                if frac >= 1.0 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        });
    } else {
        let _ = limit_tx.send(target_concurrency);
    }

    info!("Phase 2b: Running main benchmark games...");
    let mut games_started = 0usize;
    let warmup_offset = cli.warm_up_games;

    info!(
        "Session pool: {} users, warmup_offset={}, each game needs 4 users",
        user_sessions.len(),
        warmup_offset
    );

    while games_started < cli.total_games && Instant::now() < main_deadline {
        let current_limit = *limit_rx.borrow();
        while active_in_flight.load(Ordering::Relaxed) >= current_limit as u64 {
            if Instant::now() >= main_deadline {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
            let new_limit = *limit_rx.borrow();
            if new_limit != current_limit {
                break;
            }
        }
        if Instant::now() >= main_deadline {
            break;
        }

        let session_idx = (warmup_offset + games_started) * 4;
        if session_idx + 4 > user_sessions.len() {
            warn!(
                "Not enough user sessions for game {} (need index {}, have {} users total, warmup_offset={})",
                games_started, session_idx, user_sessions.len(), warmup_offset
            );
            break;
        }
        let sessions: Vec<game_task::UserSession> =
            user_sessions[session_idx..session_idx + 4].to_vec();

        active_in_flight.fetch_add(1, Ordering::Relaxed);
        games_started += 1;

        game_task::spawn_game_task(
            sessions,
            client.clone(),
            target_url.clone(),
            benchmark_token.clone(),
            cli.bet,
            cli.think_time_ms,
            false,
            &game_creation_times,
            &card_play_times,
            &game_duration_times,
            &real_games_created,
            &real_games_completed,
            &creation_failures,
            &card_play_errors,
            &active_in_flight,
        );
    }

    info!(
        "Dispatched {} real games, waiting for completion...",
        games_started
    );
    while Instant::now() < deadline {
        let completed = real_games_completed.load(Ordering::Relaxed);
        let warmup_done = warmup_games_completed.load(Ordering::Relaxed);
        info!(
            "Progress: {}/{} real, {}/{} warm-up completed",
            completed, games_started, warmup_done, cli.warm_up_games
        );
        if completed as usize >= games_started && warmup_done as usize >= cli.warm_up_games {
            break;
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    let elapsed = benchmark_start.elapsed().as_secs_f64();

    let warmup_create_stats = warmup_creation_times.lock().await;
    let warmup_play_stats = warmup_card_play_times.lock().await;
    let warmup_dur_stats = warmup_duration_times.lock().await;
    let has_warmup_data = !warmup_create_stats.is_empty() || !warmup_play_stats.is_empty();

    let reg = reg_times.lock().await;
    let creation = game_creation_times.lock().await;
    let card_play = card_play_times.lock().await;
    let durations = game_duration_times.lock().await;

    let report = BenchmarkReport {
        config: BenchmarkConfig {
            target_url,
            concurrent_games: cli.concurrent_games,
            total_games: cli.total_games,
            bet: cli.bet,
            think_time_ms: cli.think_time_ms,
            duration_secs: cli.duration_secs,
            ramp_up_secs: cli.ramp_up_secs,
            warm_up_games: cli.warm_up_games,
        },
        summary: BenchmarkSummary {
            users_registered: user_sessions.len(),
            warm_up_games: warmup_games_created.load(Ordering::Relaxed),
            warm_up_completed: warmup_games_completed.load(Ordering::Relaxed),
            real_games_created: real_games_created.load(Ordering::Relaxed),
            real_games_completed: real_games_completed.load(Ordering::Relaxed),
            creation_failures: creation_failures.load(Ordering::Relaxed),
            total_duration_secs: elapsed,
            real_games_per_second: if elapsed > 0.0 {
                real_games_completed.load(Ordering::Relaxed) as f64 / elapsed
            } else {
                0.0
            },
            total_card_plays: card_play.len() as u64,
            http_errors: card_play_errors.load(Ordering::Relaxed),
        },
        latency: LatencyStats {
            user_registration: compute_percentile_stats(&reg),
            game_creation: compute_percentile_stats(&creation),
            card_play: compute_percentile_stats(&card_play),
            game_duration_secs: compute_percentile_stats(&durations),
        },
        warm_up_latency: if has_warmup_data {
            Some(LatencyStats {
                user_registration: PercentileStats {
                    p50: 0.0,
                    p95: 0.0,
                    p99: 0.0,
                    avg: 0.0,
                    count: 0,
                },
                game_creation: compute_percentile_stats(&warmup_create_stats),
                card_play: compute_percentile_stats(&warmup_play_stats),
                game_duration_secs: compute_percentile_stats(&warmup_dur_stats),
            })
        } else {
            None
        },
        errors: ErrorStats {
            registration: reg_errors.load(Ordering::Relaxed),
            game_creation: creation_failures.load(Ordering::Relaxed),
            card_play: card_play_errors.load(Ordering::Relaxed),
        },
    };

    let json = serde_json::to_string_pretty(&report)?;
    std::fs::write(&cli.output, &json)?;
    info!("Report written to {}", cli.output);
    println!("{}", json);
    Ok(())
}

fn default_vec() -> Arc<tokio::sync::Mutex<Vec<f64>>> {
    game_task::default_vec()
}
