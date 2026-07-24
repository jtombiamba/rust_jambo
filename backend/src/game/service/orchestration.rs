use uuid::Uuid;

use crate::database::models::PlayerType;
use crate::database::repositories::{GameRepository, PlayerRepository};
use crate::error::GameError;
use crate::game::bot_scheduler::BotScheduler;
use crate::game::service::types::BotMoveOutcome;

use super::GameService;

impl GameService {
    /// Advance a bot in step-by-step mode. Validates the game state and
    /// executes a single bot move synchronously.
    pub async fn advance_bot(
        &self,
        game_id: Uuid,
        human_player_id: Uuid,
    ) -> Result<BotMoveOutcome, GameError> {
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

        let current_player_id = self.next_player(game_id).await?;

        let current_player = players
            .iter()
            .find(|p| p.id == current_player_id)
            .ok_or_else(|| GameError::internal("Current player not found in game player list"))?;

        if !matches!(current_player.player_type, PlayerType::Bot) {
            return Err(GameError::NotABot);
        }

        BotScheduler::execute_one_bot_move(
            &self.db,
            &self.redis_client,
            game_id,
            current_player_id,
            self.freeze_duration_secs,
            self.unfreeze_credit_no_payment,
            None,
        )
        .await
    }

    /// Verify that the given user_id owns the specified player in the game.
    /// Returns Ok(true) if the player belongs to the user or the player is a
    /// bot linked to the user, Ok(false) if the player belongs to
    /// a different user, or Err if game/player not found.
    pub async fn verify_player_ownership(
        &self,
        game_id: Uuid,
        player_id: Uuid,
        user_id: Uuid,
    ) -> Result<bool, GameError> {
        let players = PlayerRepository::new(self.db.clone())
            .list_by_game(game_id)
            .await?;

        let player = players
            .iter()
            .find(|p| p.id == player_id)
            .ok_or(GameError::PlayerNotFound)?;

        if matches!(player.player_type, PlayerType::Bot) {
            return Ok(true);
        }

        Ok(player.user_id == Some(user_id))
    }
}
