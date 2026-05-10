use async_trait::async_trait;
use rand::Rng;
use sea_orm::DatabaseConnection;
use tracing;
use uuid::Uuid;

use crate::api::dto::responses::{
    MultiplayerGameResponse, PlayCardResponse, PlayerInfoDto, QuickGameResponse,
};
use crate::database::models::{GameStatus, Player, PlayerType};
use crate::database::repositories::{
    GameCardRepository, GameRepository, PlayerProfileRepository, PlayerRepository,
};
use crate::error::GameError;
use crate::game::bot_scheduler::BotScheduler;
use crate::game::distribution::distribute_cards;
use crate::game::service::{CardPlayResult, GameService, MultiplayerGameOutcome};
use crate::messaging::{RabbitMQClient, RedisClient};
use crate::observability::CorrelationId;

/// Outcome of a play_card operation — contains everything the API handler
/// needs to build the HTTP response without accessing repositories.
#[derive(Debug, Clone)]
pub struct PlayCardOutcome {
    pub card_id: Uuid,
    pub next_turn: Option<Uuid>,
    pub game_ended: bool,
    pub round_completed: bool,
    pub current_round: i32,
}

/// Outcome of a create_quick_game operation.
#[derive(Debug, Clone)]
pub struct QuickGameOutcome {
    pub game_id: Uuid,
    pub players: Vec<PlayerInfoDto>,
    pub status: String,
    pub current_turn: i32,
    pub bet: i32,
    pub max_players: i32,
    pub invite_expires_at: Option<String>,
    pub deck_slots: Option<Vec<Option<i32>>>,
}

/// Outcome of a create_multiplayer_game operation.
#[derive(Debug, Clone)]
pub struct MultiplayerCreationOutcome {
    pub game_id: Uuid,
    pub status: String,
    pub bet: i32,
    pub max_players: i16,
    pub invite_expires_at: String,
}

/// Outcome of accepting an invite.
#[derive(Debug, Clone)]
pub struct AcceptInviteOutcome {
    pub player_id: Uuid,
    pub position: i32,
    pub player_count: i32,
    pub max_players: i32,
    pub game_status: String,
}

impl From<MultiplayerCreationOutcome> for MultiplayerGameResponse {
    fn from(o: MultiplayerCreationOutcome) -> Self {
        MultiplayerGameResponse {
            game_id: o.game_id,
            status: o.status,
            bet: o.bet,
            max_players: o.max_players,
            invite_expires_at: o.invite_expires_at,
        }
    }
}

impl From<PlayCardOutcome> for PlayCardResponse {
    fn from(o: PlayCardOutcome) -> Self {
        PlayCardResponse {
            success: true,
            message: "Card played successfully".to_string(),
            card_id: o.card_id,
            next_turn: o.next_turn,
            round_completed: o.round_completed,
            game_ended: o.game_ended,
            current_round: o.current_round,
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
            max_players: o.max_players,
            invite_expires_at: o.invite_expires_at,
            deck_slots: o.deck_slots,
        }
    }
}

/// Trait abstracting game orchestration so handlers can be tested
/// with a mock implementation without a database.
#[allow(dead_code)]
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

    async fn create_quick_game_for_user(
        &self,
        user_id: Uuid,
        db: &DatabaseConnection,
    ) -> Result<QuickGameOutcome, GameError>;

    async fn create_multiplayer_game(
        &self,
        user_id: Uuid,
        pseudo: &str,
        bet: i32,
        max_players: i16,
    ) -> Result<MultiplayerCreationOutcome, GameError>;

    async fn start_game(&self, game_id: Uuid, user_id: Uuid) -> Result<(), GameError>;

    async fn send_invites(
        &self,
        game_id: Uuid,
        creator_user_id: Uuid,
        invited_user_ids: Vec<Uuid>,
    ) -> Result<(), GameError>;

    async fn accept_invite(
        &self,
        game_id: Uuid,
        user_id: Uuid,
        pseudo: &str,
    ) -> Result<AcceptInviteOutcome, GameError>;

    async fn decline_invite(&self, game_id: Uuid, user_id: Uuid) -> Result<(), GameError>;

    #[allow(dead_code)]
    async fn cancel_game(&self, game_id: Uuid) -> Result<(), GameError>;
}

