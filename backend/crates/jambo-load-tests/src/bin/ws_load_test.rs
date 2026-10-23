mod common;

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use clap::Parser;
use futures_util::{SinkExt, StreamExt};
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

    #[arg(long, default_value = "100")]
    spectators_per_game: usize,

    #[arg(long, default_value = "5000")]
    client_timeout_ms: u64,

    #[arg(long)]
    cleanup: bool,

    #[arg(long, default_value = "ws-benchmark.json")]
    output: String,

    #[arg(long, default_value = "http://localhost:9090")]
    prometheus_url: String,

    #[arg(long, default_value = "5")]
    metrics_interval_secs: u64,
}

#[derive(Debug, Serialize)]
struct WsBenchmarkReport {
    config: WsBenchmarkConfig,
    summary: WsBenchmarkSummary,
    ws_metrics: WsMetrics,
    resource_metrics: ResourceMetrics,
    errors: WsErrorStats,
}

#[derive(Debug, Serialize)]
struct WsBenchmarkConfig {
    target_url: String,
    concurrent_games: usize,
    total_games: usize,
    players_per_game: usize,
    spectators_per_game: usize,
    bet: i32,
    duration_secs: u64,
}

#[derive(Debug, Serialize)]
struct WsBenchmarkSummary {
    games_created: u64,
    ws_connections_attempted: u64,
    ws_connections_succeeded: u64,
    connection_success_rate: f64,
    spectator_connections_attempted: u64,
    spectator_connections_succeeded: u64,
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

#[derive(Debug, Serialize)]
struct ResourceMetrics {
    backend: ResourceStats,
}

#[derive(Debug, Serialize, Default)]
struct ResourceStats {
    cpu_avg_percent: f64,
    cpu_peak_percent: f64,
    rss_avg_mb: f64,
    rss_peak_mb: f64,
    samples: u32,
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

/// Query a Prometheus scalar (instant vector with a single value) and return
/// the numeric value, or `None` if the metric is absent / not yet scraped.
async fn query_prometheus_scalar(
    client: &reqwest::Client,
    prometheus_url: &str,
    query: &str,
) -> Result<Option<f64>> {
    let url = format!("{}/api/v1/query", prometheus_url.trim_end_matches('/'));
    let resp = client.get(&url).query(&[("query", query)]).send().await?;
    let json: serde_json::Value = resp.json().await?;
    let value = json
        .get("data")
        .and_then(|d| d.get("result"))
        .and_then(|r| r.as_array())
        .and_then(|arr| arr.first())
        .and_then(|item| item.get("value"))
        .and_then(|v| v.as_array())
        .and_then(|v| v.get(1))
        .and_then(|s| s.as_str())
        .and_then(|s| s.parse::<f64>().ok());
    Ok(value)
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

/// Mint a spectator token for the game as one of its participants, returning the
/// raw token to be passed as the `?token=` WebSocket query parameter.
async fn mint_spectate_token(
    client: &reqwest::Client,
    target_url: &str,
    game_id: Uuid,
    auth_cookie: &str,
) -> Result<String> {
    let resp = client
        .post(format!(
            "{}/api/games/{}/spectate-token",
            target_url, game_id
        ))
        .header("Cookie", auth_cookie)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!(
            "Spectate token mint failed: {} {}",
            status,
            body
        ));
    }

    let json: serde_json::Value = resp.json().await?;
    Ok(json["token"].as_str().context("Missing token")?.to_string())
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
    let spectator_connections_succeeded = Arc::new(AtomicU64::new(0));
    let spectator_connections_attempted = Arc::new(AtomicU64::new(0));
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
    let mut game_handles = Vec::new();

    // Sample the backend's self-reported CPU/memory (via Prometheus) while the
    // games dispatch and their connections are established.
    let resource_samples: Arc<tokio::sync::Mutex<Vec<(f64, f64)>>> =
        Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let sampler_stop = Arc::new(AtomicBool::new(false));
    let sampler = {
        let client = client.clone();
        let prometheus_url = cli.prometheus_url.clone();
        let interval = Duration::from_secs(cli.metrics_interval_secs);
        let samples = resource_samples.clone();
        let stop = sampler_stop.clone();
        tokio::spawn(async move {
            while !stop.load(Ordering::Relaxed) {
                let cpu = query_prometheus_scalar(
                    &client,
                    &prometheus_url,
                    "cpu_usage_percent{process=\"backend\"}",
                )
                .await
                .ok()
                .flatten();
                let rss_bytes = query_prometheus_scalar(
                    &client,
                    &prometheus_url,
                    "memory_usage_bytes{process=\"backend\"}",
                )
                .await
                .ok()
                .flatten();
                match (cpu, rss_bytes) {
                    (Some(c), Some(m)) => {
                        samples.lock().await.push((c, m / (1024.0 * 1024.0)));
                    }
                    _ => warn!("Prometheus sample missing for backend CPU/memory"),
                }
                tokio::time::sleep(interval).await;
            }
        })
    };

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
        let spectator_connections_succeeded = spectator_connections_succeeded.clone();
        let spectator_connections_attempted = spectator_connections_attempted.clone();
        let games_created = games_created.clone();
        let creation_errors = creation_errors.clone();
        let bet = cli.bet;
        let spectators_per_game = cli.spectators_per_game;
        let benchmark_token = benchmark_token.clone();

