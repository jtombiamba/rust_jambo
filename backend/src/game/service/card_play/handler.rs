use sea_orm::DatabaseTransaction;
use uuid::Uuid;

use crate::database::models::{game_card, player};
use crate::database::repositories::game::optimistic_update_round_state;
use crate::error::GameError;
use crate::game::service::types::RoundEvaluationResult;
use crate::game::turn_order::next_player;

use super::super::GameService;
use super::{engine, validator};

pub(crate) struct CardPlayOutcome {
    pub(crate) card: game_card::Model,
    pub(crate) players: Vec<player::Model>,
    pub(crate) round_result: Option<RoundEvaluationResult>,
    pub(crate) game_roll: i32,
    pub(crate) step_by_step: bool,
    pub(crate) current_rank: usize,
    pub(crate) active_count: usize,
}

pub(crate) struct CardPlayHandler<'a> {
    service: &'a GameService,
}

impl<'a> CardPlayHandler<'a> {
    pub(crate) fn new(service: &'a GameService) -> Self {
        Self { service }
    }

    pub(crate) async fn execute(
        &self,
        txn: &DatabaseTransaction,
        game_id: Uuid,
        player_id: Uuid,
        card_index: i32,
    ) -> Result<CardPlayOutcome, GameError> {
        let game = validator::fetch_and_validate_game(txn, game_id).await?;
        let read_version = game.updated_at;
        let game_rank = game.rank.unwrap_or(0);
        let game_roll = game.roll;

        let (players, player_position, active_count) =
            validator::fetch_and_validate_turn(txn, game_id, player_id).await?;

        if player_position != game_rank as usize {
            return Err(GameError::NotYourTurn);
        }

        let (target_card, player_cards) =
            validator::fetch_and_validate_card(txn, player_id, card_index).await?;

        let valid =
            validator::validate_follows_suit(card_index, game.current_winning_card, &player_cards);
        if !valid {
            return Err(GameError::InvalidCard);
        }

        let card = engine::mark_card_played(txn, &target_card, game_roll).await?;

        let new_winning_card = engine::compute_winning_card(game.current_winning_card, card_index);
        let new_winning_position = engine::compute_winning_position(
            game.current_winning_card,
            game.current_winning_player_position,
            card_index,
            player_position as i32,
        );

        let round_complete = self
            .service
            .is_round_complete_txn(txn, game_id, game_roll)
            .await?;
        let mut round_result: Option<RoundEvaluationResult> = None;

        if round_complete && !game.step_by_step {
            round_result = Some(
                self.service
                    .evaluate_round_in_txn(txn, game_id, game_roll)
                    .await?,
            );
        } else {
            let new_rank = if round_complete && game.step_by_step {
                game_rank
            } else {
                next_player(player_position, active_count) as i32
            };

            let rows_affected = optimistic_update_round_state(
                txn,
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
                self.service
                    .process_payment_in_txn(txn, game_id, result)
                    .await?;
            }
        }

        Ok(CardPlayOutcome {
            card,
            players,
            round_result,
            game_roll,
            step_by_step: game.step_by_step,
            current_rank: player_position,
            active_count,
        })
    }
}
