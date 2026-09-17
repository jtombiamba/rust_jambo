use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, DatabaseTransaction, EntityTrait, QueryFilter,
    QueryOrder, TransactionTrait,
};
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tracing::info;
use uuid::Uuid;

use crate::database::models::{game, game_card, player, GameMode, GameStatus};
use crate::database::repositories::game::optimistic_update_round_result_in_txn;
use crate::database::repositories::{
    GameCardRepository, PlayerProfileRepository, PlayerRepository,
};
use crate::error::GameError;
use crate::game::card_mapping::Card;
use crate::game::constants::{
    CARDS_PER_PLAYER, DOUBLE_KORA_CREDIT_MULTIPLIER, KORA_CREDIT_MULTIPLIER,
};
use crate::game::payment::calculate_payment;
use crate::game::round_evaluation::{evaluate_round, PlayedCard, RoundContext};
use crate::game::service::types::{RoundEvalTimer, RoundEvaluationResult};
use crate::game::special_cards::compute_special_cards;
use crate::observability::metrics;

use super::GameService;

/// Convert the DB `game_card` models of a completed round into the pure
/// [`PlayedCard`] structures consumed by [`evaluate_round`].
///
/// Ordering of `played_cards` is preserved (it must already be ordered by
/// `played_at`, since `plays[0]` becomes the leading card). `player_positions`
/// maps a player id to its position index; it must be ordered by player
/// position so the index corresponds to the in-game seat.
fn build_plays(
    played_cards: &[game_card::Model],
    player_positions: &[Uuid],
) -> Result<Vec<PlayedCard>, GameError> {
    let mut plays = Vec::with_capacity(played_cards.len());
    for card in played_cards {
        if let Some(player_id) = card.player_id {
            let position = player_positions
                .iter()
                .position(|&id| id == player_id)
                .ok_or_else(|| GameError::internal("Player not found in game"))?;
            let Ok(index) = u8::try_from(card.card_index) else {
                continue;
            };
            if let Some(card_obj) = Card::new(index) {
                plays.push(PlayedCard {
                    player_position: position,
                    card: card_obj,
                });
            }
        }
    }
    Ok(plays)
}

/// Pure decision of the final game status once the last round is complete.
///
/// A Kora is when the winning card is a 3 (indices 0, 8, 16, 24). A Double
/// Kora additionally requires the same player to have won round 4 with a Kora
/// card. Any other end state is a normal `Finished`.
fn compute_final_status(
    is_kora: bool,
    round_4_winner_id: Option<Uuid>,
    round_5_winner_id: Uuid,
    round_4_winner_played_kora: bool,
) -> GameStatus {
    if !is_kora {
        return GameStatus::Finished;
    }
    if round_4_winner_id == Some(round_5_winner_id) && round_4_winner_played_kora {
        GameStatus::DoubleKora
    } else {
        GameStatus::Kora
    }
}

/// Final game status with the special-card upgrade applied: a winner who holds
/// an unclaimed special combination always finishes as a `Kora`, overriding any
/// normal or double-kora detection.
fn compute_final_status_with_special(
    is_kora: bool,
    round_4_winner_id: Option<Uuid>,
    round_5_winner_id: Uuid,
    round_4_winner_played_kora: bool,
    winner_has_special: bool,
) -> GameStatus {
    if winner_has_special {
        GameStatus::Kora
    } else {
        compute_final_status(
            is_kora,
            round_4_winner_id,
            round_5_winner_id,
            round_4_winner_played_kora,
        )
    }
}

