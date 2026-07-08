use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, QueryFilter, QueryOrder,
    TransactionTrait,
};
use std::time::{Duration, Instant};
use tokio::time::sleep;
use uuid::Uuid;

use crate::database::models::{game, game_card, player, GameStatus};
use crate::database::repositories::{GameCardRepository, GameRepository, PlayerRepository};
use crate::game::service::types::{
    CardPlayResult, CardPlayTimer, GameServiceError, RoundEvaluationResult,
};
use crate::game::turn_order::next_player;
use crate::observability::metrics;

use super::GameService;

impl GameService {
    pub async fn validate_card_play(
        &self,
        _game_id: Uuid,
        player_id: Uuid,
        card_index: i32,
        current_winning_card: Option<i32>,
    ) -> Result<bool, GameServiceError> {
        if let Some(winning_idx) = current_winning_card {
            if (winning_idx / 8) == (card_index / 8) {
                return Ok(true);
            } else {
                let repo = GameCardRepository::new(self.db.clone());
                let player_cards = repo.list_by_player(player_id).await?;
                let unplayed_cards: Vec<i32> = player_cards
                    .iter()
                    .filter(|gc| !gc.played)
                    .map(|gc| gc.card_index)
                    .collect();
                for challenger_card in unplayed_cards {
                    if (winning_idx / 8) == (challenger_card / 8) {
                        return Ok(false);
                    }
                }
            }
        }
        Ok(true)
    }

