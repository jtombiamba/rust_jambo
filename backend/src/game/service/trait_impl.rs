use async_trait::async_trait;

use sea_orm::DatabaseConnection;
use uuid::Uuid;

use crate::database::models::{GameStatus, PlayerType};
use crate::database::repositories::{GameRepository, PlayerRepository};
use crate::error::GameError;
use crate::game::service::idempotency::IdempotencyGuard;
use crate::game::service::types::{
    AcceptInviteOutcome, AdvanceBotOutcome, BenchmarkCleanupCounts, BenchmarkGameOutcome,
    BenchmarkService, EvaluateRoundOutcome, GameLifecycleService, GamePlayService,
    GameServiceTrait, InviteService, MultiplayerCreationOutcome, PlayCardOutcome, QuickGameOutcome,
};
use crate::observability::CorrelationId;

use super::GameService;

// ── GamePlayService ────────────────────────────────────────────────────

#[async_trait]
impl GamePlayService for GameService {
    #[tracing::instrument(level = "info", skip(self), fields(correlation_id = %correlation_id.map(|c| c.to_string()).unwrap_or_default(), game_id = %game_id, player_id = %player_id, card_index = card_index))]
    async fn play_card(
        &self,
        game_id: Uuid,
        player_id: Uuid,
        card_index: i32,
        correlation_id: Option<CorrelationId>,
        idempotency_key: Option<String>,
    ) -> Result<PlayCardOutcome, GameError> {
        let idem_redis_key = idempotency_key
            .as_ref()
            .map(|k| format!("idem:{}:{}", player_id, k));

        let mut idem_guard =
            if let (Some(ref idem_key), Some(redis)) = (&idem_redis_key, self.redis_client()) {
                let mut guard = IdempotencyGuard::new(redis, idem_key.clone());
                match guard.acquire::<PlayCardOutcome>().await? {
                    Some(cached) => return Ok(cached),
                    None => Some(guard),
                }
            } else {
                None
            };

        let result = match self
            .update_card_play(game_id, player_id, card_index, correlation_id.map(|c| c.0))
            .await
        {
            Ok(r) => r,
            Err(e) => {
                if let Some(ref mut g) = idem_guard {
                    g.release().await;
                }
                return Err(e);
            }
        };

        let next_is_bot = result
            .players
            .iter()
            .find(|p| p.id == result.next_player_id)
            .map(|p| matches!(p.player_type, PlayerType::Bot))
            .unwrap_or(false);

        if !result.game_ended && next_is_bot && !result.step_by_step {
            if let Some(ref bs) = self.bot_scheduler {
                bs.schedule_if_next_bot(game_id, result.next_player_id, correlation_id)
                    .await;
            } else {
                tracing::warn!(
                    "Bot scheduler unavailable: next player {} is bot but no scheduler configured, game {} may stall",
                    result.next_player_id,
                    game_id
                );
            }
        }

        let next_turn = if result.game_ended {
            None
        } else {
            Some(result.next_player_id)
        };

        let outcome = PlayCardOutcome {
            card_id: result.card.id,
            next_turn,
            game_ended: result.game_ended,
            round_completed: result.round_completed,
            current_round: result.current_round,
        };

        if let Some(ref mut g) = idem_guard {
            g.complete(&outcome).await;
        }

        Ok(outcome)
    }

    async fn advance_bot(
        &self,
        game_id: Uuid,
        human_player_id: Uuid,
    ) -> Result<AdvanceBotOutcome, GameError> {
        let outcome = self.advance_bot(game_id, human_player_id).await?;

        let next_is_bot = outcome
            .players
            .iter()
            .find(|p| p.id == outcome.next_player_id)
            .map(|p| matches!(p.player_type, PlayerType::Bot))
            .unwrap_or(false);

        Ok(AdvanceBotOutcome {
            card_played: outcome.card_played,
            next_player_id: outcome.next_player_id,
            next_is_bot,
            round_complete: outcome.round_complete,
            game_ended: outcome.game_ended,
        })
    }

    async fn evaluate_round(
        &self,
        game_id: Uuid,
        human_player_id: Uuid,
        idempotency_key: Option<String>,
    ) -> Result<EvaluateRoundOutcome, GameError> {
        let idem_redis_key = idempotency_key
            .as_ref()
            .map(|k| format!("idem:eval:{}:{}", game_id, k));

        let mut idem_guard =
            if let (Some(ref idem_key), Some(redis)) = (&idem_redis_key, self.redis_client()) {
                let mut guard = IdempotencyGuard::new(redis, idem_key.clone());
                match guard.acquire::<EvaluateRoundOutcome>().await? {
                    Some(cached) => return Ok(cached),
                    None => Some(guard),
                }
            } else {
                None
            };

        let game = GameRepository::new(self.db.clone())
            .find_by_id(game_id)
            .await?
            .ok_or(GameError::GameNotFound)?;

        let players = PlayerRepository::new(self.db.clone())
            .list_by_game(game_id)
            .await?;

        if !players.iter().any(|p| p.id == human_player_id) {
            return Err(GameError::PlayerNotFound);
        }

        if !game.step_by_step {
            return Err(GameError::StepByStepOnly);
        }

        let result = match self.evaluate_round(game_id).await {
            Ok(r) => r,
            Err(e) => {
                if let Some(ref mut g) = idem_guard {
                    g.release().await;
                }
                return Err(e);
            }
        };

        let outcome = EvaluateRoundOutcome {
            round_number: result.round,
            winner_id: Some(result.winner_id),
            winner_position: result.winner_position as i32,
            game_ended: result.game_ended,
        };

        if let Some(ref mut g) = idem_guard {
            g.complete(&outcome).await;
        }

        Ok(outcome)
    }

