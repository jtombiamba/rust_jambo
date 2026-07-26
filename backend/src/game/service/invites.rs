use sea_orm::EntityTrait;
use std::sync::Arc;
use tracing::info;
use uuid::Uuid;

use crate::database::models::{game, GameStatus, InviteStatus};
use crate::error::GameError;

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

        let orchestrator = AcceptInviteOrchestrator::new(
            self.db.clone(),
            self.freeze_duration_secs,
            self.unfreeze_credit_no_payment,
            self.redis_client.clone(),
        );

        let player = orchestrator.execute(game_id, user_id, user_pseudo).await?;

        self.invalidate_dashboard_caches(&[user_id]).await;

        Ok(player)
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
