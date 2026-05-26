use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rand::seq::SliceRandom;
use tracing::error;
use uuid::Uuid;

use jambo_backend::game::strategy::pick_playable_cards_in_round;

#[derive(Debug, Clone)]
pub struct UserSession {
    pub user_id: Uuid,
    pub auth_cookie: String,
}

#[derive(Debug, Clone)]
pub struct PlayerInfo {
    pub player_id: Uuid,
    pub user_id: Uuid,
    pub position: i32,
    pub cards: Vec<i32>,
}

struct ActiveGuard {
    inner: Arc<AtomicU64>,
    decremented: AtomicBool,
}

impl ActiveGuard {
    fn new(inner: Arc<AtomicU64>) -> Self {
        Self {
            inner,
            decremented: AtomicBool::new(false),
        }
    }
}

impl Drop for ActiveGuard {
    fn drop(&mut self) {
        if !self.decremented.load(Ordering::Relaxed) {
            self.inner.fetch_sub(1, Ordering::Relaxed);
            self.decremented.store(true, Ordering::Relaxed);
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn spawn_game_task(
    sessions: Vec<UserSession>,
    client: reqwest::Client,
    target_url: String,
    benchmark_token: String,
    bet: i32,
    think_time_ms: u64,
    is_warmup: bool,
    creation_times: &Arc<tokio::sync::Mutex<Vec<f64>>>,
    card_play_times: &Arc<tokio::sync::Mutex<Vec<f64>>>,
    duration_times: &Arc<tokio::sync::Mutex<Vec<f64>>>,
    games_created_counter: &Arc<AtomicU64>,
    games_completed_counter: &Arc<AtomicU64>,
    creation_failures: &Arc<AtomicU64>,
    card_play_errors: &Arc<AtomicU64>,
    active_in_flight: &Arc<AtomicU64>,
) {
    let creation_times = creation_times.clone();
    let card_play_times = card_play_times.clone();
    let duration_times = duration_times.clone();
    let games_created_counter = games_created_counter.clone();
    let games_completed_counter = games_completed_counter.clone();
    let creation_failures = creation_failures.clone();
    let card_play_errors = card_play_errors.clone();
    let active_in_flight = active_in_flight.clone();

    tokio::spawn(async move {
        let _guard = ActiveGuard::new(active_in_flight);

        let user_ids: Vec<Uuid> = sessions.iter().map(|s| s.user_id).collect();
        let cookies: std::collections::HashMap<Uuid, String> = sessions
            .iter()
            .map(|s| (s.user_id, s.auth_cookie.clone()))
            .collect();

        let create_start = Instant::now();
        let mut req = client
            .post(format!(
                "{}/api/benchmark/create-multiplayer-game",
                target_url
            ))
            .json(&serde_json::json!({"user_ids": user_ids, "bet": bet}));
        if !benchmark_token.is_empty() {
            req = req.header("X-Benchmark-Token", &benchmark_token);
        }

        let resp = match req.send().await {
            Ok(r) => r,
            Err(e) => {
                error!("Creation HTTP error: {}", e);
                creation_failures.fetch_add(1, Ordering::Relaxed);
                return;
            }
        };

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            error!(
                "Creation failed ({}): body='{}' | user_ids={:?} | bet={} | target={}/api/benchmark/create-multiplayer-game",
                status,
                body,
                user_ids,
                bet,
                target_url,
            );
            creation_failures.fetch_add(1, Ordering::Relaxed);
            return;
        }
        creation_times
            .lock()
            .await
            .push(create_start.elapsed().as_secs_f64() * 1000.0);

        let json: serde_json::Value = match resp.json().await {
            Ok(v) => v,
            Err(e) => {
                error!("Parse error: {}", e);
                return;
            }
        };

        let game_id: Uuid = match json["game_id"].as_str().and_then(|s| s.parse().ok()) {
            Some(id) => id,
            None => {
                error!("Missing game_id");
                return;
            }
        };
        let players: Vec<PlayerInfo> = json["players"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .map(|p| PlayerInfo {
                        player_id: p["player_id"]
                            .as_str()
                            .and_then(|s| s.parse().ok())
                            .unwrap_or_default(),
                        user_id: p["user_id"]
                            .as_str()
                            .and_then(|s| s.parse().ok())
                            .unwrap_or_default(),
                        position: p["position"].as_i64().unwrap_or(0) as i32,
                        cards: p["cards"]
                            .as_array()
                            .map(|c| {
                                c.iter()
                                    .filter_map(|v| v.as_i64())
                                    .map(|v| v as i32)
                                    .collect()
                            })
                            .unwrap_or_default(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut current_turn = json["current_turn_position"].as_i64().unwrap_or(0) as i32;

        games_created_counter.fetch_add(1, Ordering::Relaxed);

        let mut hands: HashMap<Uuid, Vec<i32>> = players
            .iter()
            .map(|p| (p.user_id, p.cards.clone()))
            .collect();
        let mut current_winning_card: Option<i32> = None;
        let mut round_play_count: i32 = 0;
        let loop_start = Instant::now();

        loop {
            let player = match players.iter().find(|p| p.position == current_turn) {
                Some(p) => p,
                None => {
                    error!("Player pos {} not found", current_turn);
                    break;
                }
            };
            let remaining = match hands.get_mut(&player.user_id) {
                Some(cards) if !cards.is_empty() => cards,
                _ => break,
            };
            let playable = pick_playable_cards_in_round(remaining, current_winning_card);
            let card_index = playable
                .choose(&mut rand::thread_rng())
                .copied()
                .unwrap_or(remaining[0]);
            remaining.retain(|c| *c != card_index);

            let cookie = match cookies.get(&player.user_id).cloned() {
                Some(c) => c,
                None => break,
            };

            let play_start = Instant::now();
            match client
                .post(format!("{}/api/games/{}/play", target_url, game_id))
                .json(&serde_json::json!({
                    "player_id": player.player_id.to_string(),
                    "card_index": card_index
                }))
                .header("Cookie", &cookie)
                .send()
                .await
            {
                Ok(resp) => {
                    card_play_times
                        .lock()
                        .await
                        .push(play_start.elapsed().as_secs_f64() * 1000.0);
                    if !resp.status().is_success() {
                        error!("Play error: {}", resp.text().await.unwrap_or_default());
                        card_play_errors.fetch_add(1, Ordering::Relaxed);
                        break;
                    }
                    let result: serde_json::Value = match resp.json().await {
                        Ok(v) => v,
                        Err(_) => {
                            card_play_errors.fetch_add(1, Ordering::Relaxed);
                            break;
                        }
                    };
                    if round_play_count == 0 {
                        current_winning_card = Some(card_index);
                    } else if let Some(winning) = current_winning_card {
                        if winning / 8 == card_index / 8 && card_index % 8 > winning % 8 {
                            current_winning_card = Some(card_index);
                        }
                    }
                    round_play_count += 1;
                    if round_play_count >= 4 {
                        current_winning_card = None;
                        round_play_count = 0;
                    }
                    if result["game_ended"].as_bool().unwrap_or(false) {
                        duration_times
                            .lock()
                            .await
                            .push(loop_start.elapsed().as_secs_f64());
                        games_completed_counter.fetch_add(1, Ordering::Relaxed);
                        break;
                    }
                    if let Some(nt) = result["next_turn"]
                        .as_str()
                        .and_then(|s| s.parse::<Uuid>().ok())
                        .and_then(|pid| players.iter().find(|p| p.player_id == pid))
                    {
                        current_turn = nt.position;
                    }
                    tokio::time::sleep(Duration::from_millis(think_time_ms)).await;
                }
                Err(e) => {
                    error!("Play HTTP error: {}", e);
                    card_play_errors.fetch_add(1, Ordering::Relaxed);
                    break;
                }
            }
        }
        if !is_warmup {}
    });
}

pub fn default_vec() -> Arc<tokio::sync::Mutex<Vec<f64>>> {
    Arc::new(tokio::sync::Mutex::new(Vec::new()))
}
