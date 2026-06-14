mod common;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use clap::Parser;
use futures_util::StreamExt;
use serde::Serialize;
use tokio::sync::Semaphore;
use tracing::{error, info, warn};
use uuid::Uuid;

use common::{compute_percentile_stats, PercentileStats};

#[derive(Parser, Debug)]
#[command(name = "ws-load-test")]
#[command(about = "WebSocket load test for real-time game events")]
struct Cli {
    #[arg(long, default_value = "http://localhost:5000")]
    target_url: String,

    #[arg(long, default_value = "50")]
    concurrent_games: usize,

    #[arg(long, default_value = "100")]
    total_games: usize,

    #[arg(long, default_value = "10")]
    bet: i32,

    #[arg(long, default_value = "120")]
    duration_secs: u64,

    #[arg(long, default_value = "5000")]
    client_timeout_ms: u64,

    #[arg(long)]
    cleanup: bool,

    #[arg(long, default_value = "ws-benchmark.json")]
    output: String,
}

#[derive(Debug, Serialize)]
struct WsBenchmarkReport {
    config: WsBenchmarkConfig,
    summary: WsBenchmarkSummary,
    ws_metrics: WsMetrics,
    errors: WsErrorStats,
}

#[derive(Debug, Serialize)]
struct WsBenchmarkConfig {
    target_url: String,
    concurrent_games: usize,
    total_games: usize,
    players_per_game: usize,
    bet: i32,
    duration_secs: u64,
}

#[derive(Debug, Serialize)]
struct WsBenchmarkSummary {
    games_created: u64,
    ws_connections_attempted: u64,
    ws_connections_succeeded: u64,
    connection_success_rate: f64,
    total_duration_secs: f64,
}

#[derive(Debug, Serialize)]
struct WsMetrics {
    ws_connect_latency: PercentileStats,
    first_event_latency: PercentileStats,
    total_events_received: u64,
    events_per_connection: f64,
}

#[derive(Debug, Serialize)]
struct WsErrorStats {
    ws_connection: u64,
    game_creation: u64,
    registration: u64,
}

#[derive(Clone)]
struct UserSession {
    user_id: Uuid,
    auth_cookie: String,
}

fn build_client(timeout_ms: u64) -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_millis(timeout_ms))
        .connect_timeout(Duration::from_millis(timeout_ms))
        .build()
        .expect("Failed to create HTTP client")
}

async fn register_user(
    client: &reqwest::Client,
    target_url: &str,
    index: usize,
) -> Result<UserSession> {
    let pseudo = format!("wstest_user_{}", index);
    let email = format!("wstest_{}@benchmark.local", index);

    let resp = client
        .post(format!("{}/api/auth/register", target_url))
        .json(&serde_json::json!({
            "pseudo": pseudo,
            "email": email,
            "password": "benchmark123",
            "password_confirm": "benchmark123",
        }))
        .send()
        .await?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("Registration failed: {}", body));
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
    Ok(UserSession {
        user_id,
        auth_cookie,
    })
}

