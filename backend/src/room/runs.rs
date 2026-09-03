use std::collections::HashMap;
use uuid::Uuid;

use crate::api::dto::responses::{
    ActiveRunGameInfo, ActiveRunPlayerInfo, ActiveRunResponse, CreateRunResponse, JoinRunResponse,
};
use crate::database::models::RunStatus;
use crate::game::constants::KORA_CREDIT_MULTIPLIER;
use crate::messaging::events::RoomEvent;
use crate::room::error::RoomServiceError;
use crate::room::service::RoomService;

impl RoomService {
    async fn validate_create_run(
        &self,
        room_id: Uuid,
        user_id: Uuid,
        num_games: i32,
        bet: i32,
        player_ids: &[Uuid],
    ) -> Result<Vec<Uuid>, RoomServiceError> {
        self.room_repo
            .find_by_id(room_id)
            .await?
            .ok_or(RoomServiceError::RoomNotFound)?;

        if self
            .member_repo
            .find_membership(room_id, user_id)
            .await?
            .is_none()
        {
            return Err(RoomServiceError::NotMember);
        }

        if self.run_repo.find_active_by_room(room_id).await?.is_some() {
            return Err(RoomServiceError::RunAlreadyActive);
        }

        if num_games <= 0 {
            return Err(RoomServiceError::Internal(
                "num_games must be positive".to_string(),
            ));
        }
        if bet <= 0 {
            return Err(RoomServiceError::Internal(
                "bet must be positive".to_string(),
            ));
        }
        if player_ids.len() < 2 {
            return Err(RoomServiceError::NotEnoughPlayers);
        }
        if player_ids.len() > self.config.room_max_players as usize {
            return Err(RoomServiceError::TooManyPlayers {
                max: self.config.room_max_players,
            });
        }

        let player_ids: Vec<Uuid> = {
            let mut seen = std::collections::HashSet::new();
            player_ids
                .iter()
                .copied()
                .filter(|id| seen.insert(*id))
                .collect()
        };

        let room_members = self.member_repo.list_by_room(room_id).await?;
        let member_ids: std::collections::HashSet<Uuid> =
            room_members.iter().map(|m| m.user_id).collect();
        for p_id in &player_ids {
            if !member_ids.contains(p_id) {
                return Err(RoomServiceError::NotMember);
            }
        }

        Ok(player_ids)
    }

    async fn validate_players_credit(
        &self,
        player_ids: &[Uuid],
        required_credit: i32,
    ) -> Result<(), RoomServiceError> {
        let profiles = self.profile_repo.find_by_user_ids(player_ids).await?;
        let profile_map: HashMap<Uuid, crate::database::models::PlayerProfile> =
            profiles.into_iter().map(|p| (p.user_id, p)).collect();

        for &p_id in player_ids {
            let profile = profile_map
                .get(&p_id)
                .ok_or_else(|| RoomServiceError::ProfileNotFound)?;

            if let Some(frozen_until) = profile.frozen_until {
                if frozen_until > chrono::Utc::now() {
                    return Err(RoomServiceError::AccountFrozen);
                }
            }

            if profile.credit < required_credit {
                return Err(RoomServiceError::InsufficientCredits {
                    required: required_credit,
                    current: profile.credit,
                });
            }
        }
        Ok(())
    }