        let handle = tokio::spawn(async move {
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

            if spectators_per_game > 0 {
                // Production mints ONE spectate token per game (the shared
                // "copy stream URL" flow); every spectator connects with that
                // same token. Mirror that here.
                let token = match mint_spectate_token(
                    &client,
                    &target_url,
                    game_id,
                    &sessions[0].auth_cookie,
                )
                .await
                {
                    Ok(t) => t,
                    Err(e) => {
                        error!("Spectate token mint error for {}: {}", game_id, e);
                        return;
                    }
                };

                let mut spectator_handles = Vec::new();
                for i in 0..spectators_per_game {
                    let endpoint = format!("{}/ws/{}?token={}", ws_url, game_id, token);
                    let connect_start = Instant::now();
                    spectator_connections_attempted.fetch_add(1, Ordering::Relaxed);

                    spectator_handles.push(async move {
                        let request = match tokio_tungstenite::tungstenite::client::IntoClientRequest::into_client_request(&endpoint) {
                            Ok(req) => req,
                            Err(e) => {
                                error!("SPEC[{}] request build fail: {}", i, e);
                                return None;
                            }
                        };
                        match tokio_tungstenite::connect_async(request).await {
                            Ok((ws_stream, _)) => {
                                let ms = connect_start.elapsed().as_secs_f64() * 1000.0;
                                Some((ms, ws_stream, i, game_id))
                            }
                            Err(e) => {
                                error!("SPEC[{}] connect fail: {}", i, e);
                                None
                            }
                        }
                    });
                }

                let spectator_connections: Vec<_> =
                    futures_util::future::join_all(spectator_handles)
                        .await
                        .into_iter()
                        .flatten()
                        .collect();

                for (connect_ms, ws_stream, idx, gid) in spectator_connections {
                    ws_connect_times.lock().await.push(connect_ms);
                    spectator_connections_succeeded.fetch_add(1, Ordering::Relaxed);

                    let total_events = total_events.clone();
                    let first_event_times = first_event_times.clone();
                    let first_event_recorded = Arc::new(AtomicU64::new(0));
                    let connection_start = Instant::now();

                    tokio::spawn(async move {
                        let (mut write, mut read) = ws_stream.split();
                        // Mark the connection as a spectator so the backend emits a
                        // public snapshot (and the periodic resync reaches it).
                        let join = serde_json::json!({
                            "type": "join_game",
                            "game_id": gid,
                            "spectator": true,
                        });
                        if let Err(e) = write
                            .send(tokio_tungstenite::tungstenite::Message::Text(
                                join.to_string().into(),
                            ))
                            .await
                        {
                            error!("SPEC[{}] join send fail: {}", idx, e);
                        }
                        while let Some(msg) = read.next().await {
                            match msg {
                                Ok(tokio_tungstenite::tungstenite::Message::Text(_)) => {
                                    total_events.fetch_add(1, Ordering::Relaxed);
                                    if first_event_recorded.fetch_add(1, Ordering::Relaxed) == 0 {
                                        first_event_times.lock().await.push(
                                            connection_start.elapsed().as_secs_f64() * 1000.0,
                                        );
                                    }
                                }
                                Ok(tokio_tungstenite::tungstenite::Message::Close(_)) => break,
                                Err(e) => {
                                    error!("SPEC[{}] read error: {}", idx, e);
                                    break;
                                }
                                _ => {}
                            }
                        }
                    });
                }
            }
        });
        game_handles.push(handle);
        games_started += 1;
    }

    info!(
        "Dispatched {} WS sessions, awaiting connections...",
        games_started
    );
    futures_util::future::join_all(game_handles).await;

    sampler_stop.store(true, Ordering::Relaxed);
    let _ = sampler.await;

    let resource_metrics = {
        let samples = resource_samples.lock().await;
        let mut stats = ResourceStats {
            samples: samples.len() as u32,
            ..Default::default()
        };
        if !samples.is_empty() {
            let mut cpu_sum = 0.0;
            let mut rss_sum = 0.0;
            let mut cpu_peak = f64::MIN;
            let mut rss_peak = f64::MIN;
            for (cpu, rss) in samples.iter() {
                cpu_sum += cpu;
                rss_sum += rss;
                if *cpu > cpu_peak {
                    cpu_peak = *cpu;
                }
                if *rss > rss_peak {
                    rss_peak = *rss;
                }
            }
            let n = samples.len() as f64;
            stats.cpu_avg_percent = cpu_sum / n;
            stats.rss_avg_mb = rss_sum / n;
            stats.cpu_peak_percent = cpu_peak;
            stats.rss_peak_mb = rss_peak;
        }
        ResourceMetrics { backend: stats }
    };

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
            spectators_per_game: cli.spectators_per_game,
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
            spectator_connections_attempted: spectator_connections_attempted
                .load(Ordering::Relaxed),
            spectator_connections_succeeded: spectator_connections_succeeded
                .load(Ordering::Relaxed),
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
        resource_metrics,
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
