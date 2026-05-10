use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, DatabaseConnection, DatabaseTransaction,
    EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, TransactionTrait,
};
use serde_json::json;
use thiserror::Error;
use uuid::Uuid;

use crate::database::models::Player;
use crate::database::models::{
    game, game_card, game_invite, player, player_profile, GameMode, GameStatus, InviteStatus,
    PlayerType,
};
use crate::database::repositories::{GameCardRepository, GameRepository, PlayerRepository};
use crate::game::card_mapping::Card;
use crate::game::constants::{CARDS_PER_PLAYER, TOTAL_CARDS};
use crate::game::payment::calculate_payment;
use crate::game::round_evaluation::{evaluate_round, PlayedCard, RoundContext};
use crate::game::turn_order::next_player;
use crate::messaging::ai_task::{AITask, PlayerInfo};
use crate::messaging::{events::GameEvent, RedisClient};
use tracing::{error, info};

pub fn compute_display_position(actual_pos: usize, num_players: usize, my_pos: usize) -> usize {
    (num_players + actual_pos - my_pos) % num_players
}

#[allow(dead_code)]
#[derive(Error, Debug)]
pub enum GameServiceError {
    #[error("Database error: {0}")]
    Database(#[from] Box<dyn std::error::Error + Send>),
    #[error("Game not found")]
    GameNotFound,
    #[error("Player not found")]
    PlayerNotFound,
    #[error("Card not found or already played")]
    CardNotFound,
    #[error("Not your turn to play")]
    NotYourTurn,
    #[error("Invalid card play: you must follow suit if possible")]
    InvalidCard,
    #[error("Round not complete")]
    RoundNotComplete,
    #[error("Game already finished")]
    GameFinished,
    #[error("Insufficient credits for bet")]
    InsufficientCredits,
    #[error("Game is not pending")]
    GameNotPending,
    #[error("User is not the game creator")]
    NotCreator,
    #[error("User is not invited to this game")]
    NotInvited,
    #[error("User is already a player in this game")]
    AlreadyJoined,
    #[error("Game is full")]
    GameFull,
    #[error("Invite has expired")]
    InviteExpired,
    #[error("Creator cannot join their own game")]
    CreatorCannotJoin,
    #[error("Game is not in ready state")]
    GameNotReady,
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Rich result returned by `update_card_play` after a successful card play.
/// Contains everything callers need (next player, player types, game status)
/// so they never have to query the database independently.
pub struct CardPlayResult {
    pub card: game_card::Model,
    pub next_player_id: Uuid,
    pub players: Vec<player::Model>,
    pub game_ended: bool,
    pub round_completed: bool,
    pub current_round: i32,
}

/// Result of creating a multiplayer game.
#[allow(dead_code)]
pub struct MultiplayerGameOutcome {
    pub game_id: Uuid,
    pub player_id: Uuid,
    pub status: GameStatus,
    pub bet: i32,
    pub max_players: i16,
    pub invite_expires_at: chrono::DateTime<Utc>,
}

/// Result of evaluating a completed round inside a transaction.
/// Carries all data needed for post-transaction event publishing and payment processing.
struct RoundEvaluationResult {
    round: i32,
    winner_id: Uuid,
    winner_position: usize,
    game_ended: bool,
    final_status: GameStatus,
    players: Vec<Player>,
}

pub struct GameService {
    db: DatabaseConnection,
    redis_client: Option<RedisClient>,
}

impl GameService {
    #[allow(dead_code)]
    pub fn new(db: DatabaseConnection) -> Self {
        Self {
            db,
            redis_client: None,
        }
    }

    pub fn new_with_redis(db: DatabaseConnection, redis_client: Option<RedisClient>) -> Self {
        Self { db, redis_client }
    }

    #[allow(dead_code)]
    pub fn redis_client(&self) -> Option<RedisClient> {
        self.redis_client.clone()
    }

    pub async fn create_multiplayer_game(
        &self,
        creator_user_id: Uuid,
        creator_pseudo: &str,
        bet: i32,
        max_players: i16,
    ) -> Result<MultiplayerGameOutcome, GameServiceError> {
        const INVITE_TIMEOUT_MINUTES: i64 = 6;

        let txn = self.db.begin().await.map_err(|e| {
            GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
        })?;

        let profile = player_profile::Entity::find()
            .filter(player_profile::Column::UserId.eq(creator_user_id))
            .one(&txn)
            .await
            .map_err(|e| {
                GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
            })?
            .ok_or_else(|| GameServiceError::Internal("Player profile not found".to_string()))?;

        if profile.credit < bet {
            txn.rollback().await.ok();
            return Err(GameServiceError::InsufficientCredits);
        }

        let creator_credit_before = profile.credit;
        let new_credit = creator_credit_before - bet;
        let mut profile_active: player_profile::ActiveModel = profile.into();
        profile_active.credit = sea_orm::ActiveValue::Set(new_credit);
        profile_active.updated_at = sea_orm::ActiveValue::Set(chrono::Utc::now());
        profile_active.update(&txn).await.map_err(|e| {
            GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
        })?;

        let game_id = Uuid::new_v4();
        let now = chrono::Utc::now();
        let expires_at = now + chrono::Duration::minutes(INVITE_TIMEOUT_MINUTES);

        let game_active = game::ActiveModel {
            id: sea_orm::ActiveValue::Set(game_id),
            status: sea_orm::ActiveValue::Set(GameStatus::Pending),
            bet: sea_orm::ActiveValue::Set(bet),
            created_at: sea_orm::ActiveValue::Set(now),
            updated_at: sea_orm::ActiveValue::Set(now),
            finished_at: sea_orm::ActiveValue::NotSet,
            rank: sea_orm::ActiveValue::NotSet,
            roll: sea_orm::ActiveValue::Set(1),
            auto: sea_orm::ActiveValue::Set(false),
            winner_id: sea_orm::ActiveValue::NotSet,
            player_positions: sea_orm::ActiveValue::Set(json!({})),
            current_winning_card: sea_orm::ActiveValue::NotSet,
            current_winning_player_position: sea_orm::ActiveValue::NotSet,
            creator_id: sea_orm::ActiveValue::Set(Some(creator_user_id)),
            game_mode: sea_orm::ActiveValue::Set(GameMode::Multiplayer),
            max_players: sea_orm::ActiveValue::Set(max_players),
            invite_expires_at: sea_orm::ActiveValue::Set(Some(expires_at)),
        };
        let insert_result = game::Entity::insert(game_active)
            .exec(&txn)
            .await
            .map_err(|e| {
                GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
            })?;
        let inserted_game_id = insert_result.last_insert_id;

        let player_id = Uuid::new_v4();
        let player_active = player::ActiveModel {
            id: sea_orm::ActiveValue::Set(player_id),
            game_id: sea_orm::ActiveValue::Set(inserted_game_id),
            player_type: sea_orm::ActiveValue::Set(crate::database::models::PlayerType::Human),
            name: sea_orm::ActiveValue::Set(creator_pseudo.to_string()),
            position: sea_orm::ActiveValue::Set(0),
            credits: sea_orm::ActiveValue::Set(new_credit),
            created_at: sea_orm::ActiveValue::Set(now),
            user_id: sea_orm::ActiveValue::Set(Some(creator_user_id)),
        };
        player::Entity::insert(player_active)
            .exec(&txn)
            .await
            .map_err(|e| {
                GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
            })?;

        let player_positions: std::collections::HashMap<i32, Uuid> =
            std::collections::HashMap::from([(0, creator_user_id)]);
        let game_model = game::Entity::find_by_id(inserted_game_id)
            .one(&txn)
            .await
            .map_err(|e| {
                GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
            })?
            .ok_or_else(|| GameServiceError::Internal("Game not found after insert".to_string()))?;
        let mut game_active: game::ActiveModel = game_model.into();
        game_active.player_positions =
            sea_orm::ActiveValue::Set(serde_json::to_value(player_positions).map_err(|e| {
                GameServiceError::Internal(format!("Failed to serialize player_positions: {}", e))
            })?);
        game_active.update(&txn).await.map_err(|e| {
            GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
        })?;

        txn.commit().await.map_err(|e| {
            GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
        })?;

        info!(
            "Multiplayer game created: game_id={}, creator_id={}, bet={}, expires_at={}",
            inserted_game_id, creator_user_id, bet, expires_at
        );

        Ok(MultiplayerGameOutcome {
            game_id: inserted_game_id,
            player_id,
            status: GameStatus::Pending,
            bet,
            max_players,
            invite_expires_at: expires_at,
        })
    }

    pub async fn send_invites(
        &self,
        game_id: Uuid,
        creator_user_id: Uuid,
        invited_user_ids: &[Uuid],
    ) -> Result<(), GameServiceError> {
        let game = game::Entity::find_by_id(game_id)
            .one(&self.db)
            .await
            .map_err(|e| {
                GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
            })?
            .ok_or(GameServiceError::GameNotFound)?;

        if game.status != GameStatus::Pending {
            return Err(GameServiceError::GameNotPending);
        }
        if game.creator_id != Some(creator_user_id) {
            return Err(GameServiceError::NotCreator);
        }

        let invite_repo = crate::database::repositories::GameInviteRepository::new(self.db.clone());
        for &user_id in invited_user_ids {
            if user_id == creator_user_id {
                continue;
            }
            if crate::database::repositories::PlayerRepository::new(self.db.clone())
                .find_by_game_and_user(game_id, user_id)
                .await
                .map_err(|e| {
                    GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
                })?
                .is_some()
            {
                continue;
            }
            let existing = invite_repo
                .find_invite(game_id, user_id)
                .await
                .map_err(|e| {
                    GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
                })?;
            if existing.is_none() {
                invite_repo
                    .create_invite(game_id, user_id)
                    .await
                    .map_err(|e| {
                        GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
                    })?;
                info!(
                    "Invite sent: game_id={}, invited_user_id={}",
                    game_id, user_id
                );
            }
        }
        Ok(())
    }

    pub async fn accept_invite(
        &self,
        game_id: Uuid,
        user_id: Uuid,
        user_pseudo: &str,
    ) -> Result<crate::database::models::player::Model, GameServiceError> {
        let txn = self.db.begin().await.map_err(|e| {
            GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
        })?;

        let game_model = game::Entity::find_by_id(game_id)
            .one(&txn)
            .await
            .map_err(|e| {
                GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
            })?
            .ok_or(GameServiceError::GameNotFound)?;

        if game_model.status != GameStatus::Pending {
            txn.rollback().await.ok();
            return Err(GameServiceError::GameNotPending);
        }
        if Some(user_id) == game_model.creator_id {
            txn.rollback().await.ok();
            return Err(GameServiceError::CreatorCannotJoin);
        }

        let existing_player = player::Entity::find()
            .filter(player::Column::GameId.eq(game_id))
            .filter(player::Column::UserId.eq(user_id))
            .one(&txn)
            .await
            .map_err(|e| {
                GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
            })?;
        if existing_player.is_some() {
            txn.rollback().await.ok();
            return Err(GameServiceError::AlreadyJoined);
        }

        let invite = game_invite::Entity::find()
            .filter(game_invite::Column::GameId.eq(game_id))
            .filter(game_invite::Column::InvitedUserId.eq(user_id))
            .filter(game_invite::Column::Status.eq(InviteStatus::Pending))
            .one(&txn)
            .await
            .map_err(|e| {
                GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
            })?
            .ok_or(GameServiceError::NotInvited)?;

        let player_count: u64 = player::Entity::find()
            .filter(player::Column::GameId.eq(game_id))
            .count(&txn)
            .await
            .map_err(|e| {
                GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
            })?;

        if player_count >= game_model.max_players as u64 {
            txn.rollback().await.ok();
            return Err(GameServiceError::GameFull);
        }

        let next_position = player_count as i32;
        let max_players_val = game_model.max_players;
        let bet = game_model.bet;

        let profile = player_profile::Entity::find()
            .filter(player_profile::Column::UserId.eq(user_id))
            .one(&txn)
            .await
            .map_err(|e| {
                GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
            })?
            .ok_or_else(|| GameServiceError::Internal("Player profile not found".to_string()))?;

        if profile.credit < bet {
            txn.rollback().await.ok();
            return Err(GameServiceError::InsufficientCredits);
        }

        let now = chrono::Utc::now();
        let new_credit = profile.credit - bet;
        let mut profile_active: player_profile::ActiveModel = profile.into();
        profile_active.credit = sea_orm::ActiveValue::Set(new_credit);
        profile_active.updated_at = sea_orm::ActiveValue::Set(now);
        profile_active.update(&txn).await.map_err(|e| {
            GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
        })?;

        let new_player_id = Uuid::new_v4();
        let player_active = player::ActiveModel {
            id: sea_orm::ActiveValue::Set(new_player_id),
            game_id: sea_orm::ActiveValue::Set(game_id),
            player_type: sea_orm::ActiveValue::Set(PlayerType::Human),
            name: sea_orm::ActiveValue::Set(user_pseudo.to_string()),
            position: sea_orm::ActiveValue::Set(next_position),
            credits: sea_orm::ActiveValue::Set(new_credit),
            created_at: sea_orm::ActiveValue::Set(now),
            user_id: sea_orm::ActiveValue::Set(Some(user_id)),
        };
        player::Entity::insert(player_active)
            .exec(&txn)
            .await
            .map_err(|e| {
                GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
            })?;

        let mut invite_active: game_invite::ActiveModel = invite.into();
        invite_active.status = sea_orm::ActiveValue::Set(InviteStatus::Accepted);
        invite_active.update(&txn).await.map_err(|e| {
            GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
        })?;

        let current_positions: std::collections::HashMap<i32, Uuid> =
            if game_model.player_positions.is_null() {
                std::collections::HashMap::new()
            } else {
                serde_json::from_value(game_model.player_positions.clone()).map_err(|e| {
                    GameServiceError::Internal(format!("Failed to parse player_positions: {}", e))
                })?
            };
        let mut updated_positions = current_positions;
        updated_positions.insert(next_position, user_id);

        let new_status = if (player_count + 1) >= max_players_val as u64 {
            GameStatus::Ready
        } else {
            GameStatus::Pending
        };

        let mut game_active: game::ActiveModel = game_model.into();
        game_active.player_positions =
            sea_orm::ActiveValue::Set(serde_json::to_value(&updated_positions).map_err(|e| {
                GameServiceError::Internal(format!("Failed to serialize player_positions: {}", e))
            })?);
        game_active.status = sea_orm::ActiveValue::Set(new_status);
        game_active.updated_at = sea_orm::ActiveValue::Set(now);
        game_active.update(&txn).await.map_err(|e| {
            GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
        })?;

        txn.commit().await.map_err(|e| {
            GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
        })?;

        info!(
            "Player joined game: game_id={}, user_id={}, position={}, status={}",
            game_id,
            user_id,
            next_position,
            match new_status {
                GameStatus::Ready => "ready",
                _ => "pending",
            }
        );

        if let Some(ref redis) = self.redis_client {
            let event = GameEvent::PlayerJoined {
                game_id,
                player_id: new_player_id,
                user_id,
                pseudo: user_pseudo.to_string(),
                position: next_position,
                player_count: (player_count + 1) as i32,
                max_players: max_players_val as i32,
            };
            if let Err(e) = redis.clone().publish_game_event(&event).await {
                error!("Failed to publish PlayerJoined event: {}", e);
            }
            if new_status == GameStatus::Ready {
                let event = GameEvent::GameReady {
                    game_id,
                    correlation_id: None,
                };
                if let Err(e) = redis.clone().publish_game_event(&event).await {
                    error!("Failed to publish GameReady event: {}", e);
                }
            }
        }

        player::Entity::find_by_id(new_player_id)
            .one(&self.db)
            .await
            .map_err(|e| {
                GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
            })?
            .ok_or(GameServiceError::PlayerNotFound)
    }

    pub async fn cancel_game(&self, game_id: Uuid) -> Result<(), GameServiceError> {
        let txn = self.db.begin().await.map_err(|e| {
            GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
        })?;

        let game_model = game::Entity::find_by_id(game_id)
            .one(&txn)
            .await
            .map_err(|e| {
                GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
            })?
            .ok_or(GameServiceError::GameNotFound)?;

        if game_model.status != GameStatus::Pending {
            txn.rollback().await.ok();
            return Ok(());
        }

        let players = player::Entity::find()
            .filter(player::Column::GameId.eq(game_id))
            .all(&txn)
            .await
            .map_err(|e| {
                GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
            })?;

        for p in &players {
            if let Some(uid) = p.user_id {
                let profile = player_profile::Entity::find()
                    .filter(player_profile::Column::UserId.eq(uid))
                    .one(&txn)
                    .await
                    .map_err(|e| {
                        GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
                    })?;
                if let Some(profile_model) = profile {
                    let mut profile_active: player_profile::ActiveModel = profile_model.into();
                    profile_active.credit =
                        sea_orm::ActiveValue::Set(profile_active.credit.unwrap() + game_model.bet);
                    profile_active.updated_at = sea_orm::ActiveValue::Set(chrono::Utc::now());
                    profile_active.update(&txn).await.map_err(|e| {
                        GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
                    })?;
                }
            }
        }

        let mut game_active: game::ActiveModel = game_model.into();
        game_active.status = sea_orm::ActiveValue::Set(GameStatus::Cancelled);
        game_active.updated_at = sea_orm::ActiveValue::Set(chrono::Utc::now());
        game_active.finished_at = sea_orm::ActiveValue::Set(Some(chrono::Utc::now()));
        game_active.update(&txn).await.map_err(|e| {
            GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
        })?;

        txn.commit().await.map_err(|e| {
            GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
        })?;

        info!("Game cancelled: game_id={}", game_id);

        if let Some(ref redis) = self.redis_client {
            let event = GameEvent::GameCancelled {
                game_id,
                reason: "Not enough players joined before timeout".to_string(),
            };
            if let Err(e) = redis.clone().publish_game_event(&event).await {
                error!("Failed to publish GameCancelled event: {}", e);
            }
        }

        Ok(())
    }

    pub async fn cancel_expired_games(&self) -> Result<u64, GameServiceError> {
        let now = chrono::Utc::now();

        let expired_games = game::Entity::find()
            .filter(game::Column::Status.eq(GameStatus::Pending))
            .filter(game::Column::GameMode.eq(GameMode::Multiplayer))
            .filter(game::Column::InviteExpiresAt.lte(now))
            .all(&self.db)
            .await
            .map_err(|e| {
                GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
            })?;

        let mut cancelled = 0u64;
        for g in expired_games {
            if let Err(e) = self.cancel_game(g.id).await {
                error!("Failed to cancel expired game {}: {}", g.id, e);
            } else {
                cancelled += 1;
            }
        }

        if cancelled > 0 {
            info!("Cancelled {} expired games", cancelled);
        }
        Ok(cancelled)
    }

    pub async fn start_game(&self, game_id: Uuid, user_id: Uuid) -> Result<(), GameServiceError> {
        use rand::{seq::SliceRandom, thread_rng};

        let cards: Vec<i32> = {
            let mut cards: Vec<i32> = (0..TOTAL_CARDS as i32).collect();
            let mut rng = thread_rng();
            cards.shuffle(&mut rng);
            cards
        };

        let txn = self.db.begin().await.map_err(|e| {
            GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
        })?;

        let game_model = game::Entity::find_by_id(game_id)
            .one(&txn)
            .await
            .map_err(|e| {
                GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
            })?
            .ok_or(GameServiceError::GameNotFound)?;

        if game_model.status != GameStatus::Ready {
            txn.rollback().await.ok();
            return Err(GameServiceError::GameNotReady);
        }
        if game_model.creator_id != Some(user_id) {
            txn.rollback().await.ok();
            return Err(GameServiceError::NotCreator);
        }

        let players = player::Entity::find()
            .filter(player::Column::GameId.eq(game_id))
            .order_by_asc(player::Column::Position)
            .all(&txn)
            .await
            .map_err(|e| {
                GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
            })?;

        let num_players = players.len();
        if num_players < 2 {
            txn.rollback().await.ok();
            return Err(GameServiceError::Internal(
                "Not enough players to start".to_string(),
            ));
        }

        let player_ids: Vec<Uuid> = players.iter().map(|p| p.id).collect();

        let now = chrono::Utc::now();
        for (i, &pid) in player_ids.iter().enumerate() {
            let start = i * CARDS_PER_PLAYER;
            let end = start + CARDS_PER_PLAYER;
            for &card_index in &cards[start..end] {
                game_card::Entity::insert(game_card::ActiveModel {
                    id: sea_orm::ActiveValue::Set(Uuid::new_v4()),
                    game_id: sea_orm::ActiveValue::Set(game_id),
                    player_id: sea_orm::ActiveValue::Set(Some(pid)),
                    card_index: sea_orm::ActiveValue::Set(card_index),
                    played: sea_orm::ActiveValue::Set(false),
                    played_at: sea_orm::ActiveValue::NotSet,
                    round: sea_orm::ActiveValue::NotSet,
                    created_at: sea_orm::ActiveValue::Set(now),
                })
                .exec(&txn)
                .await
                .map_err(|e| {
                    GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
                })?;
            }
        }

        let initial_rank = 0i32;
        let first_player_id = player_ids[0];

        let mut game_active: game::ActiveModel = game_model.into();
        game_active.status = sea_orm::ActiveValue::Set(GameStatus::Active);
        game_active.rank = sea_orm::ActiveValue::Set(Some(initial_rank));
        game_active.roll = sea_orm::ActiveValue::Set(1);
        game_active.updated_at = sea_orm::ActiveValue::Set(now);
        game_active.update(&txn).await.map_err(|e| {
            GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
        })?;

        txn.commit().await.map_err(|e| {
            GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
        })?;

        info!(
            "Game started: game_id={}, players={}, first_turn={}",
            game_id, num_players, first_player_id
        );

        if let Some(ref redis) = self.redis_client {
            for &pid in &player_ids {
                let player_cards: Vec<i32> = {
                    let offset =
                        players.iter().position(|p| p.id == pid).unwrap_or(0) * CARDS_PER_PLAYER;
                    cards[offset..offset + CARDS_PER_PLAYER].to_vec()
                };
                let event = GameEvent::CardsDealt {
                    game_id,
                    player_id: pid,
                    cards: player_cards,
                };
                if let Err(e) = redis.clone().publish_game_event(&event).await {
                    error!("Failed to publish CardsDealt event: {}", e);
                }
            }

            let game_started_players: Vec<crate::messaging::events::GameStartedPlayer> = players
                .iter()
                .map(|p| crate::messaging::events::GameStartedPlayer {
                    id: p.id,
                    name: p.name.clone(),
                    position: p.position,
                    display_position: p.position,
                    cards_count: CARDS_PER_PLAYER as i32,
                })
                .collect();

            let event = GameEvent::GameStarted {
                game_id,
                players: game_started_players,
                current_turn: first_player_id,
                correlation_id: None,
            };
            if let Err(e) = redis.clone().publish_game_event(&event).await {
                error!("Failed to publish GameStarted event: {}", e);
            }
        }

        Ok(())
    }

    /// Validate if a card can be played by a player in the current game state.
    /// Returns true if the play is valid, false otherwise.
    /// `current_winning_card` is the index of the currently winning card in the round (if any).
    pub async fn validate_card_play(
        &self,
        _game_id: Uuid,
        player_id: Uuid,
        card_index: i32,
        current_winning_card: Option<i32>,
    ) -> Result<bool, GameServiceError> {
        if let Some(winning_idx) = current_winning_card {
            // The card played is from the same colour (suit)
            if (winning_idx / 8) == (card_index / 8) {
                return Ok(true);
            } else {
                // The card played is not from the same colour, find if the player had one he didn't play
                let repo = GameCardRepository::new(self.db.clone());
                let player_cards = repo.list_by_player(player_id).await.map_err(|e| {
                    GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
                })?;
                let unplayed_cards: Vec<i32> = player_cards
                    .iter()
                    .filter(|gc| !gc.played)
                    .map(|gc| gc.card_index)
                    .collect();
                for challenger_card in unplayed_cards {
                    // You have a card of the same winning card colour that you are not playing, play it instead
                    if (winning_idx / 8) == (challenger_card / 8) {
                        return Ok(false);
                    }
                }
            }
        }
        Ok(true)
    }

    /// Update the game state after a card is played.
    ///
    /// # Transactional guarantees
    ///
    /// All state changes (card played, rank updated, round evaluation, payment processing)
    /// happen inside a single database transaction. This ensures atomicity:
    /// - If any step fails, all changes are rolled back
    /// - No partial state is visible to other connections
    /// - Payment updates are atomic with game state changes
    ///
    /// Post-transaction operations (event publishing) are best-effort and non-critical.
    pub async fn update_card_play(
        &self,
        game_id: Uuid,
        player_id: Uuid,
        card_index: i32,
        correlation_id: Option<Uuid>,
    ) -> Result<CardPlayResult, GameServiceError> {
        let span = tracing::info_span!(
            "card_play",
            correlation_id = %correlation_id.map(|id| id.to_string()).unwrap_or_default(),
            game_id = %game_id,
            player_id = %player_id,
            card_index = card_index,
        );
        let _guard = span.enter();

        // Start transaction first — all reads and writes happen inside it
        let txn = self.db.begin().await.map_err(|e| {
            GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
        })?;

        // --- All data fetching happens INSIDE the transaction ---

        // 1. Fetch game and verify it's not finished (via txn)
        let game = game::Entity::find_by_id(game_id)
            .one(&txn)
            .await
            .map_err(|e| {
                GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
            })?
            .ok_or(GameServiceError::GameNotFound)?;
        if game.status == GameStatus::Finished
            || game.status == GameStatus::Kora
            || game.status == GameStatus::DoubleKora
        {
            txn.rollback().await.ok();
            return Err(GameServiceError::GameFinished);
        }

        // 2. Verify it's the player's turn (via txn)
        let players = player::Entity::find()
            .filter(player::Column::GameId.eq(game_id))
            .order_by_asc(player::Column::Position)
            .all(&txn)
            .await
            .map_err(|e| {
                GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
            })?;
        let current_player = players
            .iter()
            .find(|p| p.id == player_id)
            .ok_or(GameServiceError::PlayerNotFound)?;
        let current_rank = game.rank.unwrap_or(0) as usize;
        if current_player.position as usize != current_rank {
            txn.rollback().await.ok();
            return Err(GameServiceError::NotYourTurn);
        }

        // 3. Fetch the card and ensure it's unplayed (via txn)
        let game_cards = game_card::Entity::find()
            .filter(game_card::Column::PlayerId.eq(player_id))
            .all(&txn)
            .await
            .map_err(|e| {
                GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
            })?;
        let target_card = game_cards
            .iter()
            .find(|gc| gc.card_index == card_index && !gc.played)
            .ok_or(GameServiceError::CardNotFound)?;

        // 4. Use stored current winning card from game model
        let current_winning_card = game.current_winning_card;
        let _current_winning_player_position = game.current_winning_player_position;

        // 5. Validate the card (uses self.db for card lookup — acceptable since it's read-only validation)
        let valid = self
            .validate_card_play(game_id, player_id, card_index, current_winning_card)
            .await?;
        if !valid {
            txn.rollback().await.ok();
            return Err(GameServiceError::InvalidCard);
        }

        // 6. Mark card as played and set round = game.roll (using txn connection)
        let mut card_active: game_card::ActiveModel = target_card.clone().into();
        card_active.played = ActiveValue::Set(true);
        card_active.played_at = ActiveValue::Set(Some(Utc::now()));
        card_active.round = ActiveValue::Set(Some(game.roll));
        card_active.update(&txn).await.map_err(|e| {
            GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
        })?;

        // 6a. Compute updated current_winning_card and current_winning_player_position.
        //     The first card played in a round defines the leading suit (stored as current_winning_card).
        //     If a higher card of the same suit is played later, the winning position updates.
        let new_winning_card = if current_winning_card.is_none() {
            Some(card_index)
        } else {
            current_winning_card
        };
        let new_winning_position = match current_winning_card {
            None => Some(current_player.position),
            Some(winning) => {
                if winning / 8 == card_index / 8 && card_index % 8 > winning % 8 {
                    Some(current_player.position)
                } else {
                    game.current_winning_player_position
                }
            }
        };

        // 7. Check if round is complete BEFORE updating rank.
        //    We check using the current game.roll (before any rank/roll changes).
        //    This must happen before the rank update to avoid the rank update's
        //    ActiveModel (which implicitly writes ALL fields including roll=current_roll)
        //    from overwriting the round evaluation's roll increment.
        let round_complete = self.is_round_complete_txn(&txn, game_id, game.roll).await?;
        let mut round_result: Option<RoundEvaluationResult> = None;

        if round_complete {
            // Round is complete: evaluate it FIRST, which sets winner, rank=winner_pos, roll+=1
            // Then skip the rank update below since evaluate_round_in_txn handles it.
            round_result = Some(self.evaluate_round_in_txn(&txn, game_id, game.roll).await?);
        } else {
            // Round not complete: just advance rank to next player
            let next_rank = next_player(current_rank, players.len());
            info!(
                "Card played by player {} (index {}), updating rank from {} to {}",
                player_id, card_index, current_rank, next_rank
            );
            let mut game_active: game::ActiveModel = game::Entity::find_by_id(game_id)
                .one(&txn)
                .await
                .map_err(|e| {
                    GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
                })?
                .ok_or(GameServiceError::GameNotFound)?
                .into();
            game_active.rank = ActiveValue::Set(Some(next_rank as i32));
            game_active.current_winning_card = ActiveValue::Set(new_winning_card);
            game_active.current_winning_player_position = ActiveValue::Set(new_winning_position);
            game_active.updated_at = ActiveValue::Set(Utc::now());
            game_active.update(&txn).await.map_err(|e| {
                GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
            })?;
        }

        // 8. If game ended, process payment INSIDE the transaction (atomic with game state)
        if let Some(ref result) = round_result {
            if result.game_ended {
                self.process_payment_in_txn(&txn, game_id, result).await?;
            }
        }

        // 9. Commit transaction — all state changes (game, cards, credits) are atomic
        txn.commit().await.map_err(|e| {
            GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
        })?;

        // === PHASE 2: Post-transaction operations (best-effort, non-critical) ===

        // 10. Determine next player and game-ended status (before consuming data for events)
        let round_completed = round_result.is_some();
        let game_ended = round_result.as_ref().map(|r| r.game_ended).unwrap_or(false);
        let current_round = game.roll;
        let next_player_id = if let Some(ref result) = round_result {
            players
                .get(result.winner_position)
                .map(|p| p.id)
                .ok_or_else(|| {
                    GameServiceError::Internal("Winner not in player list".to_string())
                })?
        } else {
            let rank_after = next_player(current_rank, players.len());
            players.get(rank_after).map(|p| p.id).ok_or_else(|| {
                GameServiceError::Internal("No player at computed rank".to_string())
            })?
        };

        // 11. Publish CardPlayed event AFTER commit succeeds, BEFORE round events.
        self.publish_card_played(
            game_id,
            player_id,
            card_index,
            Some(next_player_id),
            correlation_id,
        )
        .await;

        // 11b. Publish TurnChanged event so frontend knows whose turn it is
        if !game_ended {
            self.publish_turn_changed(game_id, next_player_id, correlation_id)
                .await;
        }

        // 12. If round was evaluated, publish events
        if let Some(result) = round_result {
            self.publish_round_completed(game_id, &result, &players, correlation_id)
                .await;

            if result.game_ended {
                crate::observability::metrics::GAMES_FINISHED_TOTAL
                    .with_label_values(&[&result.final_status.to_string()])
                    .inc();
                self.publish_game_finished(game_id, &result, correlation_id)
                    .await;
            }
        }

        // Return the updated card + full context (no extra DB queries needed by callers)
        let card_repo = GameCardRepository::new(self.db.clone());
        let card = card_repo
            .list_by_player(player_id)
            .await
            .map_err(|e| {
                GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
            })?
            .into_iter()
            .find(|gc| gc.id == target_card.id)
            .ok_or(GameServiceError::Internal("Card disappeared".to_string()))?;

        Ok(CardPlayResult {
            card,
            next_player_id,
            players,
            game_ended,
            round_completed,
            current_round,
        })
    }

    /// Check if all players have played a card in the given round.
    /// Uses the provided transaction connection to see uncommitted data.
    async fn is_round_complete_txn(
        &self,
        txn: &DatabaseTransaction,
        game_id: Uuid,
        round: i32,
    ) -> Result<bool, GameServiceError> {
        use crate::database::models::game_card;
        use crate::database::models::player;
        use sea_orm::ColumnTrait;
        use sea_orm::QueryFilter;

        let players = player::Entity::find()
            .filter(player::Column::GameId.eq(game_id))
            .all(txn)
            .await
            .map_err(|e| {
                GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
            })?;

        for player_model in players {
            let cards = game_card::Entity::find()
                .filter(game_card::Column::PlayerId.eq(player_model.id))
                .all(txn)
                .await
                .map_err(|e| {
                    GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
                })?;
            let played_in_round = cards.iter().any(|c| c.played && c.round == Some(round));
            if !played_in_round {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Evaluate a completed round inside an active transaction.
    /// All DB writes use the transaction connection for atomicity.
    /// Returns RoundEvaluationResult for post-transaction event publishing.
    async fn evaluate_round_in_txn(
        &self,
        txn: &DatabaseTransaction,
        game_id: Uuid,
        round: i32,
    ) -> Result<RoundEvaluationResult, GameServiceError> {
        let span = tracing::info_span!(
            "round_eval",
            game_id = %game_id,
            round = round,
        );
        let _guard = span.enter();
        use crate::database::models::game_card;
        use crate::database::models::player;
        use sea_orm::ColumnTrait;
        use sea_orm::QueryFilter;

        // Fetch played cards for this round (via txn)
        // Order by PlayedAt to correctly identify the chronologically first card
        // as the leading card (plays.first() at line ~359).
        let played_cards = game_card::Entity::find()
            .filter(game_card::Column::GameId.eq(game_id))
            .filter(game_card::Column::Round.eq(round))
            .filter(game_card::Column::Played.eq(true))
            .order_by_asc(game_card::Column::PlayedAt)
            .all(txn)
            .await
            .map_err(|e| {
                GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
            })?;
        info!(
            "evaluate_round_in_txn: found {} played cards for round {}",
            played_cards.len(),
            round
        );
        if played_cards.is_empty() {
            return Err(GameServiceError::RoundNotComplete);
        }

        // Fetch players (via txn)
        let players = player::Entity::find()
            .filter(player::Column::GameId.eq(game_id))
            .order_by_asc(player::Column::Position)
            .all(txn)
            .await
            .map_err(|e| {
                GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
            })?;
        let player_positions: Vec<Uuid> = players.iter().map(|p| p.id).collect();

        // Convert to PlayedCard structures
        let mut plays = Vec::new();
        for card in &played_cards {
            if let Some(player_id) = card.player_id {
                let position = player_positions
                    .iter()
                    .position(|&id| id == player_id)
                    .ok_or_else(|| {
                        GameServiceError::Internal("Player not found in game".to_string())
                    })?;
                let Ok(index) = u8::try_from(card.card_index) else {
                    continue;
                };
                if let Some(card_obj) = Card::new(index) {
                    plays.push(PlayedCard {
                        player_position: position,
                        card: card_obj,
                    });
                }
            }
        }

        // The leading card for this round is the first card played.
        // The first player (previous round's winner, or randomly chosen for round 1)
        // determines the leading suit by playing any card they choose.
        // Subsequent players must follow that suit if they have a matching card.
        let first_play = plays
            .first()
            .ok_or_else(|| GameServiceError::Internal("No plays in round".to_string()))?;
        let leading_card = Some(first_play.card);
        let leading_player_position = Some(first_play.player_position);

        // Log the plays and the leading card
        for (index, value) in plays.iter().map(|p| (p.player_position, p.card)) {
            info!(
                " played in round {}: Index: {}, Value: {}",
                round, index, value.index
            );
        }
        if let Some(card) = leading_card {
            info!(
                " leading card for round {}: index {} (suit {})",
                round,
                card.index,
                card.index / 8
            );
        } else {
            info!(" no leading card for round {} (first round)", round);
        }

        // Build RoundContext and evaluate
        let ctx = RoundContext {
            plays,
            leading_card,
            leading_player_position,
        };
        let round_result = evaluate_round(&ctx)
            .ok_or_else(|| GameServiceError::Internal("Round evaluation failed".to_string()))?;
        let winner_pos = round_result.winner_position;
        let winner_id = player_positions[winner_pos];

        // Update game: winner_id, rank = winner position, roll += 1 (via txn)
        let new_roll = round + 1;
        info!(
            "Round {} evaluated, winner is player {}, updating round to {}",
            round, winner_id, new_roll
        );

        // Fetch current game model to get the current status
        let game_model = game::Entity::find_by_id(game_id)
            .one(txn)
            .await
            .map_err(|e| {
                GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
            })?
            .ok_or(GameServiceError::GameNotFound)?;

        // Determine if game ends (all rounds played)
        let game_ends = new_roll > CARDS_PER_PLAYER as i32;
        let mut final_status = game_model.status;

        if game_ends {
            // Use the is_kora flag from RoundResult (determined by the pure evaluation function)
            if round_result.is_kora {
                final_status = GameStatus::Kora;
                // TODO: check for double Kora? Not implemented.
            } else {
                final_status = GameStatus::Finished;
            }
        }

        // Save game updates using the game_model fetched above (avoids redundant fetch).
        // No writes to the game table happen between the fetch and this update.
        let mut game_active: game::ActiveModel = game_model.into();
        game_active.winner_id = ActiveValue::Set(Some(winner_id));
        game_active.rank = ActiveValue::Set(Some(winner_pos as i32));
        game_active.roll = ActiveValue::Set(new_roll);
        game_active.current_winning_card = ActiveValue::Set(None);
        game_active.current_winning_player_position = ActiveValue::Set(None);
        game_active.updated_at = ActiveValue::Set(Utc::now());
        if game_ends {
            game_active.status = ActiveValue::Set(final_status);
        }
        game_active.update(txn).await.map_err(|e| {
            GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
        })?;

        Ok(RoundEvaluationResult {
            round,
            winner_id,
            winner_position: winner_pos,
            game_ended: game_ends,
            final_status,
            players,
        })
    }

    /// Publish a CardPlayed event to Redis (best-effort).
    async fn publish_card_played(
        &self,
        game_id: Uuid,
        player_id: Uuid,
        card_index: i32,
        next_turn: Option<Uuid>,
        correlation_id: Option<Uuid>,
    ) {
        if let Some(mut redis_client) = self.redis_client.clone() {
            let event = GameEvent::CardPlayed {
                game_id,
                player_id,
                card_index,
                next_turn,
                correlation_id,
            };
            if let Err(e) = redis_client.publish_game_event(&event).await {
                error!("Failed to publish CardPlayed event: {}", e);
            }
        }
    }

    /// Publish a TurnChanged event to Redis so the frontend knows whose turn it is.
    async fn publish_turn_changed(
        &self,
        game_id: Uuid,
        current_turn: Uuid,
        correlation_id: Option<Uuid>,
    ) {
        if let Some(mut redis_client) = self.redis_client.clone() {
            let event = GameEvent::TurnChanged {
                game_id,
                current_turn,
                correlation_id,
            };
            if let Err(e) = redis_client.publish_game_event(&event).await {
                error!("Failed to publish TurnChanged event: {}", e);
            }
        }
    }

    /// Publish a RoundCompleted event to Redis.
    /// `players` is the full player list ordered by position, used to build deck_slots
    /// where each slot corresponds to the card played by the player at that position.
    async fn publish_round_completed(
        &self,
        game_id: Uuid,
        result: &RoundEvaluationResult,
        players: &[player::Model],
        correlation_id: Option<Uuid>,
    ) {
        if let Some(mut redis_client) = self.redis_client.clone() {
            let win_type = if result.game_ended {
                match result.final_status {
                    GameStatus::Kora => Some("kora".to_string()),
                    GameStatus::DoubleKora => Some("doubleKora".to_string()),
                    _ => Some("normal".to_string()),
                }
            } else {
                Some("normal".to_string())
            };

            // Build deck_slots: for each player position, find the card they played in this round
            // The deck_slots array is indexed by player position (0..num_players)
            let num_players = players.len();
            let mut deck_slots: Vec<Option<i32>> = vec![None; num_players];

            // We need to fetch the played cards for this round to populate deck_slots
            // Use the database to get the cards played in this round
            if let Ok(played_cards) = crate::database::models::game_card::Entity::find()
                .filter(crate::database::models::game_card::Column::GameId.eq(game_id))
                .filter(crate::database::models::game_card::Column::Round.eq(result.round))
                .filter(crate::database::models::game_card::Column::Played.eq(true))
                .all(&self.db)
                .await
            {
                for card in &played_cards {
                    if let Some(pid) = card.player_id {
                        if let Some(pos) = players.iter().position(|p| p.id == pid) {
                            deck_slots[pos] = Some(card.card_index);
                        }
                    }
                }
            }

            let event = GameEvent::RoundCompleted {
                game_id,
                round_number: result.round,
                winner_id: result.winner_id,
                winner_position: result.winner_position as i32,
                win_type,
                deck_slots,
                correlation_id,
            };

            if let Err(e) = redis_client.publish_game_event(&event).await {
                error!("Failed to publish RoundCompleted event: {}", e);
            }
        }
    }

    /// Publish a GameFinished event to Redis.
    async fn publish_game_finished(
        &self,
        game_id: Uuid,
        result: &RoundEvaluationResult,
        correlation_id: Option<Uuid>,
    ) {
        if let Some(mut redis_client) = self.redis_client.clone() {
            let winner_name = result
                .players
                .iter()
                .find(|p| p.id == result.winner_id)
                .map(|p| p.name.clone());

            let event = GameEvent::GameFinished {
                game_id,
                winner_id: Some(result.winner_id),
                winner_name,
                winner_position: Some(result.winner_position as i32),
                status: match result.final_status {
                    GameStatus::Kora => "kora".to_string(),
                    GameStatus::DoubleKora => "doubleKora".to_string(),
                    _ => "finished".to_string(),
                },
                final_score: None,           // TODO: Calculate final score
                rounds_played: result.round, // round is the old round number, which equals rounds played
                correlation_id,
            };

            if let Err(e) = redis_client.publish_game_event(&event).await {
                error!("Failed to publish GameFinished event: {}", e);
            }
        }
    }

    /// Process payment for a finished game inside an active transaction.
    /// Updates player credits and user profiles atomically with game state changes.
    async fn process_payment_in_txn(
        &self,
        txn: &DatabaseTransaction,
        game_id: Uuid,
        result: &RoundEvaluationResult,
    ) -> Result<(), GameServiceError> {
        use crate::database::models::player_profile;

        let players = &result.players;
        let total_players = players.len();
        let winner_id = result.winner_id;
        let winner_position = players
            .iter()
            .position(|p| p.id == winner_id)
            .ok_or_else(|| GameServiceError::Internal("Winner not in player list".to_string()))?;

        // Determine bet multiplier based on status
        let bet_multiplier = match result.final_status {
            GameStatus::Kora => 2,
            GameStatus::DoubleKora => 4,
            _ => 1,
        };

        let is_kora = matches!(
            result.final_status,
            GameStatus::Kora | GameStatus::DoubleKora
        );

        // Fetch the game model from txn to get the bet amount
        let game_model = game::Entity::find_by_id(game_id)
            .one(txn)
            .await
            .map_err(|e| {
                GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
            })?
            .ok_or(GameServiceError::GameNotFound)?;
        let bet = game_model.bet * bet_multiplier;

        // Calculate payment using existing function
        let credits = calculate_payment(winner_position, total_players, bet);

        // Update player credits (via txn — atomic with game state)
        for (idx, player) in players.iter().enumerate() {
            let new_credits = player.credits + credits[idx];
            let mut player_active: player::ActiveModel = player.clone().into();
            player_active.credits = ActiveValue::Set(new_credits);
            player_active.update(txn).await.map_err(|e| {
                GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
            })?;

            // Update user profile if this player is linked to a user
            if let Some(user_id) = player.user_id {
                let profile = player_profile::Entity::find()
                    .filter(player_profile::Column::UserId.eq(user_id))
                    .one(txn)
                    .await
                    .map_err(|e| {
                        GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
                    })?;

                if let Some(profile_model) = profile {
                    let won = player.id == winner_id;
                    let mut profile_active: player_profile::ActiveModel = profile_model.into();
                    profile_active.credit = ActiveValue::Set(new_credits);
                    profile_active.game_played =
                        ActiveValue::Set(profile_active.game_played.unwrap() + 1);
                    if won {
                        profile_active.wins = ActiveValue::Set(profile_active.wins.unwrap() + 1);
                    }
                    if won && is_kora {
                        profile_active.kora_wins =
                            ActiveValue::Set(profile_active.kora_wins.unwrap() + 1);
                    }
                    profile_active.updated_at = ActiveValue::Set(chrono::Utc::now());
                    profile_active.update(txn).await.map_err(|e| {
                        GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
                    })?;
                }
            }
        }

        Ok(())
    }

    /// Process payment for a finished game (status Finished, Kora, DoubleKora).
    /// Updates player credits.
    /// NOTE: This runs outside a transaction. Prefer process_payment_in_txn for atomicity.
    #[allow(dead_code)]
    pub async fn process_payment(&self, game_id: Uuid) -> Result<(), GameServiceError> {
        let game_repo = GameRepository::new(self.db.clone());
        let player_repo = PlayerRepository::new(self.db.clone());
        let game = game_repo
            .find_by_id(game_id)
            .await
            .map_err(|e| {
                GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
            })?
            .ok_or(GameServiceError::GameNotFound)?;
        if game.status != GameStatus::Finished
            && game.status != GameStatus::Kora
            && game.status != GameStatus::DoubleKora
        {
            return Ok(());
        }
        let players = player_repo.list_by_game(game_id).await.map_err(|e| {
            GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
        })?;
        let total_players = players.len();
        let winner_id = game
            .winner_id
            .ok_or_else(|| GameServiceError::Internal("No winner set".to_string()))?;
        let winner_position = players
            .iter()
            .position(|p| p.id == winner_id)
            .ok_or_else(|| GameServiceError::Internal("Winner not in player list".to_string()))?;

        // Determine bet multiplier based on status
        let bet_multiplier = match game.status {
            GameStatus::Kora => 2,
            GameStatus::DoubleKora => 4,
            _ => 1,
        };
        let bet = game.bet * bet_multiplier;

        // Calculate payment using existing function
        let credits = calculate_payment(winner_position, total_players, bet);

        // Update player credits (simplified, no profile table yet)
        for (idx, player) in players.iter().enumerate() {
            let mut player_active: player::ActiveModel = player.clone().into();
            player_active.credits = ActiveValue::Set(player.credits + credits[idx]);
            player_active.update(&self.db).await.map_err(|e| {
                GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
            })?;
        }

        Ok(())
    }

    /// Get the player whose turn it is now (the current rank).
    pub async fn next_player(&self, game_id: Uuid) -> Result<Uuid, GameServiceError> {
        let game_repo = GameRepository::new(self.db.clone());
        let player_repo = PlayerRepository::new(self.db.clone());
        let game = game_repo
            .find_by_id(game_id)
            .await
            .map_err(|e| {
                GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
            })?
            .ok_or(GameServiceError::GameNotFound)?;
        let players = player_repo.list_by_game(game_id).await.map_err(|e| {
            GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
        })?;
        let current_rank = game.rank.unwrap_or(0) as usize;
        let player_id =
            players
                .get(current_rank)
                .map(|p| p.id)
                .ok_or(GameServiceError::Internal(
                    "Player index out of bounds".to_string(),
                ))?;
        Ok(player_id)
    }

    /// Build a comprehensive AI task with all necessary context for bot decision making.
    /// This method gathers game state, player information, card data, and round state
    /// to provide the AI worker with everything it needs without additional database queries.
    pub async fn build_ai_task(
        &self,
        game_id: Uuid,
        player_id: Uuid,
        correlation_id: Option<Uuid>,
    ) -> Result<AITask, GameServiceError> {
        let span = tracing::info_span!(
            "build_ai_task",
            correlation_id = %correlation_id.map(|id| id.to_string()).unwrap_or_default(),
            game_id = %game_id,
            player_id = %player_id,
        );
        let _guard = span.enter();
        let game_repo = GameRepository::new(self.db.clone());
        let player_repo = PlayerRepository::new(self.db.clone());
        let card_repo = GameCardRepository::new(self.db.clone());

        // Fetch game first to get current round
        let game = game_repo
            .find_by_id(game_id)
            .await
            .map_err(|e| {
                GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
            })?
            .ok_or(GameServiceError::GameNotFound)?;

        // Fetch other data in parallel
        let players_future = player_repo.list_by_game(game_id);
        let bot_cards_future = card_repo.list_by_player(player_id);

        // Determine current round - in Jambo, each game has 5 rounds (rolls)
        // The roll field tracks which round we're in (1-5)
        let current_round = game.roll;

        let round_cards_future = card_repo.list_by_game_and_round(game_id, current_round);

        let players = players_future.await.map_err(|e| {
            GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
        })?;

        let bot_cards = bot_cards_future.await.map_err(|e| {
            GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
        })?;

        let round_cards = round_cards_future.await.map_err(|e| {
            GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
        })?;

        // Convert player positions from game.player_positions JSON mapping
        let player_positions: std::collections::HashMap<Uuid, i32> = {
            // player_positions is Value, not Option<Value>
            if game.player_positions.is_null() {
                // Fallback to player.position field if JSON mapping not available
                players.iter().map(|p| (p.id, p.position)).collect()
            } else {
                serde_json::from_value(game.player_positions.clone()).map_err(|e| {
                    GameServiceError::Internal(format!("Failed to parse player positions: {}", e))
                })?
            }
        };

        // Build PlayerInfo list
        let player_info_list: Vec<PlayerInfo> = players
            .iter()
            .map(|player| {
                let position = player_positions
                    .get(&player.id)
                    .copied()
                    .unwrap_or(player.position);
                let player_type_str = match player.player_type {
                    crate::database::models::PlayerType::Human => "human".to_string(),
                    crate::database::models::PlayerType::Bot => "bot".to_string(),
                };
                PlayerInfo {
                    player_id: player.id,
                    position,
                    player_type: player_type_str,
                    credits: player.credits,
                    name: player.name.clone(),
                }
            })
            .collect();

        // Get bot's unplayed cards
        let bot_hand_cards: Vec<i32> = bot_cards
            .iter()
            .filter(|gc| !gc.played)
            .map(|gc| gc.card_index)
            .collect();

        // Get played cards in current round
        let played_cards_this_round: Vec<i32> = round_cards
            .iter()
            .filter(|gc| gc.played)
            .map(|gc| gc.card_index)
            .collect();

        // Determine current player turn
        let current_player_turn = if game.status == GameStatus::Active {
            let current_rank = game.rank.unwrap_or(0) as usize;
            players.get(current_rank).map(|p| p.id)
        } else {
            None
        };

        // Build the AI task
        let task = AITask::new(
            game_id,
            player_id,
            correlation_id,
            current_round, // Use roll as current_round
            current_round, // Use roll as current_roll (same in Jambo)
            format!("{:?}", game.status),
            current_player_turn,
            played_cards_this_round,
            bot_hand_cards,
            player_info_list,
            game.current_winning_card,
            game.current_winning_player_position,
            game.bet,
            game.auto,
        );

        Ok(task)
    }
}
