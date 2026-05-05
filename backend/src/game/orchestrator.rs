use async_trait::async_trait;
use rand::Rng;
use sea_orm::DatabaseConnection;
use tracing;
use uuid::Uuid;

use crate::api::dto::responses::{PlayCardResponse, PlayerInfoDto, QuickGameResponse};
use crate::database::models::{GameStatus, Player, PlayerType};
use crate::database::repositories::{GameCardRepository, GameRepository, PlayerRepository};
use crate::error::GameError;
use crate::game::bot_scheduler::BotScheduler;
use crate::game::distribution::distribute_cards;
use crate::game::service::{CardPlayResult, GameService};
use crate::messaging::{RabbitMQClient, RedisClient};
use crate::observability::CorrelationId;

/// Outcome of a play_card operation — contains everything the API handler
/// needs to build the HTTP response without accessing repositories.
#[derive(Debug, Clone)]
pub struct PlayCardOutcome {
    pub card_id: Uuid,
    pub next_turn: Option<Uuid>,
    pub game_ended: bool,
}

/// Outcome of a create_quick_game operation.
#[derive(Debug, Clone)]
pub struct QuickGameOutcome {
    pub game_id: Uuid,
    pub players: Vec<PlayerInfoDto>,
    pub status: String,
    pub current_turn: i32,
    pub bet: i32,
}

impl From<PlayCardOutcome> for PlayCardResponse {
    fn from(o: PlayCardOutcome) -> Self {
        PlayCardResponse {
            success: true,
            message: "Card played successfully".to_string(),
            card_id: o.card_id,
            next_turn: o.next_turn,
        }
    }
}

impl From<QuickGameOutcome> for QuickGameResponse {
    fn from(o: QuickGameOutcome) -> Self {
        QuickGameResponse {
            game_id: o.game_id,
            players: o.players,
            status: o.status,
            current_turn: o.current_turn,
            bet: o.bet,
        }
    }
}

/// Trait abstracting game orchestration so handlers can be tested
/// with a mock implementation without a database.
#[async_trait]
pub trait GameOrchestratorTrait: Send + Sync + 'static {
    async fn play_card(
        &self,
        game_id: Uuid,
        player_id: Uuid,
        card_index: i32,
        correlation_id: Option<CorrelationId>,
    ) -> Result<PlayCardOutcome, GameError>;

    async fn create_quick_game(
        &self,
        correlation_id: Option<CorrelationId>,
    ) -> Result<QuickGameOutcome, GameError>;
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
            .map_err(|e| match e {
                crate::game::service::GameServiceError::GameNotFound => GameError::GameNotFound,
                crate::game::service::GameServiceError::PlayerNotFound => GameError::PlayerNotFound,
                crate::game::service::GameServiceError::CardNotFound => GameError::CardNotFound,
                crate::game::service::GameServiceError::NotYourTurn => GameError::NotYourTurn,
                crate::game::service::GameServiceError::InvalidCard => GameError::InvalidCard,
                crate::game::service::GameServiceError::GameFinished => GameError::GameFinished,
                crate::game::service::GameServiceError::RoundNotComplete => {
                    GameError::RoundNotComplete
                }
                crate::game::service::GameServiceError::Internal(msg) => {
                    GameError::Internal(Box::new(std::io::Error::other(msg)))
                }
                crate::game::service::GameServiceError::Database(e) => GameError::Internal(e),
            })?;

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
        })
    }

    /// Create a quick game with 1 human + 3 bots, distribute cards,
    /// set a random initial rank, activate, and kick off bot chain if needed.
    pub async fn create_quick_game(
        &self,
        correlation_id: Option<CorrelationId>,
    ) -> Result<QuickGameOutcome, GameError> {
        let cid_str = correlation_id.map(|c| c.to_string()).unwrap_or_default();
        let span = tracing::info_span!(
            "create_quick_game",
            correlation_id = %cid_str,
        );
        let _guard = span.enter();

        let game_repo = GameRepository::new(self.db.clone());
        let player_repo = PlayerRepository::new(self.db.clone());
        let card_repo = GameCardRepository::new(self.db.clone());

        let game = game_repo.create(10, false).await.map_err(|e| {
            tracing::error!("Failed to create game: {}", e);
            GameError::Database(e)
        })?;

        let human_player = player_repo
            .create(game.id, PlayerType::Human, "You", 0)
            .await
            .map_err(|e| {
                tracing::error!("Failed to create human player: {}", e);
                GameError::Database(e)
            })?;

        let bot_names = ["Bot East", "Bot North", "Bot West"];
        let mut bot_players = Vec::new();
        for (i, name) in bot_names.iter().enumerate() {
            let position = (i + 1) as i32;
            match player_repo
                .create(game.id, PlayerType::Bot, name, position)
                .await
            {
                Ok(player) => bot_players.push(player),
                Err(e) => {
                    tracing::error!("Failed to create bot player {}: {}", name, e);
                    return Err(GameError::Database(e));
                }
            }
        }

        let all_players: Vec<&Player> = std::iter::once(&human_player)
            .chain(bot_players.iter())
            .collect();
        let player_ids: Vec<Uuid> = all_players.iter().map(|p| p.id).collect();

        let initial_rank = rand::thread_rng().gen_range(0..4) as i32;
        game_repo
            .update_rank(game.id, Some(initial_rank))
            .await
            .map_err(|e| {
                tracing::error!("Failed to set initial rank: {}", e);
                GameError::Database(e)
            })?;

        game_repo
            .update_status(game.id, GameStatus::Active)
            .await
            .map_err(|e| {
                tracing::error!("Failed to activate game: {}", e);
                GameError::Database(e)
            })?;

        let card_assignments = distribute_cards(&player_ids);
        for &(player_id, card_index) in &card_assignments {
            card_repo
                .create(game.id, Some(player_id), card_index, None)
                .await
                .map_err(|e| {
                    tracing::error!("Failed to create game card: {}", e);
                    GameError::Database(e)
                })?;
        }

        // Log bot cards for debugging
        for bot in &bot_players {
            let bot_cards: Vec<i32> = card_assignments
                .iter()
                .filter(|(pid, _)| *pid == bot.id)
                .map(|(_, card)| *card)
                .collect();
            tracing::info!(
                "Bot '{}' (player_id: {}) received cards: {:?}",
                bot.name,
                bot.id,
                bot_cards
            );
        }

        let human_cards: Vec<i32> = card_assignments
            .iter()
            .filter(|(pid, _)| *pid == human_player.id)
            .map(|(_, card)| *card)
            .collect();

        let players_json: Vec<PlayerInfoDto> = all_players
            .iter()
            .map(|player| {
                let player_type = match player.player_type {
                    PlayerType::Human => "human",
                    PlayerType::Bot => "bot",
                };
                let cards = if matches!(player.player_type, PlayerType::Human) {
                    human_cards.clone()
                } else {
                    Vec::new()
                };
                PlayerInfoDto {
                    id: player.id,
                    player_type: player_type.to_string(),
                    name: player.name.clone(),
                    position: player.position,
                    cards,
                    cards_count: 5,
                }
            })
            .collect();

        // Kick off bot chain if first player is a bot
        if let Some(first_player) = all_players.iter().find(|p| p.position == initial_rank) {
            if matches!(first_player.player_type, PlayerType::Bot) {
                tracing::info!(
                    "First player is bot (position {}), scheduling initial move",
                    initial_rank
                );
                self.bot_scheduler
                    .schedule_if_next_bot(game.id, first_player.id, correlation_id)
                    .await;
            }
        }

        Ok(QuickGameOutcome {
            game_id: game.id,
            players: players_json,
            status: "active".to_string(),
            current_turn: initial_rank,
            bet: 10,
        })
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
}

