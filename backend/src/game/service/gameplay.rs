use sea_orm::TransactionTrait;
use std::time::{Duration, Instant};
use tokio::time::sleep;
use uuid::Uuid;

use crate::database::repositories::game::optimistic_update_round_state;
use crate::database::repositories::{GameRepository, PlayerRepository};
use crate::error::GameError;
use crate::game::service::card_play::{self, side_effects::PostCommitContext};
use crate::game::service::types::{CardPlayResult, CardPlayTimer, RoundEvaluationResult};
use crate::game::turn_order::next_player;

use super::GameService;

struct TxOutcome {
    card: crate::database::models::game_card::Model,
    players: Vec<crate::database::models::player::Model>,
    round_result: Option<RoundEvaluationResult>,
    game_roll: i32,
    step_by_step: bool,
    current_rank: usize,
    active_count: usize,
}

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

            let body_result = async {
                let game = card_play::validator::fetch_and_validate_game(&txn, game_id).await?;
                let read_version = game.updated_at;
                let game_rank = game.rank.unwrap_or(0);
                let game_roll = game.roll;

                let (players, player_position, active_count) =
                    card_play::validator::fetch_and_validate_turn(&txn, game_id, player_id).await?;

                if player_position != game_rank as usize {
                    return Err(GameError::NotYourTurn);
                }

                let (target_card, player_cards) =
                    card_play::validator::fetch_and_validate_card(&txn, player_id, card_index)
                        .await?;

                let valid = card_play::validator::validate_follows_suit(
                    card_index,
                    game.current_winning_card,
                    &player_cards,
                );
                if !valid {
                    return Err(GameError::InvalidCard);
                }

                let card =
                    card_play::engine::mark_card_played(&txn, &target_card, game_roll).await?;

                let new_winning_card =
                    card_play::engine::compute_winning_card(game.current_winning_card, card_index);
                let new_winning_position = card_play::engine::compute_winning_position(
                    game.current_winning_card,
                    game.current_winning_player_position,
                    card_index,
                    player_position as i32,
                );

                let round_complete = self.is_round_complete_txn(&txn, game_id, game_roll).await?;
                let mut round_result: Option<RoundEvaluationResult> = None;

                if round_complete && !game.step_by_step {
                    round_result =
                        Some(self.evaluate_round_in_txn(&txn, game_id, game_roll).await?);
                } else {
                    let new_rank = if round_complete && game.step_by_step {
                        game_rank
                    } else {
                        next_player(player_position, active_count) as i32
                    };

                    let rows_affected = optimistic_update_round_state(
                        &txn,
                        game_id,
                        Some(new_rank),
                        new_winning_card,
                        new_winning_position,
                        read_version,
                    )
                    .await?;

                    if rows_affected == 0 {
                        return Err(GameError::VersionConflict);
                    }
                }

                if let Some(ref result) = round_result {
                    if result.game_ended {
                        self.process_payment_in_txn(&txn, game_id, result).await?;
                    }
                }

                Ok(TxOutcome {
                    card,
                    players,
                    round_result,
                    game_roll,
                    step_by_step: game.step_by_step,
                    current_rank: player_position,
                    active_count,
                })
            };

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
        let next_player_id = card_play::engine::determine_next_player_id(
            outcome.round_result.as_ref(),
            &outcome.players,
            outcome.current_rank,
            outcome.active_count,
        )?;

        // // In step_by_step mode, when the round is complete, the DB rank is preserved
        // // (same player starts the new round). Override next_player_id to match.
        // if outcome.step_by_step && round_completed {
        //     if let Some(p) = outcome
        //         .players
        //         .iter()
        //         .find(|p| p.position as usize == outcome.current_rank)
        //     {
        //         next_player_id = p.id;
        //     }
        // }

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
