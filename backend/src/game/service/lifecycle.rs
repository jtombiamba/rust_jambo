use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, QueryFilter, QueryOrder,
    TransactionTrait,
};
use tracing::{error, info};
use uuid::Uuid;

use crate::database::models::{
    game, game_card, player, player_profile, GameMode, GameStatus, PlayerType,
};
use crate::game::constants::{CARDS_PER_PLAYER, TOTAL_CARDS};
use crate::game::service::types::{GameCreationTimer, GameServiceError};
use crate::messaging::events::GameEvent;

use super::GameService;

impl GameService {
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
                        ActiveValue::Set(profile_active.credit.unwrap() + game_model.bet);
                    profile_active.updated_at = ActiveValue::Set(chrono::Utc::now());
                    profile_active.update(&txn).await.map_err(|e| {
                        GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
                    })?;
                }
            }
        }

        let mut game_active: game::ActiveModel = game_model.into();
        game_active.status = ActiveValue::Set(GameStatus::Cancelled);
        game_active.updated_at = ActiveValue::Set(chrono::Utc::now());
        game_active.finished_at = ActiveValue::Set(Some(chrono::Utc::now()));
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

        let user_ids: Vec<Uuid> = players.iter().filter_map(|p| p.user_id).collect();
        if !user_ids.is_empty() {
            self.invalidate_dashboard_caches(&user_ids).await;
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
        let _timer = GameCreationTimer::new("quick");
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
        if num_players > game_model.max_players as usize {
            txn.rollback().await.ok();
            return Err(GameServiceError::Internal(format!(
                "Player count {} exceeds max_players {} for game {}",
                num_players, game_model.max_players, game_id
            )));
        }

        let player_ids: Vec<Uuid> = players.iter().map(|p| p.id).collect();

        // collect all cards per player into a single Vec for bulk insert
        let now = chrono::Utc::now();
        let card_models: Vec<game_card::ActiveModel> = player_ids
            .iter()
            .enumerate()
            .flat_map(|(i, &pid)| {
                let start = i * CARDS_PER_PLAYER;
                let end = start + CARDS_PER_PLAYER;
                cards[start..end]
                    .iter()
                    .map(move |&card_index| game_card::ActiveModel {
                        id: ActiveValue::Set(Uuid::new_v4()),
                        game_id: ActiveValue::Set(game_id),
                        player_id: ActiveValue::Set(Some(pid)),
                        card_index: ActiveValue::Set(card_index),
                        played: ActiveValue::Set(false),
                        played_at: ActiveValue::NotSet,
                        round: ActiveValue::NotSet,
                        created_at: ActiveValue::Set(now),
                    })
            })
            .collect();

        game_card::Entity::insert_many(card_models)
            .exec(&txn)
            .await
            .map_err(|e| {
                GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
            })?;

        let initial_rank = 0i32;
        let first_player_id = player_ids[0];

        let mut game_active: game::ActiveModel = game_model.into();
        game_active.status = ActiveValue::Set(GameStatus::Active);
        game_active.rank = ActiveValue::Set(Some(initial_rank));
        game_active.roll = ActiveValue::Set(1);
        game_active.updated_at = ActiveValue::Set(now);
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
                .map(|p| {
                    let player_type_str = match p.player_type {
                        PlayerType::Human => "human",
                        PlayerType::Bot => "bot",
                    };
                    crate::messaging::events::GameStartedPlayer {
                        id: p.id,
                        name: p.name.clone(),
                        position: p.position,
                        display_position: p.position,
                        cards_count: CARDS_PER_PLAYER as i32,
                        player_type: player_type_str.to_string(),
                    }
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

        self.cache_game_state(game_id).await;

        Ok(())
    }
}
