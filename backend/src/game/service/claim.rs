use sea_orm::TransactionTrait;
use uuid::Uuid;

use crate::database::models::{GameStatus, PlayerType};
use crate::database::repositories::{GameCardRepository, GameRepository, PlayerRepository};
use crate::error::GameError;
use crate::game::service::types::RoundEvaluationResult;
use crate::game::special_cards::compute_special_cards;
use crate::observability::metrics;

use super::GameService;

/// Data produced by a successful special-card claim, consumed for event
/// publishing once the transaction has committed.
pub(crate) struct ClaimOutcome {
    pub(crate) winner_id: Uuid,
    pub(crate) winner_position: i32,
}

impl GameService {
    /// Claim victory through a held special-card combination. The game must be
    /// in the claim-pending state for this player; claiming ends the game as a
    /// regular (`finished`) win and settles the pot inline.
    #[tracing::instrument(skip(self), fields(game_id = %game_id, claimant = %claimant_id))]
    pub(crate) async fn claim_special_victory(
        &self,
        game_id: Uuid,
        claimant_id: Uuid,
    ) -> Result<ClaimOutcome, GameError> {
        let txn = self.db.begin().await?;

        let game_repo = GameRepository::new(self.db.clone());
        let player_repo = PlayerRepository::new(self.db.clone());
        let card_repo = GameCardRepository::new(self.db.clone());

        let game = game_repo
            .find_by_id_in_txn(&txn, game_id)
            .await?
            .ok_or(GameError::GameNotFound)?;

        if matches!(
            game.status,
            GameStatus::Finished | GameStatus::Kora | GameStatus::DoubleKora
        ) {
            txn.rollback().await.ok();
            return Err(GameError::GameFinished);
        }

        if game.pending_claim_player_id != Some(claimant_id) {
            txn.rollback().await.ok();
            return Err(GameError::SpecialClaimNotAllowed);
        }

        let players = player_repo.list_by_game_in_txn(&txn, game_id).await?;
        if players
            .iter()
            .any(|p| matches!(p.player_type, PlayerType::Bot))
        {
            txn.rollback().await.ok();
            return Err(GameError::SpecialClaimNotAllowed);
        }

        let claimant_cards = card_repo.list_by_player_in_txn(&txn, claimant_id).await?;
        let hand: Vec<i32> = claimant_cards.iter().map(|c| c.card_index).collect();
        if !compute_special_cards(&hand).has_any() {
            txn.rollback().await.ok();
            return Err(GameError::SpecialClaimNotAllowed);
        }

        let winner_position = players
            .iter()
            .position(|p| p.id == claimant_id)
            .ok_or_else(|| GameError::internal("Claimant not found in player list"))?;

        let now = chrono::Utc::now();
        let rows = game_repo
            .finish_by_claim_in_txn(&txn, game_id, claimant_id, now)
            .await?;
        if rows == 0 {
            txn.rollback().await.ok();
            return Err(GameError::GameFinished);
        }

        let result = RoundEvaluationResult {
            round: game.roll,
            winner_id: claimant_id,
            winner_position,
            game_ended: true,
            final_status: GameStatus::Finished,
            players: players.clone(),
        };

        self.process_payment_in_txn(&txn, game_id, &result).await?;

        txn.commit().await?;

        metrics::ACTIVE_GAMES.dec();
        metrics::GAMES_FINISHED_TOTAL
            .with_label_values(&["finished"])
            .inc();

        self.publish_special_claim(game_id, claimant_id, hand, winner_position as i32)
            .await;
        self.publish_game_finished(game_id, &result, None).await;
        self.invalidate_game_state_cache(game_id).await;

        let user_ids: Vec<Uuid> = players.iter().filter_map(|p| p.user_id).collect();
        if !user_ids.is_empty() {
            self.invalidate_dashboard_caches(&user_ids).await;
        }

        Ok(ClaimOutcome {
            winner_id: claimant_id,
            winner_position: winner_position as i32,
        })
    }

    /// Decline the pending special-card claim. Returns `true` when the claim
    /// was actually cleared by this call (the game may then proceed normally).
    #[tracing::instrument(skip(self), fields(game_id = %game_id, player_id = %player_id))]
    pub async fn decline_special_claim(
        &self,
        game_id: Uuid,
        player_id: Uuid,
    ) -> Result<bool, GameError> {
        let txn = self.db.begin().await?;
        let game_repo = GameRepository::new(self.db.clone());
        let rows = game_repo
            .clear_pending_claim_in_txn(&txn, game_id, player_id)
            .await?;
        txn.commit().await?;

        if rows > 0 {
            self.publish_claim_resolved(game_id).await;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}
