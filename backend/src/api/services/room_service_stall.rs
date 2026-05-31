impl RoomService {
    #[allow(dead_code)]
    pub async fn check_stalled_runs(
        db: sea_orm::DatabaseConnection,
        mailer: Arc<dyn Mailer>,
        timeout_secs: u64,
    ) -> u64 {
        use crate::database::models::{game, game_run, game_run_game, game_run_player, user};
        use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set};

        let now = chrono::Utc::now();
        let cutoff = now - chrono::Duration::seconds(timeout_secs as i64);
        let cancel_cutoff = now - chrono::Duration::seconds(timeout_secs as i64 * 2);

        let stalled_runs = match game_run::Entity::find()
            .filter(game_run::Column::Status.eq("active"))
            .filter(game_run::Column::UpdatedAt.lt(cutoff))
            .all(&db)
            .await
        {
            Ok(runs) => runs,
            Err(e) => {
                tracing::error!("Failed to query stalled runs: {}", e);
                return 0;
            }
        };

        let mut processed = 0u64;

        for run in stalled_runs {
            if run.current_game_index >= run.num_games {
                continue;
            }

            let last_game = match game_run_game::Entity::find()
                .filter(game_run_game::Column::GameRunId.eq(run.id))
                .filter(game_run_game::Column::GameIndex.lt(run.num_games))
                .order_by_desc(game_run_game::Column::GameIndex)
                .one(&db)
                .await
            {
                Ok(Some(g)) => g,
                _ => {
                    tracing::info!(
                        "Stalled run {} has no games yet ({} games remaining)",
                        run.id,
                        run.num_games - run.current_game_index
                    );
                    processed += 1;
                    continue;
                }
            };

            let current_game = match game::Entity::find_by_id(last_game.game_id).one(&db).await {
                Ok(Some(g)) => g,
                _ => continue,
            };

            let is_finished = matches!(
                current_game.status,
                crate::database::models::GameStatus::Finished
                    | crate::database::models::GameStatus::Kora
                    | crate::database::models::GameStatus::DoubleKora
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
                let mut active: game_run::ActiveModel = run.clone().into();
                active.status = Set("cancelled".to_string());
                active.stall_cancelled_at = Set(Some(now));
                active.updated_at = Set(now);
                if let Err(e) = active.update(&db).await {
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

            let players = match game_run_player::Entity::find()
                .filter(game_run_player::Column::GameRunId.eq(run.id))
                .filter(game_run_player::Column::Kicked.eq(false))
                .all(&db)
                .await
            {
                Ok(p) => p,
                Err(e) => {
                    tracing::error!("Failed to fetch run players for {}: {}", run.id, e);
                    continue;
                }
            };

            let player_ids: Vec<Uuid> = players.iter().map(|p| p.user_id).collect();
            let users = if player_ids.is_empty() {
                Vec::new()
            } else {
                match user::Entity::find()
                    .filter(user::Column::Id.is_in(player_ids.iter().copied()))
                    .all(&db)
                    .await
                {
                    Ok(u) => u,
                    Err(e) => {
                        tracing::error!("Failed to fetch users for run {}: {}", run.id, e);
                        continue;
                    }
                }
            };
            let user_map: std::collections::HashMap<Uuid, &crate::database::models::User> =
                users.iter().map(|u| (u.id, u)).collect();

            let remaining_secs = (cancel_cutoff - run.updated_at).num_seconds().max(0);
            let inactive_minutes = stall_seconds / 60;
            let remaining_minutes = remaining_secs / 60;

            for rp in &players {
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

            {
                let mut active: game_run::ActiveModel = run.clone().into();
                active.stall_warning_sent_at = Set(Some(now));
                active.updated_at = Set(now);
                if let Err(e) = active.update(&db).await {
                    tracing::error!("Failed to update run {} stall metadata: {}", run.id, e);
                }
            }

            processed += 1;
        }

        if processed > 0 {
            tracing::info!("Processed {} stalled runs", processed);
        }
        processed
    }
}
