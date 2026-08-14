use std::collections::HashMap;
use uuid::Uuid;

use sea_orm::{ActiveValue, ColumnTrait, EntityTrait, QueryFilter, Set, TransactionTrait};

use crate::database::models::{
    game, game_run, game_run_player, player, player_profile, GameStatus, RunStatus,
};
use crate::game::constants::KORA_CREDIT_MULTIPLIER;
use crate::messaging::events::RoomEvent;
use crate::room::error::RoomServiceError;
use crate::room::service::RoomService;

// TODO: queries here into repositories for easy tracing
impl RoomService {
    // TODO: respect SOLID principles
    pub async fn create_run(
        &self,
        room_id: Uuid,
        user_id: Uuid,
        num_games: i32,
        bet: i32,
        player_ids: &[Uuid],
    ) -> Result<serde_json::Value, RoomServiceError> {
        let _room = self
            .room_repo
            .find_by_id(room_id)
            .await?
            .ok_or(RoomServiceError::RoomNotFound)?;

        let member = self.member_repo.find_membership(room_id, user_id).await?;
        if member.is_none() {
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

        let total_cost = num_games * bet;
        let required_credit = total_cost * KORA_CREDIT_MULTIPLIER;

        let profiles = self.profile_repo.find_by_user_ids(&player_ids).await?;

        let profile_map: HashMap<Uuid, crate::database::models::PlayerProfile> =
            profiles.into_iter().map(|p| (p.user_id, p)).collect();

        for &p_id in &player_ids {
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

        // TODO: refactor with transaction_runner
        let txn = self.db.begin().await?;

        let run_id = Uuid::now_v7();
        let now = chrono::Utc::now();
        let run_active = game_run::ActiveModel {
            id: Set(run_id),
            room_id: Set(room_id),
            num_games: Set(num_games),
            bet_per_game: Set(bet),
            num_players: Set(0),
            current_game_index: Set(0),
            status: Set(RunStatus::Active),
            created_by: Set(user_id),
            next_game_auto_start_at: ActiveValue::NotSet,
            stall_warning_sent_at: ActiveValue::NotSet,
            stall_cancelled_at: ActiveValue::NotSet,
            created_at: Set(now),
            updated_at: Set(now),
        };
        game_run::Entity::insert(run_active).exec(&txn).await?;

        for (i, &p_id) in player_ids.iter().enumerate() {
            let current_credit = profile_map.get(&p_id).unwrap().credit;
            let new_credit = current_credit - total_cost;

            player_profile::Entity::update_many()
                .col_expr(
                    player_profile::Column::Credit,
                    sea_orm::sea_query::Expr::value(new_credit),
                )
                .col_expr(
                    player_profile::Column::UpdatedAt,
                    sea_orm::sea_query::Expr::value(now),
                )
                .filter(player_profile::Column::UserId.eq(p_id))
                .exec(&txn)
                .await?;

            let rp_id = Uuid::now_v7();
            game_run_player::Entity::insert(game_run_player::ActiveModel {
                id: Set(rp_id),
                game_run_id: Set(run_id),
                user_id: Set(p_id),
                position: Set(i as i32),
                provisioned_credits: Set(total_cost),
                kicked: Set(false),
                joined_at: Set(now),
            })
            .exec(&txn)
            .await?;
        }

        let num_players = player_ids.len() as i32;
        game_run::Entity::update_many()
            .col_expr(
                game_run::Column::NumPlayers,
                sea_orm::sea_query::Expr::value(num_players),
            )
            .col_expr(
                game_run::Column::UpdatedAt,
                sea_orm::sea_query::Expr::value(now),
            )
            .filter(game_run::Column::Id.eq(run_id))
            .exec(&txn)
            .await?;

        txn.commit().await?;

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

        // TODO: create a real DTO
        Ok(serde_json::json!({
            "run_id": run_id,
            "room_id": room_id,
            "num_games": num_games,
            "bet_per_game": bet,
            "num_players": num_players,
            "status": "active",
        }))
    }

    pub async fn join_run(
        &self,
        run_id: Uuid,
        user_id: Uuid,
    ) -> Result<serde_json::Value, RoomServiceError> {
        let run = self
            .run_repo
            .find_by_id(run_id)
            .await?
            .ok_or(RoomServiceError::RunNotFound)?;

        if run.status != RunStatus::Active {
            return Err(RoomServiceError::RunAlreadyActive);
        }

        let member = self
            .member_repo
            .find_membership(run.room_id, user_id)
            .await?;
        if member.is_none() {
            return Err(RoomServiceError::NotMember);
        }

        let existing = self
            .run_player_repo
            .find_by_run_and_user(run_id, user_id)
            .await?;
        if existing.is_some() {
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

        let txn = self.db.begin().await?;

        let current_credit = profile.credit;
        let new_credit = current_credit - total_cost;
        let now = chrono::Utc::now();

        player_profile::Entity::update_many()
            .col_expr(
                player_profile::Column::Credit,
                sea_orm::sea_query::Expr::value(new_credit),
            )
            .col_expr(
                player_profile::Column::UpdatedAt,
                sea_orm::sea_query::Expr::value(now),
            )
            .filter(player_profile::Column::UserId.eq(user_id))
            .exec(&txn)
            .await?;

        let position = existing_players.len() as i32 + 1;

        game_run_player::Entity::insert(game_run_player::ActiveModel {
            id: Set(Uuid::now_v7()),
            game_run_id: Set(run_id),
            user_id: Set(user_id),
            position: Set(position),
            provisioned_credits: Set(total_cost),
            kicked: Set(false),
            joined_at: Set(now),
        })
        .exec(&txn)
        .await?;

        txn.commit().await?;

        self.run_event_repo
            .log(run_id, Some(user_id), "player_joined_run", None)
            .await?;

        Ok(serde_json::json!({
            "run_id": run_id,
            "provisioned_credits": total_cost,
            "profile_credit_remaining": new_credit,
        }))
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
                let active_game = game::Entity::find_by_id(run_game.game_id)
                    .filter(game::Column::Status.eq(GameStatus::Active))
                    .one(&self.db)
                    .await?;
                if active_game.is_some() {
                    let has_player = player::Entity::find()
                        .filter(player::Column::GameId.eq(run_game.game_id))
                        .filter(player::Column::UserId.eq(user_id))
                        .filter(player::Column::Kicked.eq(false))
                        .one(&self.db)
                        .await?;
                    if has_player.is_some() {
                        return Err(RoomServiceError::GameInProgress);
                    }
                }
            }
        }

        if run_player.provisioned_credits > 0 {
            let txn = self.db.begin().await?;

            game_run_player::Entity::delete_many()
                .filter(game_run_player::Column::GameRunId.eq(run_id))
                .filter(game_run_player::Column::UserId.eq(user_id))
                .exec(&txn)
                .await?;

            let profile = self
                .profile_repo
                .find_by_user_id(user_id)
                .await?
                .ok_or_else(|| RoomServiceError::ProfileNotFound)?;

            let current_credit = profile.credit;
            let now = chrono::Utc::now();
            player_profile::Entity::update_many()
                .col_expr(
                    player_profile::Column::Credit,
                    sea_orm::sea_query::Expr::value(
                        current_credit + run_player.provisioned_credits,
                    ),
                )
                .col_expr(
                    player_profile::Column::UpdatedAt,
                    sea_orm::sea_query::Expr::value(now),
                )
                .filter(player_profile::Column::UserId.eq(user_id))
                .exec(&txn)
                .await?;

            txn.commit().await?;
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
    ) -> Result<serde_json::Value, RoomServiceError> {
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

        let mut player_infos = Vec::new();
        for rp in &players {
            player_infos.push(serde_json::json!({
                "user_id": rp.user_id,
                "pseudo": user_map.get(&rp.user_id).cloned().unwrap_or_default(),
                "position": rp.position,
                "provisioned_credits": rp.provisioned_credits,
                "kicked": rp.kicked,
            }));
        }

        Ok(serde_json::json!({
            "id": run.id,
            "room_id": run.room_id,
            "num_games": run.num_games,
            "bet_per_game": run.bet_per_game,
            "current_game_index": run.current_game_index,
            "status": run.status,
            "players": player_infos,
            "games": games.iter().map(|g| serde_json::json!({
                "game_id": g.game_id,
                "game_index": g.game_index,
                "status": g.status,
            })).collect::<Vec<_>>(),
        }))
    }
}
