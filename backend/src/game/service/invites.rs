use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter,
    TransactionTrait,
};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{error, info};
use uuid::Uuid;

use crate::database::models::{
    game, game_invite, player, player_profile, GameStatus, InviteStatus, PlayerType,
};
use crate::error::GameError;
use crate::game::constants::KORA_CREDIT_MULTIPLIER;
use crate::messaging::events::GameEvent;
use crate::messaging::redis::PublishResult;

use super::is_unique_violation;
use super::GameService;

impl GameService {
    /// Acquire a per-game mutex to serialize concurrent accept_invite calls.
    pub(crate) async fn accept_invite_lock(
        &self,
        game_id: Uuid,
    ) -> tokio::sync::OwnedMutexGuard<()> {
        let arc_lock: Arc<tokio::sync::Mutex<()>> = {
            let mut locks = self.accept_invite_locks.lock().await;
            locks
                .entry(game_id)
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
                .clone()
        };
        arc_lock.lock_owned().await
    }

    pub async fn send_invites(
        &self,
        game_id: Uuid,
        creator_user_id: Uuid,
        invited_user_ids: &[Uuid],
    ) -> Result<(), GameError> {
        let game = game::Entity::find_by_id(game_id)
            .one(&self.db)
            .await?
            .ok_or(GameError::GameNotFound)?;

        if game.status != GameStatus::Pending {
            return Err(GameError::GameNotPending);
        }
        if game.creator_id != Some(creator_user_id) {
            return Err(GameError::NotCreator);
        }

        let invite_repo = crate::database::repositories::GameInviteRepository::new(self.db.clone());
        for &user_id in invited_user_ids {
            if user_id == creator_user_id {
                continue;
            }
            if crate::database::repositories::PlayerRepository::new(self.db.clone())
                .find_by_game_and_user(game_id, user_id)
                .await?
                .is_some()
            {
                continue;
            }
            let existing = invite_repo.find_invite(game_id, user_id).await?;
            if existing.is_none() {
                invite_repo.create_invite(game_id, user_id).await?;
            }
        }
        Ok(())
    }

