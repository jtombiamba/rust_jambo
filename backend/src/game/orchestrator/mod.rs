mod creation;
#[cfg(test)]
pub mod mock;
mod types;

pub use types::{
    AcceptInviteOutcome, GameOrchestratorTrait, MultiplayerCreationOutcome, PlayCardOutcome,
    QuickGameOutcome,
};

use async_trait::async_trait;
use sea_orm::DatabaseConnection;
use tracing;
use uuid::Uuid;

use crate::database::models::{GameStatus, PlayerType};
use crate::database::repositories::{GameRepository, PlayerRepository};
use crate::error::GameError;
use crate::game::bot_scheduler::BotScheduler;
use crate::game::service::{CardPlayResult, GameService, MultiplayerGameOutcome};
use crate::messaging::{RabbitMQClient, RedisClient};
use crate::observability::CorrelationId;

fn map_service_error(e: crate::game::service::GameServiceError) -> GameError {
    use crate::game::service::GameServiceError;
    match e {
        GameServiceError::GameNotFound => GameError::GameNotFound,
        GameServiceError::PlayerNotFound => GameError::PlayerNotFound,
        GameServiceError::CardNotFound => GameError::CardNotFound,
        GameServiceError::NotYourTurn => GameError::NotYourTurn,
        GameServiceError::InvalidCard => GameError::InvalidCard,
        GameServiceError::GameFinished => GameError::GameFinished,
        GameServiceError::RoundNotComplete => GameError::RoundNotComplete,
        GameServiceError::InsufficientCredits => GameError::InsufficientCredits,
        GameServiceError::GameNotPending => GameError::GameNotPending,
        GameServiceError::NotCreator => GameError::NotCreator,
        GameServiceError::NotInvited => GameError::NotInvited,
        GameServiceError::AlreadyJoined => GameError::AlreadyJoined,
        GameServiceError::DuplicatePlayer => GameError::AlreadyJoined,
        GameServiceError::GameFull => GameError::GameFull,
        GameServiceError::InviteExpired => GameError::InviteExpired,
        GameServiceError::CreatorCannotJoin => GameError::CreatorCannotJoin,
        GameServiceError::GameNotReady => GameError::GameNotReady,
        GameServiceError::Internal(msg) => {
            GameError::Internal(Box::new(std::io::Error::other(msg)))
        }
        GameServiceError::Database(e) => GameError::Internal(e),
    }
}

/// Thin orchestration layer between API handlers and domain services.
/// No repository calls leak into `api/` — handlers call the orchestrator,
/// and the orchestrator coordinates `GameService` and `BotScheduler`.
pub struct GameOrchestrator {
    db: DatabaseConnection,
    game_service: GameService,
    bot_scheduler: BotScheduler,
}

impl GameOrchestrator {
    pub fn new(
        db: DatabaseConnection,
        redis: Option<RedisClient>,
        rabbitmq: Option<RabbitMQClient>,
    ) -> Self {
        let game_service = GameService::new_with_redis(db.clone(), redis.clone());
        let bot_scheduler = BotScheduler::new(db.clone(), rabbitmq, redis);
        Self {
            db,
            game_service,
            bot_scheduler,
        }
    }

    /// Validate and record a card play, then schedule bot moves if needed.
    /// Returns enough information for the API handler to build the response
    /// without any additional database queries.
    pub async fn play_card(
        &self,
        game_id: Uuid,
        player_id: Uuid,
        card_index: i32,
        correlation_id: Option<CorrelationId>,
    ) -> Result<PlayCardOutcome, GameError> {
        let cid_str = correlation_id.map(|c| c.to_string()).unwrap_or_default();
        let span = tracing::info_span!(
            "orchestrate_card_play",
            correlation_id = %cid_str,
            game_id = %game_id,
            player_id = %player_id,
            card_index = card_index,
        );
        let _guard = span.enter();

        let result: CardPlayResult = self
            .game_service
            .update_card_play(game_id, player_id, card_index, correlation_id.map(|c| c.0))
            .await
            .map_err(map_service_error)?;

        let next_is_bot = result
            .players
            .iter()
            .find(|p| p.id == result.next_player_id)
            .map(|p| matches!(p.player_type, PlayerType::Bot))
            .unwrap_or(false);

        if !result.game_ended && next_is_bot {
            self.bot_scheduler
                .schedule_if_next_bot(game_id, result.next_player_id, correlation_id)
                .await;
        }

        let next_turn = if result.game_ended {
            None
        } else {
            Some(result.next_player_id)
        };

        Ok(PlayCardOutcome {
            card_id: result.card.id,
            next_turn,
            game_ended: result.game_ended,
            round_completed: result.round_completed,
            current_round: result.current_round,
        })
    }

