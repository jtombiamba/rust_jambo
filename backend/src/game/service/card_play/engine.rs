use sea_orm::{ActiveModelTrait, ActiveValue, DatabaseTransaction};
use uuid::Uuid;

use crate::database::models::{game_card, player};
use crate::error::GameError;
use crate::game::service::types::RoundEvaluationResult;
use crate::game::turn_order::next_player;

pub(crate) fn compute_winning_card(current_winning: Option<i32>, played: i32) -> Option<i32> {
    match current_winning {
        None => Some(played),
        Some(winning) => {
            if winning / 8 == played / 8 && played % 8 > winning % 8 {
                Some(played)
            } else {
                current_winning
            }
        }
    }
}

pub(crate) fn compute_winning_position(
    current_winning: Option<i32>,
    current_pos: Option<i32>,
    played: i32,
    player_position: i32,
) -> Option<i32> {
    match current_winning {
        None => Some(player_position),
        Some(winning) => {
            if winning / 8 == played / 8 && played % 8 > winning % 8 {
                Some(player_position)
            } else {
                current_pos
            }
        }
    }
}

pub(crate) fn determine_next_player_id(
    round_result: Option<&RoundEvaluationResult>,
    players: &[player::Model],
    current_rank: usize,
    active_count: usize,
) -> Result<Uuid, GameError> {
    if let Some(result) = round_result {
        result
            .players
            .get(result.winner_position)
            .map(|p| p.id)
            .ok_or_else(|| GameError::internal("Winner not in player list"))
    } else {
        let active_players: Vec<_> = players.iter().filter(|p| !p.kicked).collect();
        let rank_after = next_player(current_rank, active_count);
        active_players
            .get(rank_after)
            .map(|p| p.id)
            .ok_or_else(|| GameError::internal("No player at computed rank"))
    }
}

pub(crate) async fn mark_card_played(
    txn: &DatabaseTransaction,
    target_card: &game_card::Model,
    game_roll: i32,
) -> Result<game_card::Model, GameError> {
    let mut card_active: game_card::ActiveModel = target_card.clone().into();
    card_active.played = ActiveValue::Set(true);
    card_active.played_at = ActiveValue::Set(Some(chrono::Utc::now()));
    card_active.round = ActiveValue::Set(Some(game_roll));
    card_active.update(txn).await.map_err(GameError::from)
}