impl GameService {
    /// Check if all players have played a card in the given round.
    /// Uses the provided transaction connection to see uncommitted data.
    #[tracing::instrument(skip(self, txn), fields(game_id = %game_id, round = %round))]
    pub(crate) async fn is_round_complete_txn(
        &self,
        txn: &DatabaseTransaction,
        game_id: Uuid,
        round: i32,
    ) -> Result<bool, GameError> {
        let players = player::Entity::find()
            .filter(player::Column::GameId.eq(game_id))
            .all(txn)
            .await?;

        for player_model in &players {
            if player_model.kicked {
                continue;
            }
            let cards = game_card::Entity::find()
                .filter(game_card::Column::PlayerId.eq(player_model.id))
                .all(txn)
                .await?;
            let played_in_round = cards.iter().any(|c| c.played && c.round == Some(round));
            if !played_in_round {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Evaluate a completed round inside an active transaction.
    /// All DB writes use the transaction connection for atomicity.
    /// Returns RoundEvaluationResult for post-transaction event publishing.
    pub async fn evaluate_round_in_txn(
        &self,
        txn: &DatabaseTransaction,
        game_id: Uuid,
        round: i32,
    ) -> Result<RoundEvaluationResult, GameError> {
        let _timer = RoundEvalTimer(Instant::now());
        let span = tracing::info_span!(
            "round_eval",
            game_id = %game_id,
            round = round,
        );
        let _guard = span.enter();

        // Fetch played cards for this round (via txn).
        let game_card_repo = GameCardRepository::new(self.db.clone());
        let player_repo = PlayerRepository::new(self.db.clone());
        let played_cards = game_card_repo
            .list_played_by_game_and_round_in_txn(txn, game_id, round)
            .await?;
        info!(
            "evaluate_round_in_txn: found {} played cards for round {}",
            played_cards.len(),
            round
        );
        if played_cards.is_empty() {
            return Err(GameError::RoundNotComplete);
        }

        // Fetch players (via txn), ordered by position so the index in
        // `player_positions` matches each player's in-game seat.
        let players = player_repo.list_by_game_in_txn(txn, game_id).await?;
        let active_players: Vec<&player::Model> = players.iter().filter(|p| !p.kicked).collect();
        let player_positions: Vec<Uuid> = active_players.iter().map(|p| p.id).collect();

        let plays = build_plays(&played_cards, &player_positions)?;

        let first_play = plays
            .first()
            .ok_or_else(|| GameError::internal("No plays in round"))?;
        let leading_card = Some(first_play.card);
        let leading_player_position = Some(first_play.player_position);

        for (index, value) in plays.iter().map(|p| (p.player_position, p.card)) {
            info!(
                " played in round {}: Index: {}, Value: {}",
                round, index, value.index
            );
        }
        if let Some(card) = leading_card {
            info!(
                " leading card for round {}: index {} (suit {})",
                round,
                card.index,
                card.index / 8
            );
        } else {
            info!(" no leading card for round {} (first round)", round);
        }

        let ctx = RoundContext {
            plays,
            leading_card,
            leading_player_position,
        };
        let round_result =
            evaluate_round(&ctx).ok_or_else(|| GameError::internal("Round evaluation failed"))?;
        let winner_pos = round_result.winner_position;
        let winner_id = player_positions[winner_pos];

        let new_roll = round + 1;
        info!(
            "Round {} evaluated, winner is player {}, updating round to {}",
            round, winner_id, new_roll
        );

        let game_model = game::Entity::find_by_id(game_id)
            .one(txn)
            .await?
            .ok_or(GameError::GameNotFound)?;

        let read_version = game_model.updated_at;

        let game_ends = new_roll > CARDS_PER_PLAYER as i32;

        let round_4_winner_played_kora =
            if round_result.is_kora && game_model.winner_id == Some(winner_id) {
                game_card_repo
                    .find_played_card_in_round_in_txn(txn, winner_id, 4)
                    .await
                    .ok()
                    .flatten()
                    .map(|c| c.card_index % 8 == 0)
                    .unwrap_or(false)
            } else {
                false
            };

        let winner_has_special =
            if game_ends && matches!(game_model.game_mode, GameMode::Multiplayer) {
                game_card_repo
                    .list_by_player_in_txn(txn, winner_id)
                    .await
                    .map(|cards| {
                        let hand: Vec<i32> = cards.iter().map(|c| c.card_index).collect();
                        compute_special_cards(&hand).has_any()
                    })
                    .unwrap_or(false)
            } else {
                false
            };

        let final_status = game_ends.then(|| {
            compute_final_status_with_special(
                round_result.is_kora,
                game_model.winner_id,
                winner_id,
                round_4_winner_played_kora,
                winner_has_special,
            )
        });

        let rows_affected = optimistic_update_round_result_in_txn(
            txn,
            game_id,
            winner_id,
            winner_pos as i32,
            new_roll,
            read_version,
            final_status,
        )
        .await?;

        if rows_affected == 0 {
            return Err(GameError::VersionConflict);
        }

        Ok(RoundEvaluationResult {
            round,
            winner_id,
            winner_position: winner_pos,
            game_ended: game_ends,
            final_status: final_status.unwrap_or(game_model.status),
            players: active_players.into_iter().cloned().collect(),
        })
    }

    /// Process payment for a finished game inside an active transaction.
    #[tracing::instrument(skip(self, txn), fields(game_id = %game_id, winner_id = %result.winner_id))]
    pub(crate) async fn process_payment_in_txn(
        &self,
        txn: &DatabaseTransaction,
        game_id: Uuid,
        result: &RoundEvaluationResult,
    ) -> Result<(), GameError> {
        let players = &result.players;
        let total_players = players.len();
        let winner_id = result.winner_id;
        let winner_position = players
            .iter()
            .position(|p| p.id == winner_id)
            .ok_or_else(|| GameError::internal("Winner not in player list"))?;

        let bet_multiplier = match result.final_status {
            GameStatus::Kora => KORA_CREDIT_MULTIPLIER,
            GameStatus::DoubleKora => DOUBLE_KORA_CREDIT_MULTIPLIER,
            _ => 1,
        };

        let is_kora = matches!(
            result.final_status,
            GameStatus::Kora | GameStatus::DoubleKora
        );

        let game_model = game::Entity::find_by_id(game_id)
            .one(txn)
            .await?
            .ok_or(GameError::GameNotFound)?;
        let bet = game_model.bet * bet_multiplier;

        let credits = calculate_payment(winner_position, total_players, bet);

        let profile_repo = PlayerProfileRepository::new(self.db.clone());

        for (idx, player) in players.iter().enumerate() {
            let new_credits = player.credits + game_model.bet + credits[idx];
            let mut player_active: player::ActiveModel = player.clone().into();
            player_active.credits = ActiveValue::Set(new_credits);
            player_active.update(txn).await?;

            if let Some(user_id) = player.user_id {
                let won = player.id == winner_id;
                let delta = game_model.bet + credits[idx];
                let _new_credit = profile_repo
                    .apply_game_settlement_in_txn(
                        txn,
                        user_id,
                        delta,
                        won,
                        is_kora,
                        self.freeze_duration_secs,
                    )
                    .await?;
            }
        }

        Ok(())
    }

    pub async fn evaluate_round(&self, game_id: Uuid) -> Result<RoundEvaluationResult, GameError> {
        let max_retries = 3u32;
        let mut attempt = 0u32;

        'retry: loop {
            let txn = self.db.begin().await?;

            let game = game::Entity::find_by_id(game_id)
                .one(&txn)
                .await?
                .ok_or(GameError::GameNotFound)?;

            let round = game.roll;

            let result = match self.evaluate_round_in_txn(&txn, game_id, round).await {
                Ok(result) => result,
                Err(GameError::VersionConflict) => {
                    txn.rollback().await.ok();
                    attempt += 1;
                    if attempt >= max_retries {
                        return Err(GameError::internal(
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
            };

            if result.game_ended {
                self.process_payment_in_txn(&txn, game_id, &result).await?;
            }

            txn.commit().await?;

            let players = player::Entity::find()
                .filter(player::Column::GameId.eq(game_id))
                .order_by_asc(player::Column::Position)
                .all(&self.db)
                .await?;

            self.publish_round_completed(game_id, &result, &players, None)
                .await;

            if !result.game_ended {
                self.publish_turn_changed(game_id, result.winner_id, None)
                    .await;
            }

            if result.game_ended {
                metrics::ACTIVE_GAMES.dec();
                metrics::GAMES_FINISHED_TOTAL
                    .with_label_values(&[&result.final_status.to_string()])
                    .inc();
                self.publish_game_finished(game_id, &result, None).await;
                self.invalidate_game_state_cache(game_id).await;

                let user_ids: Vec<uuid::Uuid> =
                    result.players.iter().filter_map(|p| p.user_id).collect();
                if !user_ids.is_empty() {
                    self.invalidate_dashboard_caches(&user_ids).await;
                }
            } else {
                self.cache_game_state(game_id).await;
            }

            return Ok(result);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::models::game_card;
    use chrono::Utc;

    fn card_model(player_id: Option<Uuid>, card_index: i32) -> game_card::Model {
        let now = Utc::now();
        game_card::Model {
            id: Uuid::new_v4(),
            game_id: Uuid::new_v4(),
            player_id,
            card_index,
            played: true,
            played_at: Some(now),
            round: Some(1),
            created_at: now,
        }
    }

    #[test]
    fn build_plays_maps_cards_to_positions() {
        let p0 = Uuid::new_v4();
        let p1 = Uuid::new_v4();
        let cards = vec![card_model(Some(p0), 0), card_model(Some(p1), 8)];
        let plays = build_plays(&cards, &[p0, p1]).unwrap();
        assert_eq!(plays.len(), 2);
        assert_eq!(plays[0].player_position, 0);
        assert_eq!(plays[0].card.index, 0);
        assert_eq!(plays[1].player_position, 1);
        assert_eq!(plays[1].card.index, 8);
    }

    #[test]
    fn build_plays_skips_unassigned_cards() {
        let p0 = Uuid::new_v4();
        let cards = vec![card_model(Some(p0), 0), card_model(None, 1)];
        let plays = build_plays(&cards, &[p0]).unwrap();
        assert_eq!(plays.len(), 1);
        assert_eq!(plays[0].player_position, 0);
    }

    #[test]
    fn build_plays_skips_card_index_out_of_deck() {
        let p0 = Uuid::new_v4();
        // 32 is a valid u8 but not a valid deck card (deck is 0..32).
        let cards = vec![card_model(Some(p0), 32)];
        assert!(build_plays(&cards, &[p0]).unwrap().is_empty());
    }

    #[test]
    fn build_plays_skips_card_index_too_large_for_u8() {
        let p0 = Uuid::new_v4();
        let cards = vec![card_model(Some(p0), 256)];
        assert!(build_plays(&cards, &[p0]).unwrap().is_empty());
    }

    #[test]
    fn build_plays_errors_on_unknown_player() {
        let p0 = Uuid::new_v4();
        let unknown = Uuid::new_v4();
        let cards = vec![card_model(Some(unknown), 0)];
        assert!(build_plays(&cards, &[p0]).is_err());
    }

    #[test]
    fn compute_final_status_finished_when_not_kora() {
        let p = Uuid::new_v4();
        assert_eq!(
            compute_final_status(false, Some(p), p, true),
            GameStatus::Finished
        );
    }

    #[test]
    fn compute_final_status_double_kora() {
        let p = Uuid::new_v4();
        assert_eq!(
            compute_final_status(true, Some(p), p, true),
            GameStatus::DoubleKora
        );
    }

    #[test]
    fn compute_final_status_kora_when_round4_winner_not_kora() {
        let p = Uuid::new_v4();
        assert_eq!(
            compute_final_status(true, Some(p), p, false),
            GameStatus::Kora
        );
    }

    #[test]
    fn compute_final_status_kora_when_different_winner() {
        let p = Uuid::new_v4();
        let q = Uuid::new_v4();
        assert_eq!(
            compute_final_status(true, Some(p), q, true),
            GameStatus::Kora
        );
    }

    #[test]
    fn compute_final_status_kora_when_no_round4_winner() {
        let p = Uuid::new_v4();
        assert_eq!(compute_final_status(true, None, p, true), GameStatus::Kora);
    }

    #[test]
    fn compute_final_status_with_special_overrides_double_kora() {
        let p = Uuid::new_v4();
        assert_eq!(
            compute_final_status_with_special(true, Some(p), p, true, true),
            GameStatus::Kora
        );
    }

    #[test]
    fn compute_final_status_with_special_overrides_finished() {
        let p = Uuid::new_v4();
        assert_eq!(
            compute_final_status_with_special(false, Some(p), p, false, true),
            GameStatus::Kora
        );
    }

    #[test]
    fn compute_final_status_with_special_defers_when_no_special() {
        let p = Uuid::new_v4();
        assert_eq!(
            compute_final_status_with_special(true, Some(p), p, true, false),
            GameStatus::DoubleKora
        );
        assert_eq!(
            compute_final_status_with_special(false, Some(p), p, false, false),
            GameStatus::Finished
        );
    }
}