    pub async fn create_multiplayer_game(
        &self,
        user_id: Uuid,
        pseudo: &str,
        bet: i32,
        max_players: i16,
    ) -> Result<MultiplayerCreationOutcome, GameError> {
        let outcome: MultiplayerGameOutcome = self
            .game_service
            .create_multiplayer_game(user_id, pseudo, bet, max_players)
            .await
            .map_err(map_service_error)?;

        Ok(MultiplayerCreationOutcome {
            game_id: outcome.game_id,
            status: "pending".to_string(),
            bet: outcome.bet,
            max_players: outcome.max_players,
            invite_expires_at: outcome.invite_expires_at.to_rfc3339(),
        })
    }

    pub async fn start_game(&self, game_id: Uuid, user_id: Uuid) -> Result<(), GameError> {
        self.game_service
            .start_game(game_id, user_id)
            .await
            .map_err(map_service_error)
    }

    pub async fn send_invites(
        &self,
        game_id: Uuid,
        creator_user_id: Uuid,
        invited_user_ids: Vec<Uuid>,
    ) -> Result<(), GameError> {
        self.game_service
            .send_invites(game_id, creator_user_id, &invited_user_ids)
            .await
            .map_err(map_service_error)
    }

    pub async fn accept_invite(
        &self,
        game_id: Uuid,
        user_id: Uuid,
        pseudo: &str,
    ) -> Result<AcceptInviteOutcome, GameError> {
        let player = self
            .game_service
            .accept_invite(game_id, user_id, pseudo)
            .await
            .map_err(map_service_error)?;

        let player_count = PlayerRepository::new(self.db.clone())
            .list_by_game(game_id)
            .await
            .map_err(GameError::Database)?
            .len() as i32;

        let game = GameRepository::new(self.db.clone())
            .find_by_id(game_id)
            .await
            .map_err(GameError::Database)?
            .ok_or(GameError::GameNotFound)?;

        Ok(AcceptInviteOutcome {
            player_id: player.id,
            position: player.position,
            player_count,
            max_players: game.max_players as i32,
            game_status: match game.status {
                GameStatus::Ready => "ready".to_string(),
                _ => "pending".to_string(),
            },
        })
    }

    pub async fn decline_invite(&self, game_id: Uuid, user_id: Uuid) -> Result<(), GameError> {
        self.game_service
            .decline_invite(game_id, user_id)
            .await
            .map_err(map_service_error)
    }

    #[allow(dead_code)]
    pub async fn cancel_game(&self, game_id: Uuid) -> Result<(), GameError> {
        self.game_service
            .cancel_game(game_id)
            .await
            .map_err(map_service_error)
    }
}

#[async_trait]
impl GameOrchestratorTrait for GameOrchestrator {
    async fn play_card(
        &self,
        game_id: Uuid,
        player_id: Uuid,
        card_index: i32,
        correlation_id: Option<CorrelationId>,
    ) -> Result<PlayCardOutcome, GameError> {
        self.play_card(game_id, player_id, card_index, correlation_id)
            .await
    }

    async fn create_quick_game(
        &self,
        correlation_id: Option<CorrelationId>,
    ) -> Result<QuickGameOutcome, GameError> {
        self.create_quick_game(correlation_id).await
    }

    async fn create_bot_only_game(&self) -> Result<QuickGameOutcome, GameError> {
        self.create_bot_only_game().await
    }

    async fn create_quick_game_for_user(
        &self,
        user_id: Uuid,
        db: &DatabaseConnection,
    ) -> Result<QuickGameOutcome, GameError> {
        self.create_quick_game_for_user(user_id, db).await
    }

    async fn create_multiplayer_game(
        &self,
        user_id: Uuid,
        pseudo: &str,
        bet: i32,
        max_players: i16,
    ) -> Result<MultiplayerCreationOutcome, GameError> {
        self.create_multiplayer_game(user_id, pseudo, bet, max_players)
            .await
    }

    async fn start_game(&self, game_id: Uuid, user_id: Uuid) -> Result<(), GameError> {
        self.start_game(game_id, user_id).await
    }

    async fn send_invites(
        &self,
        game_id: Uuid,
        creator_user_id: Uuid,
        invited_user_ids: Vec<Uuid>,
    ) -> Result<(), GameError> {
        self.send_invites(game_id, creator_user_id, invited_user_ids)
            .await
    }

    async fn accept_invite(
        &self,
        game_id: Uuid,
        user_id: Uuid,
        pseudo: &str,
    ) -> Result<AcceptInviteOutcome, GameError> {
        self.accept_invite(game_id, user_id, pseudo).await
    }

    async fn decline_invite(&self, game_id: Uuid, user_id: Uuid) -> Result<(), GameError> {
        self.decline_invite(game_id, user_id).await
    }

    async fn cancel_game(&self, game_id: Uuid) -> Result<(), GameError> {
        self.cancel_game(game_id).await
    }
}
