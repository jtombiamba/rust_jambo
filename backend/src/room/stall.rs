use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use crate::database::models::{GameRun, GameRunGame, GameRunPlayer, GameStatus, User};
use crate::database::repositories::{
    GameRepository, GameRunGameRepository, GameRunPlayerRepository, GameRunRepository,
    UserRepository,
};
use crate::mailer::Mailer;
use crate::room::service::RoomService;

impl RoomService {
    #[allow(dead_code)]
    pub async fn check_stalled_runs(
        db: sea_orm::DatabaseConnection,
        mailer: Arc<dyn Mailer>,
        timeout_secs: u64,
    ) -> u64 {
        let now = chrono::Utc::now();
        let cutoff = now - chrono::Duration::seconds(timeout_secs as i64);
        let cancel_cutoff = now - chrono::Duration::seconds(timeout_secs as i64 * 2);

        let run_repo = GameRunRepository::new(db.clone());
        let run_game_repo = GameRunGameRepository::new(db.clone());
        let game_repo = GameRepository::new(db.clone());
        let run_player_repo = GameRunPlayerRepository::new(db.clone());
        let user_repo = UserRepository::new(db.clone(), 0);

        let stalled_runs = match run_repo.find_stalled_active(cutoff).await {
            Ok(runs) => runs,
            Err(e) => {
                tracing::error!("Failed to query stalled runs: {}", e);
                return 0;
            }
        };

        let eligible_runs: Vec<GameRun> = stalled_runs
            .into_iter()
            .filter(|run| run.current_game_index < run.num_games)
            .collect();

        if eligible_runs.is_empty() {
            return 0;
        }

        let run_ids: Vec<Uuid> = eligible_runs.iter().map(|r| r.id).collect();

        // Batch: last game per run (highest game_index below num_games).
        let run_games = match run_game_repo.list_by_runs(&run_ids).await {
            Ok(g) => g,
            Err(e) => {
                tracing::error!("Failed to query run games: {}", e);
                return 0;
            }
        };
        let num_games_by_run: HashMap<Uuid, i32> =
            eligible_runs.iter().map(|r| (r.id, r.num_games)).collect();
        let mut last_game_by_run: HashMap<Uuid, GameRunGame> = HashMap::new();
        for rg in run_games {
            let Some(&num_games) = num_games_by_run.get(&rg.game_run_id) else {
                continue;
            };
            if rg.game_index >= num_games {
                continue;
            }
            match last_game_by_run.get(&rg.game_run_id) {
                Some(existing) if existing.game_index >= rg.game_index => {}
                _ => {
                    last_game_by_run.insert(rg.game_run_id, rg);
                }
            }
        }

        // Batch: games referenced by those last games.
        let game_ids: Vec<Uuid> = last_game_by_run.values().map(|rg| rg.game_id).collect();
        let games = match game_repo.find_by_ids(&game_ids).await {
            Ok(g) => g,
            Err(e) => {
                tracing::error!("Failed to query games for stalled runs: {}", e);
                return 0;
            }
        };
        let game_by_id: HashMap<Uuid, crate::database::models::game::Model> =
            games.into_iter().map(|g| (g.id, g)).collect();

        let mut processed = 0u64;
        let mut warning_runs: Vec<GameRun> = Vec::new();

        for run in &eligible_runs {
            let Some(last_game) = last_game_by_run.get(&run.id) else {
                tracing::info!(
                    "Stalled run {} has no games yet ({} games remaining)",
                    run.id,
                    run.num_games - run.current_game_index
                );
                processed += 1;
                continue;
            };

            let Some(current_game) = game_by_id.get(&last_game.game_id) else {
                continue;
            };

            let is_finished = matches!(
                current_game.status,
                GameStatus::Finished | GameStatus::Kora | GameStatus::DoubleKora
            );

            if !is_finished {
                continue;
            }

            let stall_seconds = (now - run.updated_at).num_seconds();

            if run.updated_at < cancel_cutoff {
                tracing::info!(
                    "Run {} stalled too long ({}s), auto-cancelling",
                    run.id,
                    stall_seconds
                );
                if let Err(e) = run_repo.cancel(run.id, now).await {
                    tracing::error!("Failed to cancel stalled run {}: {}", run.id, e);
                }
                processed += 1;
                continue;
            }

            if run.stall_warning_sent_at.is_some() {
                processed += 1;
                continue;
            }

            tracing::info!(
                "Run {} stalled: last game {} finished, {}s since last activity, {} games remaining",
                run.id,
                last_game.game_id,
                stall_seconds,
                run.num_games - run.current_game_index
            );

            warning_runs.push(run.clone());
        }

        // Batch: players and users for the warning branch.
        if !warning_runs.is_empty() {
            let warning_run_ids: Vec<Uuid> = warning_runs.iter().map(|r| r.id).collect();
            let players = match run_player_repo.list_active_by_runs(&warning_run_ids).await {
                Ok(p) => p,
                Err(e) => {
                    tracing::error!("Failed to fetch run players: {}", e);
                    return processed;
                }
            };

            let mut players_by_run: HashMap<Uuid, Vec<GameRunPlayer>> = HashMap::new();
            for p in players {
                players_by_run.entry(p.game_run_id).or_default().push(p);
            }

            let player_ids: Vec<Uuid> = warning_runs
                .iter()
                .flat_map(|r| {
                    players_by_run
                        .get(&r.id)
                        .into_iter()
                        .flatten()
                        .map(|p| p.user_id)
                })
                .collect();
            let users: Vec<User> = if player_ids.is_empty() {
                Vec::new()
            } else {
                match user_repo.find_by_ids(&player_ids).await {
                    Ok(u) => u,
                    Err(e) => {
                        tracing::error!("Failed to fetch users for stalled runs: {}", e);
                        return processed;
                    }
                }
            };
            let user_map: HashMap<Uuid, &User> = users.iter().map(|u| (u.id, u)).collect();

            for run in &warning_runs {
                let remaining_secs = (cancel_cutoff - run.updated_at).num_seconds().max(0);
                let inactive_minutes = (now - run.updated_at).num_seconds() / 60;
                let remaining_minutes = remaining_secs / 60;

                if let Some(run_players) = players_by_run.get(&run.id) {
                    for rp in run_players {
                        if let Some(user) = user_map.get(&rp.user_id) {
                            let lang = crate::i18n::Lang::parse(&user.language).unwrap_or_default();
                            if let Err(e) = mailer
                                .send_stall_warning(
                                    &user.email,
                                    &run.id.to_string(),
                                    inactive_minutes,
                                    remaining_minutes,
                                    lang,
                                )
                                .await
                            {
                                tracing::error!(
                                    "Failed to send run stall warning to {}: {}",
                                    user.email,
                                    e
                                );
                            }
                        }
                    }
                }

                if let Err(e) = run_repo.mark_stall_warning_sent(run.id, now).await {
                    tracing::error!("Failed to update run {} stall metadata: {}", run.id, e);
                }

                processed += 1;
            }
        }

        if processed > 0 {
            tracing::info!("Processed {} stalled runs", processed);
        }
        processed
    }
}
