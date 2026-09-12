use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, PaginatorTrait, QueryFilter};
use uuid::Uuid;

use crate::database::models::{game, game_invite, player, GameStatus, InviteStatus};
use crate::error::GameError;

pub(crate) fn validate_game_pending(game: &game::Model) -> Result<(), GameError> {
    if game.status != GameStatus::Pending {
        return Err(GameError::GameNotPending);
    }
    Ok(())
}

pub(crate) fn validate_not_creator(game: &game::Model, user_id: Uuid) -> Result<(), GameError> {
    if Some(user_id) == game.creator_id {
        return Err(GameError::CreatorCannotJoin);
    }
    Ok(())
}

pub(crate) async fn validate_not_already_in_game<C: ConnectionTrait>(
    txn: &C,
    game_id: Uuid,
    user_id: Uuid,
) -> Result<(), GameError> {
    let existing = player::Entity::find()
        .filter(player::Column::GameId.eq(game_id))
        .filter(player::Column::UserId.eq(user_id))
        .one(txn)
        .await?;
    if existing.is_some() {
        return Err(GameError::AlreadyJoined);
    }
    Ok(())
}

pub(crate) async fn validate_pending_invite_exists<C: ConnectionTrait>(
    txn: &C,
    game_id: Uuid,
    user_id: Uuid,
) -> Result<game_invite::Model, GameError> {
    game_invite::Entity::find()
        .filter(game_invite::Column::GameId.eq(game_id))
        .filter(game_invite::Column::InvitedUserId.eq(user_id))
        .filter(game_invite::Column::Status.eq(InviteStatus::Pending))
        .one(txn)
        .await?
        .ok_or(GameError::NotInvited)
}

pub(crate) async fn validate_game_not_full<C: ConnectionTrait>(
    txn: &C,
    game_id: Uuid,
    max_players: i16,
) -> Result<u64, GameError> {
    let count: u64 = player::Entity::find()
        .filter(player::Column::GameId.eq(game_id))
        .count(txn)
        .await?;
    if count >= max_players as u64 {
        return Err(GameError::GameFull);
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_game(status: GameStatus, creator_id: Option<Uuid>) -> game::Model {
        game::Model {
            id: Uuid::now_v7(),
            status,
            bet: 100,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            finished_at: None,
            rank: None,
            roll: 0,
            auto: false,
            winner_id: None,
            player_positions: serde_json::Value::Null,
            current_winning_card: None,
            current_winning_player_position: None,
            creator_id,
            game_mode: crate::database::models::GameMode::Multiplayer,
            max_players: 4,
            invite_expires_at: None,
            stall_warning_sent_at: None,
            game_run_id: None,
            step_by_step: false,
            kicked_players: serde_json::Value::Null,
        }
    }

    #[test]
    fn test_validate_game_pending_ok() {
        let game = make_game(GameStatus::Pending, None);
        assert!(validate_game_pending(&game).is_ok());
    }

    #[test]
    fn test_validate_game_pending_rejects_non_pending() {
        let game = make_game(GameStatus::Active, None);
        assert!(matches!(
            validate_game_pending(&game),
            Err(GameError::GameNotPending)
        ));
    }

    #[test]
    fn test_validate_not_creator_ok() {
        let creator = Uuid::now_v7();
        let other = Uuid::now_v7();
        let game = make_game(GameStatus::Pending, Some(creator));
        assert!(validate_not_creator(&game, other).is_ok());
    }

    #[test]
    fn test_validate_not_creator_rejects_creator() {
        let creator = Uuid::now_v7();
        let game = make_game(GameStatus::Pending, Some(creator));
        assert!(matches!(
            validate_not_creator(&game, creator),
            Err(GameError::CreatorCannotJoin)
        ));
    }
}
