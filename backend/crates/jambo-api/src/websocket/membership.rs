use actix_web::{self, web};
use uuid::Uuid;

/// Enforce that a cookie-authenticated user may open a game's WebSocket.
/// Anonymous game-token and spectator connections (`user_id == None`) bypass
/// the check entirely.
pub(super) async fn enforce_participation(
    db: Option<web::Data<sea_orm::DatabaseConnection>>,
    user_id: Option<Uuid>,
    game_id: Uuid,
) -> Result<(), actix_web::Error> {
    let (Some(uid), Some(db_ref)) = (user_id, db) else {
        return Ok(());
    };

    match check_game_membership(db_ref.get_ref(), game_id, uid).await {
        Ok(true) => Ok(()),
        Ok(false) => {
            tracing::warn!(
                "User {} is not a participant of game {}, rejecting WebSocket connection",
                uid,
                game_id
            );
            Err(actix_web::error::ErrorForbidden(
                "Not a participant of this game",
            ))
        }
        Err(e) => {
            tracing::error!(
                "Failed to check membership for user {} in game {}: {}",
                uid,
                game_id,
                e
            );
            Err(actix_web::error::ErrorInternalServerError(
                "Membership check failed",
            ))
        }
    }
}

/// Resolve the player row (id, position) belonging to an authenticated user in a
/// game. Returns `Ok(None)` when the user is not a player in this game.
pub(super) async fn resolve_player_for_user(
    db: &sea_orm::DatabaseConnection,
    game_id: Uuid,
    user_id: Uuid,
) -> Result<Option<(Uuid, i32)>, sea_orm::DbErr> {
    let player = jambo_db::repositories::PlayerRepository::new(db.clone())
        .find_by_game_and_user(game_id, user_id)
        .await?;
    Ok(player.map(|p| (p.id, p.position)))
}

/// Determine whether an authenticated user may open a game's WebSocket. Allowed
/// for the creator, an accepted player, or (while the game is still pending) an
/// invitee who has not yet responded.
async fn check_game_membership(
    db: &sea_orm::DatabaseConnection,
    game_id: Uuid,
    user_id: Uuid,
) -> Result<bool, sea_orm::DbErr> {
    use jambo_db::models::GameStatus;
    use jambo_db::repositories::{GameInviteRepository, GameRepository, PlayerRepository};

    let game = GameRepository::new(db.clone()).find_by_id(game_id).await?;
    let Some(game) = game else {
        return Ok(false);
    };
    if game.creator_id == Some(user_id) {
        return Ok(true);
    }
    if PlayerRepository::new(db.clone())
        .find_by_game_and_user(game_id, user_id)
        .await?
        .is_some()
    {
        return Ok(true);
    }
    if game.status == GameStatus::Pending
        && GameInviteRepository::new(db.clone())
            .find_invite(game_id, user_id)
            .await?
            .is_some()
    {
        return Ok(true);
    }
    Ok(false)
}