    pub async fn create_run(
        &self,
        room_id: Uuid,
        user_id: Uuid,
        num_games: i32,
        bet: i32,
        player_ids: &[Uuid],
    ) -> Result<CreateRunResponse, RoomServiceError> {
        let player_ids = self
            .validate_create_run(room_id, user_id, num_games, bet, player_ids)
            .await?;

        let total_cost = num_games * bet;
        let required_credit = total_cost * KORA_CREDIT_MULTIPLIER;
        self.validate_players_credit(&player_ids, required_credit)
            .await?;

        let txn = self.txn_runner.begin().await?;

        let run_id = Uuid::now_v7();
        let now = chrono::Utc::now();
        self.run_repo
            .create_in_txn(&txn, run_id, room_id, user_id, num_games, bet, now)
            .await?;

        for (i, &p_id) in player_ids.iter().enumerate() {
            let rows = self
                .profile_repo
                .debit_in_txn(&txn, p_id, total_cost, now)
                .await?;
            if rows == 0 {
                let current = self
                    .profile_repo
                    .find_by_user_id_in_txn(&txn, p_id)
                    .await?
                    .map(|p| p.credit)
                    .unwrap_or(0);
                txn.rollback().await.ok();
                return Err(RoomServiceError::InsufficientCredits {
                    required: total_cost,
                    current,
                });
            }
            self.run_player_repo
                .create_in_txn(&txn, run_id, p_id, i as i32, total_cost, now)
                .await?;
        }

        let num_players = player_ids.len() as i32;
        self.run_repo
            .update_num_players_in_txn(&txn, run_id, num_players, now)
            .await?;

        self.txn_runner.clone().commit(txn).await?;

        self.run_event_repo
            .log(run_id, Some(user_id), "run_created", None)
            .await?;

        self.publish_event(&RoomEvent::RunCreated {
            room_id,
            run_id,
            num_games,
            bet_per_game: bet,
        })
        .await;

        Ok(CreateRunResponse {
            run_id,
            room_id,
            num_games,
            bet_per_game: bet,
            num_players,
            status: "active".to_string(),
        })
    }

    pub async fn join_run(
        &self,
        run_id: Uuid,
        user_id: Uuid,
    ) -> Result<JoinRunResponse, RoomServiceError> {
        let run = self
            .run_repo
            .find_by_id(run_id)
            .await?
            .ok_or(RoomServiceError::RunNotFound)?;

        if run.status != RunStatus::Active {
            return Err(RoomServiceError::RunAlreadyActive);
        }

        if self
            .member_repo
            .find_membership(run.room_id, user_id)
            .await?
            .is_none()
        {
            return Err(RoomServiceError::NotMember);
        }

        if self
            .run_player_repo
            .find_by_run_and_user(run_id, user_id)
            .await?
            .is_some()
        {
            return Err(RoomServiceError::AlreadyMember);
        }

        let existing_players = self.run_player_repo.list_all_by_run(run_id).await?;
        if existing_players.len() as i32 >= self.config.room_max_players {
            return Err(RoomServiceError::TooManyPlayers {
                max: self.config.room_max_players,
            });
        }

        let total_cost = run.num_games * run.bet_per_game;
        let required_credit = total_cost * KORA_CREDIT_MULTIPLIER;

        let profile = self
            .profile_repo
            .find_by_user_id(user_id)
            .await?
            .ok_or(RoomServiceError::ProfileNotFound)?;

        if let Some(frozen_until) = profile.frozen_until {
            if frozen_until > chrono::Utc::now() {
                return Err(RoomServiceError::AccountFrozen);
            }
        }

        if profile.credit < required_credit {
            return Err(RoomServiceError::InsufficientCredits {
                required: required_credit,
                current: profile.credit,
            });
        }

        let txn = self.txn_runner.begin().await?;

        let now = chrono::Utc::now();
        let rows = self
            .profile_repo
            .debit_in_txn(&txn, user_id, total_cost, now)
            .await?;
        if rows == 0 {
            let current = self
                .profile_repo
                .find_by_user_id_in_txn(&txn, user_id)
                .await?
                .map(|p| p.credit)
                .unwrap_or(0);
            txn.rollback().await.ok();
            return Err(RoomServiceError::InsufficientCredits {
                required: total_cost,
                current,
            });
        }

        let position = existing_players.len() as i32 + 1;
        self.run_player_repo
            .create_in_txn(&txn, run_id, user_id, position, total_cost, now)
            .await?;

        self.txn_runner.clone().commit(txn).await?;

        self.run_event_repo
            .log(run_id, Some(user_id), "player_joined_run", None)
            .await?;

        let new_credit = profile.credit - total_cost;
        Ok(JoinRunResponse {
            run_id,
            provisioned_credits: total_cost,
            profile_credit_remaining: new_credit,
        })
    }

