use sea_orm::EntityTrait;
use std::sync::Arc;
use tracing::{error, info};
use uuid::Uuid;

use crate::database::models::{game, user, GameStatus, InviteStatus};
use crate::database::repositories::{GameInviteRepository, PlayerRepository};
use crate::error::GameError;
use crate::messaging::events::UserEvent;
use crate::messaging::redis::PublishResult;

use super::invite_acceptance::AcceptInviteOrchestrator;
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

        let invite_repo = GameInviteRepository::new(self.db.clone());
        let player_repo = PlayerRepository::new(self.db.clone());
        let player_count = player_repo.list_by_game(game_id).await?.len() as i64;
        let creator_pseudo = user::Entity::find_by_id(creator_user_id)
            .one(&self.db)
            .await?
            .map(|u| u.pseudo)
            .unwrap_or_else(|| "Unknown".to_string());

        for &user_id in invited_user_ids {
            if user_id == creator_user_id {
                continue;
            }
            if player_repo
                .find_by_game_and_user(game_id, user_id)
                .await?
                .is_some()
            {
                continue;
            }
            let existing = invite_repo.find_invite(game_id, user_id).await?;
            if existing.is_none() {
                let invite = invite_repo.create_invite(game_id, user_id).await?;
                self.publish_invite_received(
                    user_id,
                    invite.id,
                    game_id,
                    &creator_pseudo,
                    game.bet,
                    player_count,
                    game.max_players as i32,
                    &game,
                )
                .await;
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

        let orchestrator = AcceptInviteOrchestrator::new(
            self.db.clone(),
            self.freeze_duration_secs,
            self.unfreeze_credit_no_payment,
            self.redis_client.clone(),
        );

        let player = orchestrator.execute(game_id, user_id, user_pseudo).await?;

        self.invalidate_dashboard_caches(&[user_id]).await;
        self.publish_invite_removed(user_id, game_id).await;

        Ok(player)
    }

    pub async fn decline_invite(&self, game_id: Uuid, user_id: Uuid) -> Result<(), GameError> {
        let invite_repo = GameInviteRepository::new(self.db.clone());

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
        self.publish_invite_removed(user_id, game_id).await;

        Ok(())
    }

    /// Push a real-time notification to the invited user over the user-scoped
    /// WebSocket channel.
    #[allow(clippy::too_many_arguments)]
    async fn publish_invite_received(
        &self,
        user_id: Uuid,
        invite_id: Uuid,
        game_id: Uuid,
        creator_pseudo: &str,
        bet: i32,
        player_count: i64,
        max_players: i32,
        game: &game::Model,
    ) {
        let Some(mut redis) = self.redis_client.clone() else {
            return;
        };

        let event = UserEvent::InviteReceived {
            user_id,
            invite_id,
            game_id,
            creator_pseudo: creator_pseudo.to_string(),
            bet,
            player_count,
            max_players,
            created_at: chrono::Utc::now().to_rfc3339(),
            expires_at: game.invite_expires_at.map(|t| t.to_rfc3339()),
        };

        match redis.publish_user_event_with_retry(&event).await {
            PublishResult::Published => {}
            PublishResult::RetryExhausted(e) => {
                error!(
                    "Failed to publish InviteReceived event for user {} after retries: {}",
                    user_id, e
                );
            }
        }
    }

    /// Notify the user that an invitation is no longer valid (accepted or
    /// declined) so any open client can remove it from the pending list.
    async fn publish_invite_removed(&self, user_id: Uuid, game_id: Uuid) {
        let Some(mut redis) = self.redis_client.clone() else {
            return;
        };

        let event = UserEvent::InviteRemoved { user_id, game_id };

        match redis.publish_user_event_with_retry(&event).await {
            PublishResult::Published => {}
            PublishResult::RetryExhausted(e) => {
                error!(
                    "Failed to publish InviteRemoved event for user {} after retries: {}",
                    user_id, e
                );
            }
        }
    }
}