#[cfg(test)]
pub mod mock {
    use super::*;
    use std::sync::Mutex;

    pub struct MockGameOrchestrator {
        play_card_result: Mutex<Option<Result<PlayCardOutcome, GameError>>>,
        create_quick_game_result: Mutex<Option<Result<QuickGameOutcome, GameError>>>,
    }

    impl MockGameOrchestrator {
        pub fn new(
            play_card_result: Result<PlayCardOutcome, GameError>,
            create_quick_game_result: Result<QuickGameOutcome, GameError>,
        ) -> Self {
            Self {
                play_card_result: Mutex::new(Some(play_card_result)),
                create_quick_game_result: Mutex::new(Some(create_quick_game_result)),
            }
        }

        pub fn ok() -> Self {
            let play_outcome = PlayCardOutcome {
                card_id: Uuid::new_v4(),
                next_turn: Some(Uuid::new_v4()),
                game_ended: false,
            };
            let quick_outcome = QuickGameOutcome {
                game_id: Uuid::new_v4(),
                players: vec![],
                status: "active".to_string(),
                current_turn: 0,
                bet: 10,
            };
            Self::new(Ok(play_outcome), Ok(quick_outcome))
        }
    }

    #[async_trait]
    impl GameOrchestratorTrait for MockGameOrchestrator {
        async fn play_card(
            &self,
            _game_id: Uuid,
            _player_id: Uuid,
            _card_index: i32,
            _correlation_id: Option<CorrelationId>,
        ) -> Result<PlayCardOutcome, GameError> {
            self.play_card_result
                .lock()
                .unwrap()
                .take()
                .expect("mock orchestrator play_card called more than once")
        }

        async fn create_quick_game(
            &self,
            _correlation_id: Option<CorrelationId>,
        ) -> Result<QuickGameOutcome, GameError> {
            self.create_quick_game_result
                .lock()
                .unwrap()
                .take()
                .expect("mock orchestrator create_quick_game called more than once")
        }
    }
}