    pub async fn accept_invite(
        &self,
        game_id: Uuid,
        user_id: Uuid,
        user_pseudo: &str,
    ) -> Result<crate::database::models::player::Model, GameError> {
        let _guard = self.accept_invite_lock(game_id).await;

        let txn = self.db.begin().await?;

        let game_model = game::Entity::find_by_id(game_id)
            .one(&txn)
            .await?
            .ok_or(GameError::GameNotFound)?;

        if game_model.status != GameStatus::Pending {
            txn.rollback().await.ok();
            return Err(GameError::GameNotPending);
        }
        if Some(user_id) == game_model.creator_id {
            txn.rollback().await.ok();
            return Err(GameError::CreatorCannotJoin);
        }

        let existing_player = player::Entity::find()
            .filter(player::Column::GameId.eq(game_id))
            .filter(player::Column::UserId.eq(user_id))
            .one(&txn)
            .await?;
        if existing_player.is_some() {
            txn.rollback().await.ok();
            return Err(GameError::AlreadyJoined);
        }

        let invite = game_invite::Entity::find()
            .filter(game_invite::Column::GameId.eq(game_id))
            .filter(game_invite::Column::InvitedUserId.eq(user_id))
            .filter(game_invite::Column::Status.eq(InviteStatus::Pending))
            .one(&txn)
            .await?
            .ok_or(GameError::NotInvited)?;

        let player_count: u64 = player::Entity::find()
            .filter(player::Column::GameId.eq(game_id))
            .count(&txn)
            .await?;

        if player_count >= game_model.max_players as u64 {
            txn.rollback().await.ok();
            return Err(GameError::GameFull);
        }

        let next_position = player_count as i32;
        let max_players_val = game_model.max_players;
        let bet = game_model.bet;

        let profile = player_profile::Entity::find()
            .filter(player_profile::Column::UserId.eq(user_id))
            .one(&txn)
            .await?
            .ok_or_else(|| GameError::ProfileNotFound)?;

        if let Some(frozen_until) = profile.frozen_until {
            if frozen_until > chrono::Utc::now() {
                txn.rollback().await.ok();
                return Err(GameError::AccountFrozen {
                    until: frozen_until.to_rfc3339(),
                });
            }
        }

        let required_credit = bet * KORA_CREDIT_MULTIPLIER;
        if profile.credit < required_credit {
            txn.rollback().await.ok();
            return Err(GameError::InsufficientCredits {
                required: required_credit,
                current: profile.credit,
            });
        }

        let now = chrono::Utc::now();
        let new_credit = profile.credit - bet;
        let freeze_duration = chrono::Duration::seconds(self.freeze_duration_secs as i64);
        let was_previously_frozen = profile.frozen_until.is_some();

        let (final_credit, frozen_until_val) = if new_credit <= 0 {
            (new_credit, Some(now + freeze_duration))
        } else if was_previously_frozen {
            let auto_unfreeze_credit = if new_credit < self.unfreeze_credit_no_payment {
                self.unfreeze_credit_no_payment
            } else {
                new_credit
            };
            (auto_unfreeze_credit, None)
        } else {
            (new_credit, profile.frozen_until)
        };
        let mut profile_active: player_profile::ActiveModel = profile.into();
        profile_active.credit = ActiveValue::Set(final_credit);
        profile_active.frozen_until = ActiveValue::Set(frozen_until_val);
        profile_active.updated_at = ActiveValue::Set(now);
        profile_active.update(&txn).await?;

        let new_player_id = Uuid::now_v7();
        let player_active = player::ActiveModel {
            id: ActiveValue::Set(new_player_id),
            game_id: ActiveValue::Set(game_id),
            player_type: ActiveValue::Set(PlayerType::Human),
            name: ActiveValue::Set(user_pseudo.to_string()),
            position: ActiveValue::Set(next_position),
            credits: ActiveValue::Set(final_credit),
            created_at: ActiveValue::Set(now),
            user_id: ActiveValue::Set(Some(user_id)),
            kicked: ActiveValue::Set(false),
            kicked_at: ActiveValue::NotSet,
        };
        if let Err(e) = player::Entity::insert(player_active).exec(&txn).await {
            txn.rollback().await.ok();
            if is_unique_violation(&e) {
                return Err(GameError::AlreadyJoined);
            }
            return Err(GameError::Database(e));
        }

        let mut invite_active: game_invite::ActiveModel = invite.into();
        invite_active.status = ActiveValue::Set(InviteStatus::Accepted);
        invite_active.update(&txn).await?;

        let current_positions: HashMap<i32, Uuid> = if game_model.player_positions.is_null() {
            HashMap::new()
        } else {
            serde_json::from_value(game_model.player_positions.clone()).map_err(|e| {
                GameError::internal(format!("Failed to parse player_positions: {}", e))
            })?
        };
        let mut updated_positions = current_positions;
        updated_positions.insert(next_position, user_id);

        let new_status = if (player_count + 1) >= max_players_val as u64 {
            GameStatus::Ready
        } else {
            GameStatus::Pending
        };

        let mut game_active: game::ActiveModel = game_model.into();
        game_active.player_positions =
            ActiveValue::Set(serde_json::to_value(&updated_positions).map_err(|e| {
                GameError::internal(format!("Failed to serialize player_positions: {}", e))
            })?);
        game_active.status = ActiveValue::Set(new_status);
        game_active.updated_at = ActiveValue::Set(now);
        game_active.update(&txn).await?;

        txn.commit().await?;

        if let Some(ref redis) = self.redis_client {
            let event = GameEvent::PlayerJoined {
                game_id,
                player_id: new_player_id,
                user_id,
                pseudo: user_pseudo.to_string(),
                position: next_position,
                player_count: (player_count + 1) as i32,
                max_players: max_players_val as i32,
            };
            match redis.clone().publish_game_event_with_retry(&event).await {
                PublishResult::Published => {}
                PublishResult::RetryExhausted(e) => {
                    error!("Failed to publish PlayerJoined event: {}", e);
                }
            }
            if new_status == GameStatus::Ready {
                let event = GameEvent::GameReady {
                    game_id,
                    correlation_id: None,
                };
                match redis.clone().publish_game_event_with_retry(&event).await {
                    PublishResult::Published => {}
                    PublishResult::RetryExhausted(e) => {
                        error!("Failed to publish GameReady event: {}", e);
                    }
                }
            }
        }

        self.invalidate_dashboard_caches(&[user_id]).await;

        player::Entity::find_by_id(new_player_id)
            .one(&self.db)
            .await?
            .ok_or(GameError::PlayerNotFound)
    }

    pub async fn decline_invite(&self, game_id: Uuid, user_id: Uuid) -> Result<(), GameError> {
        let invite_repo = crate::database::repositories::GameInviteRepository::new(self.db.clone());

        let invite = invite_repo
            .find_invite(game_id, user_id)
            .await?
            .ok_or(GameError::NotInvited)?;

        if invite.status != InviteStatus::Pending {
            return Err(GameError::GameNotPending);
        }

        let game = game::Entity::find_by_id(game_id)
            .one(&self.db)
            .await?
            .ok_or(GameError::GameNotFound)?;

        if game.status != GameStatus::Pending {
            return Err(GameError::GameNotPending);
        }

        invite_repo
            .update_invite_status(invite.id, InviteStatus::Declined)
            .await?;

        info!("User {} declined invite for game {}", user_id, game_id);
        Ok(())
    }
}
