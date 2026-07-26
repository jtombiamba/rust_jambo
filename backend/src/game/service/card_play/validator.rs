use sea_orm::{ColumnTrait, DatabaseTransaction, EntityTrait, QueryFilter, QueryOrder};
use uuid::Uuid;

use crate::database::models::{game, game_card, player, GameStatus};
use crate::error::GameError;

pub(crate) async fn fetch_and_validate_game(
    txn: &DatabaseTransaction,
    game_id: Uuid,
) -> Result<game::Model, GameError> {
    let game = game::Entity::find_by_id(game_id)
        .one(txn)
        .await?
        .ok_or(GameError::GameNotFound)?;
    if matches!(
        game.status,
        GameStatus::Finished | GameStatus::Kora | GameStatus::DoubleKora
    ) {
        return Err(GameError::GameFinished);
    }
    Ok(game)
}

pub(crate) async fn fetch_and_validate_turn(
    txn: &DatabaseTransaction,
    game_id: Uuid,
    player_id: Uuid,
) -> Result<(Vec<player::Model>, usize, usize), GameError> {
    let players = player::Entity::find()
        .filter(player::Column::GameId.eq(game_id))
        .order_by_asc(player::Column::Position)
        .all(txn)
        .await?;

    let player_position = players
        .iter()
        .find(|p| p.id == player_id)
        .map(|p| p.position as usize)
        .ok_or(GameError::PlayerNotFound)?;

    let active_count = players.iter().filter(|p| !p.kicked).count();

    Ok((players, player_position, active_count))
}

pub(crate) async fn fetch_and_validate_card(
    txn: &DatabaseTransaction,
    player_id: Uuid,
    card_index: i32,
) -> Result<(game_card::Model, Vec<game_card::Model>), GameError> {
    let all_cards = game_card::Entity::find()
        .filter(game_card::Column::PlayerId.eq(player_id))
        .all(txn)
        .await?;

    let target = all_cards
        .iter()
        .find(|gc| gc.card_index == card_index && !gc.played)
        .ok_or(GameError::CardNotFound)?;

    Ok((target.clone(), all_cards))
}

pub(crate) fn validate_follows_suit(
    card_index: i32,
    current_winning_card: Option<i32>,
    player_cards: &[game_card::Model],
) -> bool {
    if let Some(winning_idx) = current_winning_card {
        if (winning_idx / 8) == (card_index / 8) {
            return true;
        }
        let has_same_suit = player_cards
            .iter()
            .any(|gc| !gc.played && (gc.card_index / 8) == (winning_idx / 8));
        if has_same_suit {
            return false;
        }
    }
    true
}