    pub async fn update_card_play(
        &self,
        game_id: Uuid,
        player_id: Uuid,
        card_index: i32,
        correlation_id: Option<Uuid>,
    ) -> Result<CardPlayResult, GameServiceError> {
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

        'retry: loop {
            let txn = self.db.begin().await?;

            // 1. Fetch game and verify it's not finished (via txn)
            let game = game::Entity::find_by_id(game_id)
                .one(&txn)
                .await?
                .ok_or(GameServiceError::GameNotFound)?;
            if game.status == GameStatus::Finished
                || game.status == GameStatus::Kora
                || game.status == GameStatus::DoubleKora
            {
                txn.rollback().await.ok();
                return Err(GameServiceError::GameFinished);
            }

            let read_version = game.updated_at;

            // 2. Verify it's the player's turn (via txn)
            let players = player::Entity::find()
                .filter(player::Column::GameId.eq(game_id))
                .order_by_asc(player::Column::Position)
                .all(&txn)
                .await?;
            let current_player = players
                .iter()
                .find(|p| p.id == player_id)
                .ok_or(GameServiceError::PlayerNotFound)?;
            let active_player_count = players.iter().filter(|p| !p.kicked).count();
            let current_rank = game.rank.unwrap_or(0) as usize;
            if current_player.position as usize != current_rank {
                txn.rollback().await.ok();
                return Err(GameServiceError::NotYourTurn);
            }

            // 3. Fetch the card and ensure it's unplayed (via txn)
            let game_cards = game_card::Entity::find()
                .filter(game_card::Column::PlayerId.eq(player_id))
                .all(&txn)
                .await?;
            let target_card = game_cards
                .iter()
                .find(|gc| gc.card_index == card_index && !gc.played)
                .ok_or(GameServiceError::CardNotFound)?;

            // 4. Use stored current winning card from game model
            let current_winning_card = game.current_winning_card;

            // 5. Validate the card
            let valid = self
                .validate_card_play(game_id, player_id, card_index, current_winning_card)
                .await?;
            if !valid {
                txn.rollback().await.ok();
                return Err(GameServiceError::InvalidCard);
            }

            // 6. Mark card as played and set round = game.roll (using txn connection)
            let mut card_active: game_card::ActiveModel = target_card.clone().into();
            card_active.played = ActiveValue::Set(true);
            card_active.played_at = ActiveValue::Set(Some(chrono::Utc::now()));
            card_active.round = ActiveValue::Set(Some(game.roll));
            card_active.update(&txn).await?;

            // 6a. Compute updated current_winning_card and current_winning_player_position
            let new_winning_card = if current_winning_card.is_none() {
                Some(card_index)
            } else {
                current_winning_card
            };
            let new_winning_position = match current_winning_card {
                None => Some(current_player.position),
                Some(winning) => {
                    if winning / 8 == card_index / 8 && card_index % 8 > winning % 8 {
                        Some(current_player.position)
                    } else {
                        game.current_winning_player_position
                    }
                }
            };

            // 7. Check if round is complete BEFORE updating rank
            let round_complete = self.is_round_complete_txn(&txn, game_id, game.roll).await?;
            let mut round_result: Option<RoundEvaluationResult> = None;

            if round_complete && !game.step_by_step {
                match self.evaluate_round_in_txn(&txn, game_id, game.roll).await {
                    Ok(result) => round_result = Some(result),
                    Err(GameServiceError::VersionConflict) => {
                        txn.rollback().await.ok();
                        attempt += 1;
                        if attempt >= max_retries {
                            return Err(GameServiceError::Internal(
                                "Optimistic lock conflict after max retries".to_string(),
                            ));
                        }
                        sleep(Duration::from_millis(10 * 2u64.pow(attempt))).await;
                        continue 'retry;
                    }
                    Err(e) => {
                        txn.rollback().await.ok();
                        return Err(e);
                    }
                }
            } else {
                let new_rank = if round_complete && game.step_by_step {
                    current_rank as i32
                } else {
                    next_player(current_rank, active_player_count) as i32
                };
                let update_result = game::Entity::update_many()
                    .col_expr(
                        game::Column::Rank,
                        sea_orm::sea_query::Expr::value(sea_orm::Value::Int(Some(new_rank))),
                    )
                    .col_expr(
                        game::Column::CurrentWinningCard,
                        sea_orm::sea_query::Expr::value(sea_orm::Value::Int(new_winning_card)),
                    )
                    .col_expr(
                        game::Column::CurrentWinningPlayerPosition,
                        sea_orm::sea_query::Expr::value(sea_orm::Value::Int(new_winning_position)),
                    )
                    .col_expr(
                        game::Column::UpdatedAt,
                        sea_orm::sea_query::Expr::value(sea_orm::Value::ChronoDateTimeUtc(Some(
                            Utc::now(),
                        ))),
                    )
                    .filter(game::Column::Id.eq(game_id))
                    .filter(game::Column::UpdatedAt.eq(read_version))
                    .exec(&txn)
                    .await?;

                if update_result.rows_affected == 0 {
                    txn.rollback().await.ok();
                    attempt += 1;
                    if attempt >= max_retries {
                        return Err(GameServiceError::Internal(
                            "Optimistic lock conflict after max retries".to_string(),
                        ));
                    }
                    sleep(Duration::from_millis(10 * 2u64.pow(attempt))).await;
                    continue 'retry;
                }
            }

            // 8. If game ended, process payment INSIDE the transaction
            if let Some(ref result) = round_result {
                if result.game_ended {
                    self.process_payment_in_txn(&txn, game_id, result).await?;
                }
            }

            // 9. Commit transaction
            txn.commit().await?;

            // === PHASE 2: Post-transaction operations (best-effort, non-critical) ===

            let round_completed = round_result.is_some();
            let game_ended = round_result.as_ref().map(|r| r.game_ended).unwrap_or(false);
            let current_round = game.roll;
            let next_player_id = if let Some(ref result) = round_result {
                players
                    .get(result.winner_position)
                    .map(|p| p.id)
                    .ok_or_else(|| {
                        GameServiceError::Internal("Winner not in player list".to_string())
                    })?
            } else {
                let rank_after = next_player(current_rank, active_player_count);
                players.get(rank_after).map(|p| p.id).ok_or_else(|| {
                    GameServiceError::Internal("No player at computed rank".to_string())
                })?
            };

            // 11. Publish CardPlayed event
            self.publish_card_played(
                game_id,
                player_id,
                card_index,
                Some(next_player_id),
                correlation_id,
            )
            .await;

            // 11b. Publish TurnChanged event
            if !game_ended {
                self.publish_turn_changed(game_id, next_player_id, correlation_id)
                    .await;
            }

            // 12. If round was evaluated, publish events
            if let Some(ref result) = round_result {
                self.publish_round_completed(game_id, result, &players, correlation_id)
                    .await;

                if result.game_ended {
                    metrics::GAMES_FINISHED_TOTAL
                        .with_label_values(&[&result.final_status.to_string()])
                        .inc();
                    self.publish_game_finished(game_id, result, correlation_id)
                        .await;
                    self.invalidate_game_state_cache(game_id).await;

                    let user_ids: Vec<Uuid> =
                        result.players.iter().filter_map(|p| p.user_id).collect();
                    if !user_ids.is_empty() {
                        self.invalidate_dashboard_caches(&user_ids).await;
                    }
                }
            }

            // 12b. Update game state cache if game is still active
            if !game_ended {
                self.cache_game_state(game_id).await;
            }

            let card_repo = GameCardRepository::new(self.db.clone());
            let card = card_repo
                .list_by_player(player_id)
                .await?
                .into_iter()
                .find(|gc| gc.id == target_card.id)
                .ok_or(GameServiceError::Internal("Card disappeared".to_string()))?;

            return Ok(CardPlayResult {
                card,
                next_player_id,
                players,
                game_ended,
                round_completed,
                current_round,
                step_by_step: game.step_by_step,
            });
        }
    }

    pub async fn next_player(&self, game_id: Uuid) -> Result<Uuid, GameServiceError> {
        let game_repo = GameRepository::new(self.db.clone());
        let player_repo = PlayerRepository::new(self.db.clone());
        let game = game_repo
            .find_by_id(game_id)
            .await?
            .ok_or(GameServiceError::GameNotFound)?;
        let players = player_repo.list_by_game(game_id).await?;
        let active_players: Vec<_> = players.iter().filter(|p| !p.kicked).collect();
        let current_rank = game.rank.unwrap_or(0) as usize;
        let player_id =
            active_players
                .get(current_rank)
                .map(|p| p.id)
                .ok_or(GameServiceError::Internal(
                    "Player index out of bounds".to_string(),
                ))?;
        Ok(player_id)
    }
}