/// Thin orchestration layer between API handlers and domain services.
/// No repository calls leak into `api/` — handlers call the orchestrator,
/// and the orchestrator coordinates `GameService` and `BotScheduler`.
pub struct GameOrchestrator {
    db: DatabaseConnection,
    game_service: GameService,
    bot_scheduler: BotScheduler,
}

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

        let player_count = crate::database::repositories::PlayerRepository::new(self.db.clone())
            .list_by_game(game_id)
            .await
            .map_err(GameError::Database)?
            .len() as i32;

        let game = crate::database::repositories::GameRepository::new(self.db.clone())
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

    /// Create a quick game with 1 human + 3 bots, distribute cards,
    /// set a random initial rank, activate, and kick off bot chain if needed.
    /// Create a quick game linked to an authenticated user.
    /// The human player will be linked to the user's account.
    pub async fn create_quick_game_for_user(
        &self,
        user_id: Uuid,
        _db: &DatabaseConnection,
    ) -> Result<QuickGameOutcome, GameError> {
        let game_repo = GameRepository::new(self.db.clone());
        let player_repo = PlayerRepository::new(self.db.clone());
        let card_repo = GameCardRepository::new(self.db.clone());
        let profile_repo = PlayerProfileRepository::new(self.db.clone());
        const SOLO_BET: i32 = 10;

        let profile = profile_repo
            .find_by_user_id(user_id)
            .await
            .map_err(GameError::Database)?
            .ok_or_else(|| {
                GameError::Internal(Box::new(std::io::Error::other("Player profile not found")))
            })?;

        if profile.credit < SOLO_BET {
            return Err(GameError::InsufficientCredits);
        }

        let new_credit = profile.credit - SOLO_BET;
        profile_repo
            .update_credit(user_id, new_credit)
            .await
            .map_err(GameError::Database)?;

        let game = game_repo.create(10, false).await.map_err(|e| {
            tracing::error!("Failed to create game: {}", e);
            GameError::Database(e)
        })?;

        let human_player = player_repo
            .create_with_user(game.id, PlayerType::Human, "You", 0, user_id)
            .await
            .map_err(|e| {
                tracing::error!("Failed to create human player: {}", e);
                GameError::Database(e)
            })?;

        player_repo
            .update_credits(human_player.id, new_credit)
            .await
            .map_err(|e| {
                tracing::error!("Failed to update player credits: {}", e);
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
                    display_position: player.position,
                    cards,
                    cards_count: 5,
                    is_current_user: matches!(player.player_type, PlayerType::Human),
                }
            })
            .collect();

        if let Some(first_player) = all_players.iter().find(|p| p.position == initial_rank) {
            if matches!(first_player.player_type, PlayerType::Bot) {
                tracing::info!(
                    "First player is bot (position {}), scheduling initial move",
                    initial_rank
                );
                self.bot_scheduler
                    .schedule_if_next_bot(game.id, first_player.id, None)
                    .await;
            }
        }

        Ok(QuickGameOutcome {
            game_id: game.id,
            players: players_json,
            status: "active".to_string(),
            current_turn: initial_rank,
            bet: 10,
            max_players: 4,
            invite_expires_at: None,
            deck_slots: None,
        })
    }

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
                    display_position: player.position,
                    cards,
                    cards_count: 5,
                    is_current_user: matches!(player.player_type, PlayerType::Human),
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
            max_players: 4,
            invite_expires_at: None,
            deck_slots: None,
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

#[cfg(test)]
pub mod mock {
    use super::*;
    use std::sync::Mutex;

