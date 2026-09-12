use sea_orm::TransactionTrait;
use std::time::{Duration, Instant};
use tokio::time::sleep;
use uuid::Uuid;

use crate::database::repositories::{GameRepository, PlayerRepository};
use crate::error::GameError;
use crate::game::service::card_play::engine;
use crate::game::service::card_play::handler::CardPlayHandler;
use crate::game::service::card_play::side_effects::PostCommitContext;
use crate::game::service::types::{CardPlayResult, CardPlayTimer};

use super::GameService;

impl GameService {
    pub async fn update_card_play(
        &self,
        game_id: Uuid,
        player_id: Uuid,
        card_index: i32,
        correlation_id: Option<Uuid>,
    ) -> Result<CardPlayResult, GameError> {
        let _timer = CardPlayTimer(Instant::now());
        let span = tracing::info_span!(
            "card_play",
            correlation_id = %correlation_id.map(|id| id.to_string()).unwrap_or_default(),
            game_id = %game_id,
            player_id = %player_id,
            card_index = card_index,
        );
        let _guard = span.enter();

        let max_retries = 3u32;
        let mut attempt = 0u32;

        let outcome = 'retry: loop {
            let txn = self.db.begin().await?;

            // The card-play handler reads and writes inside this transaction by
            // design: `read_version` (game.updated_at) is the optimistic-lock
            // guard, and the validators return state that is consumed within the
            // same snapshot. Reading outside the transaction would break the
            // optimistic-concurrency contract and reintroduce TOCTOU races.
            let handler = CardPlayHandler::new(self);
            let body_result = handler.execute(&txn, game_id, player_id, card_index);

            match body_result.await {
                Ok(value) => {
                    txn.commit().await?;
                    break 'retry value;
                }
                Err(GameError::VersionConflict) => {
                    txn.rollback().await.ok();
                    attempt += 1;
                    if attempt >= max_retries {
                        return Err(GameError::internal(
                            "Optimistic lock conflict after max retries".to_string(),
                        ));
                    }
                    sleep(Duration::from_millis(10 * 2u64.pow(attempt))).await;
                }
                Err(e) => {
                    txn.rollback().await.ok();
                    return Err(e);
                }
            }
        };

        let game_ended = outcome
            .round_result
            .as_ref()
            .map(|r| r.game_ended)
            .unwrap_or(false);
        let round_completed = outcome.round_result.is_some();
        let next_player_id = engine::determine_next_player_id(
            outcome.round_result.as_ref(),
            &outcome.players,
            outcome.current_rank,
            outcome.active_count,
        )?;

        let post_context = PostCommitContext {
            game_id,
            player_id,
            card_index,
            next_player_id,
            players: outcome.players.clone(),
            game_ended,
            round_result: outcome.round_result.clone(),
            correlation_id,
        };
        post_context.handle(self).await;

        Ok(CardPlayResult {
            card: outcome.card,
            next_player_id,
            players: outcome.players,
            game_ended,
            round_completed,
            current_round: outcome.game_roll,
            step_by_step: outcome.step_by_step,
        })
    }

    pub async fn next_player(&self, game_id: Uuid) -> Result<Uuid, GameError> {
        let game_repo = GameRepository::new(self.db.clone());
        let player_repo = PlayerRepository::new(self.db.clone());
        let game = game_repo
            .find_by_id(game_id)
            .await?
            .ok_or(GameError::GameNotFound)?;
        let players = player_repo.list_by_game(game_id).await?;
        let active_players: Vec<_> = players.iter().filter(|p| !p.kicked).collect();
        let current_rank = game.rank.unwrap_or(0) as usize;
        let player_id =
            active_players
                .get(current_rank)
                .map(|p| p.id)
                .ok_or(GameError::internal(
                    "Player index out of bounds".to_string(),
                ))?;
        Ok(player_id)
    }
}