    pub async fn leave_run(&self, run_id: Uuid, user_id: Uuid) -> Result<(), RoomServiceError> {
        let run = self
            .run_repo
            .find_by_id(run_id)
            .await?
            .ok_or(RoomServiceError::RunNotFound)?;

        let run_player = self
            .run_player_repo
            .find_by_run_and_user(run_id, user_id)
            .await?
            .ok_or(RoomServiceError::NotRunPlayer)?;

        if run.current_game_index > 0 {
            if let Some(run_game) = self
                .run_game_repo
                .find_by_run_and_index(run_id, run.current_game_index - 1)
                .await?
            {
                if self
                    .game_repo
                    .find_active_by_id(run_game.game_id)
                    .await?
                    .is_some()
                    && self
                        .player_repo
                        .find_active_player_in_game(run_game.game_id, user_id)
                        .await?
                        .is_some()
                {
                    return Err(RoomServiceError::GameInProgress);
                }
            }
        }

        if run_player.provisioned_credits > 0 {
            let txn = self.txn_runner.begin().await?;

            self.run_player_repo
                .delete_in_txn(&txn, run_id, user_id)
                .await?;

            let now = chrono::Utc::now();
            self.profile_repo
                .credit_in_txn(&txn, user_id, run_player.provisioned_credits, now)
                .await?;

            self.txn_runner.clone().commit(txn).await?;
        } else {
            self.run_player_repo.remove(run_id, user_id).await?;
        }

        self.run_event_repo
            .log(run_id, Some(user_id), "player_left_run", None)
            .await?;

        let remaining = self.run_player_repo.list_all_by_run(run_id).await?;
        if remaining.len() < 2 {
            self.run_repo
                .update_status(run_id, RunStatus::Cancelled)
                .await?;
        }

        Ok(())
    }

    pub async fn get_active_run(
        &self,
        room_id: Uuid,
        user_id: Uuid,
    ) -> Result<ActiveRunResponse, RoomServiceError> {
        let member = self.member_repo.find_membership(room_id, user_id).await?;
        if member.is_none() {
            return Err(RoomServiceError::NotMember);
        }

        let run = self
            .run_repo
            .find_active_by_room(room_id)
            .await?
            .ok_or(RoomServiceError::RunNotFound)?;

        let players = self.run_player_repo.list_by_run(run.id).await?;

        let player_user_ids: Vec<Uuid> = players.iter().map(|p| p.user_id).collect();
        let users = if player_user_ids.is_empty() {
            Vec::new()
        } else {
            self.user_repo
                .find_by_ids(&player_user_ids)
                .await
                .ok()
                .unwrap_or_default()
        };
        let user_map: HashMap<Uuid, String> = users.into_iter().map(|u| (u.id, u.pseudo)).collect();

        let games = self.run_game_repo.list_by_run(run.id).await?;

        let players = players
            .iter()
            .map(|rp| ActiveRunPlayerInfo {
                user_id: rp.user_id,
                pseudo: user_map.get(&rp.user_id).cloned().unwrap_or_default(),
                position: rp.position,
                provisioned_credits: rp.provisioned_credits,
                kicked: rp.kicked,
            })
            .collect();

        let games = games
            .iter()
            .map(|g| ActiveRunGameInfo {
                game_id: g.game_id,
                game_index: g.game_index,
                status: g.status,
            })
            .collect();

        Ok(ActiveRunResponse {
            id: run.id,
            room_id: run.room_id,
            num_games: run.num_games,
            bet_per_game: run.bet_per_game,
            current_game_index: run.current_game_index,
            status: run.status,
            players,
            games,
        })
    }
}
