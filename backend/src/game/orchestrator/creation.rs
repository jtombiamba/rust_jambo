use rand::Rng;
use sea_orm::DatabaseConnection;
use tracing;
use uuid::Uuid;

use super::GameOrchestrator;
use super::QuickGameOutcome;
use crate::api::dto::responses::PlayerInfoDto;
use crate::database::models::{GameStatus, Player, PlayerType};
use crate::database::repositories::{
    GameCardRepository, GameRepository, PlayerProfileRepository, PlayerRepository,
};
use crate::error::GameError;
use crate::game::distribution::distribute_cards;
use crate::observability::CorrelationId;

impl GameOrchestrator {
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
            .ok_or(GameError::ProfileNotFound)?;

        if let Some(frozen_until) = profile.frozen_until {
            if frozen_until > chrono::Utc::now() {
                return Err(GameError::AccountFrozen {
                    until: frozen_until.to_rfc3339(),
                });
            }
        }

        if profile.credit < SOLO_BET {
            return Err(GameError::InsufficientCredits {
                required: SOLO_BET,
                current: profile.credit,
            });
        }

        let new_credit = profile.credit - SOLO_BET;
        let freeze_duration =
            chrono::Duration::seconds(self.game_service.freeze_duration_secs as i64);
        let was_previously_frozen = profile.frozen_until.is_some();

        let (final_credit, frozen_until) = if new_credit <= 0 {
            (new_credit, Some(chrono::Utc::now() + freeze_duration))
        } else if was_previously_frozen {
            let auto_unfreeze_credit = if new_credit < self.game_service.unfreeze_credit_no_payment
            {
                self.game_service.unfreeze_credit_no_payment
            } else {
                new_credit
            };
            (auto_unfreeze_credit, None)
        } else {
            (new_credit, profile.frozen_until)
        };

        profile_repo
            .update_credit_and_frozen_until(user_id, final_credit, frozen_until)
            .await
            .map_err(GameError::Database)?;

        if was_previously_frozen && frozen_until.is_none() {
            let _ = self.game_service.send_unfreeze_email(user_id).await;
        }

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
            .update_credits(human_player.id, final_credit)
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

    pub async fn create_bot_only_game(&self) -> Result<QuickGameOutcome, GameError> {
        let span = tracing::info_span!("create_bot_only_game");
        let _guard = span.enter();

        let game_repo = GameRepository::new(self.db.clone());
        let player_repo = PlayerRepository::new(self.db.clone());
        let card_repo = GameCardRepository::new(self.db.clone());

        let game = game_repo.create(10, false).await.map_err(|e| {
            tracing::error!("Failed to create game: {}", e);
            GameError::Database(e)
        })?;

        let bot_names = ["Bot South", "Bot East", "Bot North", "Bot West"];
        let mut bot_players = Vec::new();
        for (i, name) in bot_names.iter().enumerate() {
            let position = i as i32;
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

        let all_players: Vec<&Player> = bot_players.iter().collect();
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

        let players_json: Vec<PlayerInfoDto> = all_players
            .iter()
            .map(|player| {
                let player_type = match player.player_type {
                    PlayerType::Human => "human",
                    PlayerType::Bot => "bot",
                };
                PlayerInfoDto {
                    id: player.id,
                    player_type: player_type.to_string(),
                    name: player.name.clone(),
                    position: player.position,
                    display_position: player.position,
                    cards: Vec::new(),
                    cards_count: 5,
                    is_current_user: false,
                }
            })
            .collect();

        self.bot_scheduler
            .schedule_if_next_bot(game.id, player_ids[initial_rank as usize], None)
            .await;

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
