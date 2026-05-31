impl RoomService {
    pub async fn get_current_game(
        &self,
        run_id: Uuid,
        user_id: Uuid,
    ) -> Result<serde_json::Value, RoomServiceError> {
        let run = self
            .run_repo
            .find_by_id(run_id)
            .await?
            .ok_or(RoomServiceError::RunNotFound)?;

        let _run_player = self
            .run_player_repo
            .find_by_run_and_user(run_id, user_id)
            .await?
            .ok_or(RoomServiceError::NotRunPlayer)?;

        let current = self
            .run_game_repo
            .find_by_run_and_index(run_id, run.current_game_index)
            .await?;

        match current {
            Some(rg) => Ok(serde_json::json!({
                "run_id": run_id,
                "game_id": rg.game_id,
                "game_index": rg.game_index,
                "status": rg.status,
            })),
            None => Err(RoomServiceError::GameNotFound),
        }
    }

    pub async fn start_next_game(
        &self,
        run_id: Uuid,
        user_id: Uuid,
    ) -> Result<serde_json::Value, RoomServiceError> {
        self.acquire_start_game_lock(run_id).await?;

        let lock = {
            let mut locks = self.start_game_locks.lock().await;
            locks
                .entry(run_id)
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
                .clone()
        };
        let _guard = lock.lock().await;

        let run = self
            .run_repo
            .find_by_id(run_id)
            .await?
            .ok_or(RoomServiceError::RunNotFound)?;

        if run.status != "active" {
            return Err(RoomServiceError::RunNotActive {
                status: run.status.clone(),
            });
        }

        let _run_player = self
            .run_player_repo
            .find_by_run_and_user(run_id, user_id)
            .await?
            .ok_or(RoomServiceError::NotRunPlayer)?;

        let game_index = run.current_game_index;
        if game_index >= run.num_games {
            return Err(RoomServiceError::RunCompleted);
        }

        let existing = self
            .run_game_repo
            .find_by_run_and_index(run_id, game_index)
            .await?;
        if let Some(rg) = existing {
            return Ok(serde_json::json!({
                "game_id": rg.game_id,
                "game_index": game_index,
                "total_games": run.num_games,
                "current_game_index": game_index,
                "status": "existing",
            }));
        }

        if game_index > 0 {
            if let Some(prev_run_game) = self
                .run_game_repo
                .find_by_run_and_index(run_id, game_index - 1)
                .await?
            {
                let prev_game =
                    crate::database::models::game::Entity::find_by_id(prev_run_game.game_id)
                        .one(&self.db)
                        .await?;
                if let Some(pg) = prev_game {
                    let is_finished = matches!(
                        pg.status,
                        GameStatus::Finished
                            | GameStatus::Kora
                            | GameStatus::DoubleKora
                            | GameStatus::Cancelled
                    );
                    if !is_finished {
                        return Err(RoomServiceError::RunNotActive {
                            status: "previous_game_not_finished".to_string(),
                        });
                    }
                }
            }
        }

        let run_players = self.run_player_repo.list_by_run(run_id).await?;

        if run_players.len() < 2 {
            return Err(RoomServiceError::NotEnoughPlayers);
        }

        let player_ids: Vec<Uuid> = run_players.iter().map(|p| p.user_id).collect();

        let users = self.user_repo.find_by_ids(&player_ids).await?;
        let user_map: HashMap<Uuid, String> = users.into_iter().map(|u| (u.id, u.pseudo)).collect();

        let bet = run.bet_per_game;
        use rand::seq::SliceRandom;
        use rand::thread_rng;

        let mut shuffled_positions: Vec<usize> = (0..player_ids.len()).collect();
        shuffled_positions.shuffle(&mut thread_rng());

        let game_id = Uuid::new_v4();
        let now = chrono::Utc::now();

        use serde_json::json;

        let position_map: std::collections::HashMap<i32, Uuid> = shuffled_positions
            .iter()
            .enumerate()
            .map(|(new_pos, &orig_idx)| (new_pos as i32, player_ids[orig_idx]))
            .collect();

        let txn = self.db.begin().await?;

        let game_active = game::ActiveModel {
            id: Set(game_id),
            status: Set(GameStatus::Active),
            bet: Set(bet),
            created_at: Set(now),
            updated_at: Set(now),
            finished_at: ActiveValue::NotSet,
            rank: Set(Some(0)),
            roll: Set(1),
            auto: Set(false),
            winner_id: ActiveValue::NotSet,
            player_positions: Set(serde_json::to_value(&position_map)
                .map_err(|e| RoomServiceError::Internal(format!("Failed to serialize: {}", e)))?),
            current_winning_card: ActiveValue::NotSet,
            current_winning_player_position: ActiveValue::NotSet,
            creator_id: Set(Some(user_id)),
            game_mode: Set(GameMode::Multiplayer),
            max_players: Set(player_ids.len() as i16),
            invite_expires_at: ActiveValue::NotSet,
            stall_warning_sent_at: ActiveValue::NotSet,
            game_run_id: Set(Some(run_id)),
            kicked_players: Set(json!([])),
        };
        game::Entity::insert(game_active).exec(&txn).await?;

        let profiles = self.profile_repo.find_by_user_ids(&player_ids).await?;
        let profile_map: HashMap<Uuid, crate::database::models::PlayerProfile> =
            profiles.into_iter().map(|p| (p.user_id, p)).collect();

        let run_player_map: HashMap<Uuid, crate::database::models::GameRunPlayer> =
            run_players.into_iter().map(|rp| (rp.user_id, rp)).collect();

        for (new_pos, &orig_idx) in shuffled_positions.iter().enumerate() {
            let loop_user_id = player_ids[orig_idx];
            let player_id = Uuid::new_v4();
            let name = user_map
                .get(&loop_user_id)
                .cloned()
                .unwrap_or_else(|| "Unknown".to_string());

            let profile_credit = profile_map
                .get(&loop_user_id)
                .ok_or_else(|| RoomServiceError::ProfileNotFound)?
                .credit;

            player::Entity::insert(player::ActiveModel {
                id: Set(player_id),
                game_id: Set(game_id),
                player_type: Set(crate::database::models::PlayerType::Human),
                name: Set(name),
                position: Set(new_pos as i32),
                credits: Set(profile_credit),
                created_at: Set(now),
                user_id: Set(Some(loop_user_id)),
                kicked: Set(false),
                kicked_at: ActiveValue::NotSet,
            })
            .exec(&txn)
            .await?;

            if let Some(rp) = run_player_map.get(&loop_user_id) {
                let new_provisioned = (rp.provisioned_credits - bet).max(0);
                use crate::database::models::game_run_player;
                game_run_player::Entity::update_many()
                    .col_expr(
                        game_run_player::Column::ProvisionedCredits,
                        sea_orm::sea_query::Expr::value(new_provisioned),
                    )
                    .filter(game_run_player::Column::Id.eq(rp.id))
                    .filter(game_run_player::Column::ProvisionedCredits.gte(bet))
                    .exec(&txn)
                    .await?;
            }
        }

        let cards: Vec<i32> = {
            let mut cards: Vec<i32> = (0..crate::game::constants::TOTAL_CARDS as i32).collect();
            cards.shuffle(&mut thread_rng());
            cards
        };

        let created_players = {
            use sea_orm::EntityTrait;
            player::Entity::find()
                .filter(player::Column::GameId.eq(game_id))
                .all(&txn)
                .await?
        };

        let card_models: Vec<crate::database::models::game_card::ActiveModel> = created_players
            .iter()
            .enumerate()
            .flat_map(|(i, p)| {
                let start = i * crate::game::constants::CARDS_PER_PLAYER;
                let end = start + crate::game::constants::CARDS_PER_PLAYER;
                cards[start..end].iter().map(move |&ci| {
                    crate::database::models::game_card::ActiveModel {
                        id: Set(Uuid::new_v4()),
                        game_id: Set(game_id),
                        player_id: Set(Some(p.id)),
                        card_index: Set(ci),
                        played: Set(false),
                        played_at: ActiveValue::NotSet,
                        round: ActiveValue::NotSet,
                        created_at: Set(now),
                    }
                })
            })
            .collect();

        crate::database::models::game_card::Entity::insert_many(card_models)
            .exec(&txn)
            .await?;

        use crate::database::models::game_run_game;
        game_run_game::Entity::insert(game_run_game::ActiveModel {
            id: Set(Uuid::new_v4()),
            game_run_id: Set(run_id),
            game_id: Set(game_id),
            game_index: Set(game_index),
            status: Set("active".to_string()),
            created_at: Set(now),
        })
        .exec(&txn)
        .await?;

        let new_index = game_index + 1;
        use crate::database::models::game_run;
        game_run::Entity::update_many()
            .col_expr(
                game_run::Column::CurrentGameIndex,
                sea_orm::sea_query::Expr::value(new_index),
            )
            .col_expr(
                game_run::Column::UpdatedAt,
                sea_orm::sea_query::Expr::value(now),
            )
            .filter(game_run::Column::Id.eq(run_id))
            .exec(&txn)
            .await?;

        txn.commit().await?;

        let all_games_created = new_index >= run.num_games;

        if all_games_created {
            let mut locks = self.start_game_locks.lock().await;
            locks.remove(&run_id);
        }

        self.release_start_game_lock(run_id).await;

        self.run_event_repo
            .log(
                run_id,
                Some(user_id),
                "game_started",
                Some(&format!("game_index={}", game_index)),
            )
            .await?;

        self.publish_event(&RoomEvent::GameStarted {
            room_id: run.room_id,
            run_id,
            game_id,
            game_index,
            total_games: run.num_games,
        })
        .await;

        Ok(serde_json::json!({
            "game_id": game_id,
            "game_index": game_index,
            "total_games": run.num_games,
            "current_game_index": new_index,
            "all_games_created": all_games_created,
        }))
    }

    pub async fn list_runs(
        &self,
        room_id: Uuid,
        user_id: Uuid,
    ) -> Result<Vec<serde_json::Value>, RoomServiceError> {
        let member = self.member_repo.find_membership(room_id, user_id).await?;
        if member.is_none() {
            return Err(RoomServiceError::NotMember);
        }

        let runs = self.run_repo.list_by_room(room_id).await?;

        let result = runs
            .iter()
            .map(|r| {
                serde_json::json!({
                    "id": r.id,
                    "num_games": r.num_games,
                    "bet_per_game": r.bet_per_game,
                    "current_game_index": r.current_game_index,
                    "status": r.status,
                    "created_at": r.created_at.to_rfc3339(),
                })
            })
            .collect();

        Ok(result)
    }
}