    async fn verify_player_ownership(
        &self,
        game_id: Uuid,
        player_id: Uuid,
        user_id: Uuid,
    ) -> Result<bool, GameError> {
        self.verify_player_ownership(game_id, player_id, user_id)
            .await
    }
}

// ── InviteService ──────────────────────────────────────────────────────

#[async_trait]
impl InviteService for GameService {
    async fn send_invites(
        &self,
        game_id: Uuid,
        creator_user_id: Uuid,
        invited_user_ids: Vec<Uuid>,
    ) -> Result<(), GameError> {
        self.send_invites(game_id, creator_user_id, &invited_user_ids)
            .await
    }

    async fn accept_invite(
        &self,
        game_id: Uuid,
        user_id: Uuid,
        pseudo: &str,
    ) -> Result<AcceptInviteOutcome, GameError> {
        let player = self.accept_invite(game_id, user_id, pseudo).await?;

        let player_count = PlayerRepository::new(self.db.clone())
            .list_by_game(game_id)
            .await?
            .len() as i32;

        let game = GameRepository::new(self.db.clone())
            .find_by_id(game_id)
            .await?
            .ok_or(GameError::GameNotFound)?;

        Ok(AcceptInviteOutcome {
            player_id: player.id,
            position: player.position,
            player_count,
            max_players: game.max_players as i32,
            game_status: match game.status {
                GameStatus::Ready => "ready".to_string(),
                _ => "pending".to_string(),
            },
        })
    }

    async fn decline_invite(&self, game_id: Uuid, user_id: Uuid) -> Result<(), GameError> {
        self.decline_invite(game_id, user_id).await
    }
}

// ── GameLifecycleService ───────────────────────────────────────────────

#[async_trait]
impl GameLifecycleService for GameService {
    async fn create_quick_game(
        &self,
        correlation_id: Option<CorrelationId>,
        step_by_step: bool,
    ) -> Result<QuickGameOutcome, GameError> {
        self.create_quick_game(correlation_id, step_by_step).await
    }

    async fn create_bot_only_game(&self) -> Result<QuickGameOutcome, GameError> {
        self.create_bot_only_game().await
    }

    async fn create_quick_game_for_user_with_step_by_step(
        &self,
        user_id: Uuid,
        db: &DatabaseConnection,
        step_by_step: bool,
    ) -> Result<QuickGameOutcome, GameError> {
        self.create_quick_game_for_user_with_step_by_step(user_id, db, step_by_step)
            .await
    }

    async fn create_multiplayer_game(
        &self,
        user_id: Uuid,
        pseudo: &str,
        bet: i32,
        max_players: i16,
    ) -> Result<MultiplayerCreationOutcome, GameError> {
        let outcome = self
            .create_multiplayer_game(user_id, pseudo, bet, max_players)
            .await?;

        Ok(MultiplayerCreationOutcome {
            game_id: outcome.game_id,
            status: "pending".to_string(),
            bet: outcome.bet,
            max_players: outcome.max_players,
            invite_expires_at: outcome.invite_expires_at.to_rfc3339(),
        })
    }

    async fn start_game(&self, game_id: Uuid, user_id: Uuid) -> Result<(), GameError> {
        self.start_game(game_id, user_id).await
    }

    async fn cancel_game(&self, game_id: Uuid) -> Result<(), GameError> {
        self.cancel_game(game_id).await
    }
}

// ── BenchmarkService ──────────────────────────────────────────────────

#[async_trait]
impl BenchmarkService for GameService {
    async fn create_benchmark_multiplayer_game(
        &self,
        user_ids: Vec<Uuid>,
        bet: i32,
    ) -> Result<BenchmarkGameOutcome, GameError> {
        self.create_benchmark_multiplayer_game(user_ids, bet).await
    }

    async fn cleanup_benchmark_data(&self) -> Result<BenchmarkCleanupCounts, GameError> {
        self.cleanup_benchmark_data().await
    }
}

// ── Backward-compatible supertrait ────────────────────────────────────

impl GameServiceTrait for GameService {}