async fn create_benchmark_game(
    client: &reqwest::Client,
    target_url: &str,
    user_ids: &[Uuid],
    bet: i32,
    benchmark_token: &str,
) -> Result<Uuid> {
    let mut req = client
        .post(format!(
            "{}/api/benchmark/create-multiplayer-game",
            target_url
        ))
        .json(&serde_json::json!({"user_ids": user_ids, "bet": bet}));
    if !benchmark_token.is_empty() {
        req = req.header("X-Benchmark-Token", benchmark_token);
    }
    let resp = req.send().await?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("Game creation failed: {}", body));
    }

    let json: serde_json::Value = resp.json().await?;
    Ok(json["game_id"]
        .as_str()
        .context("Missing game_id")?
        .parse::<Uuid>()?)
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("ws_load_test=info".parse().unwrap()),
        )
        .json()
        .init();

    let cli = Cli::parse();
    let benchmark_token = std::env::var("BENCHMARK_API_TOKEN").unwrap_or_default();
    info!(
        "Starting WS load test: {} concurrent, {} total, {}s",
        cli.concurrent_games, cli.total_games, cli.duration_secs
    );

    let client = build_client(cli.client_timeout_ms);
    let target_url = cli.target_url.trim_end_matches('/').to_string();

    if cli.cleanup {
        info!("Cleanup mode: deleting all benchmark data...");
        common::run_cleanup(&client, &target_url, &benchmark_token).await?;
        return Ok(());
    }

    let ws_connect_times: Arc<tokio::sync::Mutex<Vec<f64>>> = default_vec();
    let first_event_times: Arc<tokio::sync::Mutex<Vec<f64>>> = default_vec();
    let total_events = Arc::new(AtomicU64::new(0));
    let ws_connections_succeeded = Arc::new(AtomicU64::new(0));
    let ws_connections_attempted = Arc::new(AtomicU64::new(0));
    let games_created = Arc::new(AtomicU64::new(0));
    let reg_errors = Arc::new(AtomicU64::new(0));
    let creation_errors = Arc::new(AtomicU64::new(0));

    let total_users = cli.total_games * 4;
    let mut user_sessions = Vec::new();

    info!("Phase 1: Registering {} users...", total_users);
    for i in 0..total_users {
        if i > 0 && i % 100 == 0 {
            info!("Registered {}/{}", i, total_users);
        }
        match register_user(&client, &target_url, i).await {
            Ok(session) => user_sessions.push(session),
            Err(e) => {
                error!("Registration error: {}", e);
                reg_errors.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
    info!("Registered {} users", user_sessions.len());

    if user_sessions.is_empty() {
        return Err(anyhow::anyhow!("No user sessions registered, aborting"));
    }

    info!("Phase 2: Running WS benchmark...");
    let benchmark_start = Instant::now();
    let deadline = benchmark_start + Duration::from_secs(cli.duration_secs);
    let concurrency_sem = Arc::new(Semaphore::new(cli.concurrent_games));
    let mut games_started = 0usize;

    while games_started < cli.total_games && Instant::now() < deadline {
        let session_idx = games_started * 4;
        if session_idx + 4 > user_sessions.len() {
            warn!("Not enough user sessions for game {}", games_started);
            break;
        }
        let sessions: Vec<UserSession> = user_sessions[session_idx..session_idx + 4].to_vec();
        let user_ids: Vec<Uuid> = sessions.iter().map(|s| s.user_id).collect();

        let permit = concurrency_sem.clone().acquire_owned().await?;
        let client = client.clone();
        let target_url = target_url.clone();
        let ws_connect_times = ws_connect_times.clone();
        let first_event_times = first_event_times.clone();
        let total_events = total_events.clone();
        let ws_connections_succeeded = ws_connections_succeeded.clone();
        let ws_connections_attempted = ws_connections_attempted.clone();
        let games_created = games_created.clone();
        let creation_errors = creation_errors.clone();
        let bet = cli.bet;
        let benchmark_token = benchmark_token.clone();

        tokio::spawn(async move {
            let _permit = permit;
            let game_id =
                match create_benchmark_game(&client, &target_url, &user_ids, bet, &benchmark_token)
                    .await
                {
                    Ok(gid) => {
                        games_created.fetch_add(1, Ordering::Relaxed);
                        gid
                    }
                    Err(e) => {
                        error!("Creation error: {}", e);
                        creation_errors.fetch_add(1, Ordering::Relaxed);
                        return;
                    }
                };

            let ws_url = target_url
                .replace("http://", "ws://")
                .replace("https://", "wss://");

            let mut connect_handles = Vec::new();
            for (i, session) in sessions.iter().enumerate() {
                let endpoint = format!("{}/ws/{}", ws_url, game_id);
                let cookie = session.auth_cookie.clone();
                let connect_start = Instant::now();
                ws_connections_attempted.fetch_add(1, Ordering::Relaxed);

                connect_handles.push(async move {
                    // Use IntoClientRequest on the endpoint string so tungstenite
                    // properly adds all required WebSocket upgrade headers:
                    // Host, Connection: Upgrade, Upgrade: websocket,
                    // Sec-WebSocket-Version: 13, and Sec-WebSocket-Key.
                    let mut request = match tokio_tungstenite::tungstenite::client::IntoClientRequest::into_client_request(
                        &endpoint,
                    ) {
                        Ok(req) => req,
                        Err(e) => {
                            error!("WS[{}] request build fail: {}", i, e);
                            return None;
                        }
                    };
                    // Add the auth cookie for the server to authenticate the connection
                    if let Ok(cookie_val) = cookie.parse() {
                        request.headers_mut().insert("Cookie", cookie_val);
                    }
                    match tokio_tungstenite::connect_async(request).await {
                        Ok((ws_stream, _)) => {
                            let ms = connect_start.elapsed().as_secs_f64() * 1000.0;
                            Some((ms, ws_stream, i, game_id))
                        }
                        Err(e) => {
                            error!("WS[{}] connect fail: {}", i, e);
                            None
                        }
                    }
                });
            }

            let connections: Vec<_> = futures_util::future::join_all(connect_handles)
                .await
                .into_iter()
                .flatten()
                .collect();

            for (connect_ms, ws_stream, idx, gid) in connections {
                ws_connect_times.lock().await.push(connect_ms);
                ws_connections_succeeded.fetch_add(1, Ordering::Relaxed);

                let total_events = total_events.clone();
                let first_event_times = first_event_times.clone();
                let first_event_recorded = Arc::new(AtomicU64::new(0));
                let connection_start = Instant::now();

                tokio::spawn(async move {
                    let (_, mut read) = ws_stream.split();
                    while let Some(msg) = read.next().await {
                        match msg {
                            Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                                total_events.fetch_add(1, Ordering::Relaxed);
                                if first_event_recorded.fetch_add(1, Ordering::Relaxed) == 0 {
                                    first_event_times
                                        .lock()
                                        .await
                                        .push(connection_start.elapsed().as_secs_f64() * 1000.0);
                                }
                                if let Ok(event) = serde_json::from_str::<serde_json::Value>(&text)
                                {
                                    match event["type"].as_str().unwrap_or("unknown") {
                                        "game_joined" => info!("WS[{}] joined {}", idx, gid),
                                        "error" => warn!("WS[{}] error: {}", idx, event["message"]),
                                        _ => {}
                                    }
                                }
                            }
                            Ok(tokio_tungstenite::tungstenite::Message::Close(_)) => break,
                            Err(e) => {
                                error!("WS[{}] read error: {}", idx, e);
                                break;
                            }
                            _ => {}
                        }
                    }
                });
            }
        });
        games_started += 1;
    }

    info!("Dispatched {} WS sessions, stabilizing...", games_started);
    tokio::time::sleep(Duration::from_secs(10)).await;

    let elapsed = benchmark_start.elapsed().as_secs_f64();
    let attempted = ws_connections_attempted.load(Ordering::Relaxed);
    let succeeded = ws_connections_succeeded.load(Ordering::Relaxed);
    let total_events = total_events.load(Ordering::Relaxed);

    let report = WsBenchmarkReport {
        config: WsBenchmarkConfig {
            target_url,
            concurrent_games: cli.concurrent_games,
            total_games: cli.total_games,
            players_per_game: 4,
            bet: cli.bet,
            duration_secs: cli.duration_secs,
        },
        summary: WsBenchmarkSummary {
            games_created: games_created.load(Ordering::Relaxed),
            ws_connections_attempted: attempted,
            ws_connections_succeeded: succeeded,
            connection_success_rate: if attempted > 0 {
                succeeded as f64 / attempted as f64
            } else {
                0.0
            },
            total_duration_secs: elapsed,
        },
        ws_metrics: WsMetrics {
            ws_connect_latency: compute_percentile_stats(&ws_connect_times.lock().await),
            first_event_latency: compute_percentile_stats(&first_event_times.lock().await),
            total_events_received: total_events,
            events_per_connection: if succeeded > 0 {
                total_events as f64 / succeeded as f64
            } else {
                0.0
            },
        },
        errors: WsErrorStats {
            ws_connection: 0,
            game_creation: creation_errors.load(Ordering::Relaxed),
            registration: reg_errors.load(Ordering::Relaxed),
        },
    };

    let json = serde_json::to_string_pretty(&report)?;
    std::fs::write(&cli.output, &json)?;
    info!("Report written to {}", cli.output);
    println!("{}", json);
    Ok(())
}

fn default_vec() -> Arc<tokio::sync::Mutex<Vec<f64>>> {
    Arc::new(tokio::sync::Mutex::new(Vec::new()))
}