    #[allow(dead_code)]
    pub struct MockGameOrchestrator {
        play_card_result: Mutex<Option<Result<PlayCardOutcome, GameError>>>,
        create_quick_game_result: Mutex<Option<Result<QuickGameOutcome, GameError>>>,
        create_multiplayer_game_result:
            Mutex<Option<Result<MultiplayerCreationOutcome, GameError>>>,
        send_invites_result: Mutex<Option<Result<(), GameError>>>,
        accept_invite_result: Mutex<Option<Result<AcceptInviteOutcome, GameError>>>,
        cancel_game_result: Mutex<Option<Result<(), GameError>>>,
        start_game_result: Mutex<Option<Result<(), GameError>>>,
    }

    impl MockGameOrchestrator {
        pub fn new(
            play_card_result: Result<PlayCardOutcome, GameError>,
            create_quick_game_result: Result<QuickGameOutcome, GameError>,
        ) -> Self {
            Self {
                play_card_result: Mutex::new(Some(play_card_result)),
                create_quick_game_result: Mutex::new(Some(create_quick_game_result)),
                create_multiplayer_game_result: Mutex::new(None),
                send_invites_result: Mutex::new(None),
                accept_invite_result: Mutex::new(None),
                cancel_game_result: Mutex::new(None),
                start_game_result: Mutex::new(None),
            }
        }

        pub fn ok() -> Self {
            let play_outcome = PlayCardOutcome {
                card_id: Uuid::new_v4(),
                next_turn: Some(Uuid::new_v4()),
                game_ended: false,
                round_completed: false,
                current_round: 1,
            };
            let quick_outcome = QuickGameOutcome {
                game_id: Uuid::new_v4(),
                players: vec![],
                status: "active".to_string(),
                current_turn: 0,
                bet: 10,
                max_players: 4,
                invite_expires_at: None,
                deck_slots: None,
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

        async fn create_quick_game_for_user(
            &self,
            _user_id: Uuid,
            _db: &DatabaseConnection,
        ) -> Result<QuickGameOutcome, GameError> {
            self.create_quick_game_result
                .lock()
                .unwrap()
                .take()
                .expect("mock orchestrator create_quick_game_for_user called more than once")
        }

        async fn create_multiplayer_game(
            &self,
            _user_id: Uuid,
            _pseudo: &str,
            _bet: i32,
            _max_players: i16,
        ) -> Result<MultiplayerCreationOutcome, GameError> {
            self.create_multiplayer_game_result
                .lock()
                .unwrap()
                .take()
                .expect("mock orchestrator create_multiplayer_game called more than once")
        }

        async fn start_game(&self, _game_id: Uuid, _user_id: Uuid) -> Result<(), GameError> {
            self.start_game_result
                .lock()
                .unwrap()
                .take()
                .expect("mock orchestrator start_game called more than once")
        }

        async fn send_invites(
            &self,
            _game_id: Uuid,
            _creator_user_id: Uuid,
            _invited_user_ids: Vec<Uuid>,
        ) -> Result<(), GameError> {
            self.send_invites_result
                .lock()
                .unwrap()
                .take()
                .expect("mock orchestrator send_invites called more than once")
        }

        async fn accept_invite(
            &self,
            _game_id: Uuid,
            _user_id: Uuid,
            _pseudo: &str,
        ) -> Result<AcceptInviteOutcome, GameError> {
            self.accept_invite_result
                .lock()
                .unwrap()
                .take()
                .expect("mock orchestrator accept_invite called more than once")
        }

        async fn decline_invite(&self, _game_id: Uuid, _user_id: Uuid) -> Result<(), GameError> {
            Ok(())
        }

        async fn cancel_game(&self, _game_id: Uuid) -> Result<(), GameError> {
            self.cancel_game_result
                .lock()
                .unwrap()
                .take()
                .expect("mock orchestrator cancel_game called more than once")
        }
    }
}
