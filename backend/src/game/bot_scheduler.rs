use sea_orm::DatabaseConnection;
use tracing::{error, info};
use uuid::Uuid;

use crate::database::models::{GameStatus, PlayerType};
use crate::database::repositories::{GameCardRepository, GameRepository, PlayerRepository};
use crate::game::constants::BOT_THINKING_DELAY_SECS;
use crate::game::service::GameService;
use crate::game::strategy::compute_strategy;
use crate::messaging::{AITask, RabbitMQClient, RedisClient};
use crate::observability::CorrelationId;

/// Handles scheduling and execution of bot moves, either via RabbitMQ (async)
/// or synchronously as a fallback. Extracted from the API layer so handlers
/// remain thin — they only coordinate, never touch repositories or bot logic.
pub struct BotScheduler {
    db: DatabaseConnection,
    rabbitmq: Option<RabbitMQClient>,
    redis: Option<RedisClient>,
}

impl BotScheduler {
    pub fn new(
        db: DatabaseConnection,
        rabbitmq: Option<RabbitMQClient>,
        redis: Option<RedisClient>,
    ) -> Self {
        Self {
            db,
            rabbitmq,
            redis,
        }
    }

    /// Called after a human plays. If the next player is a bot, dispatch an AI
    /// task via RabbitMQ, or fall back to the synchronous chain when RabbitMQ
    /// is unavailable.
    pub async fn schedule_if_next_bot(
        &self,
        game_id: Uuid,
        next_player: Uuid,
        correlation_id: Option<CorrelationId>,
    ) {
        let cid_str = correlation_id.map(|c| c.to_string()).unwrap_or_default();
        let span = tracing::info_span!(
            "bot_scheduling",
            correlation_id = %cid_str,
            game_id = %game_id,
            next_player = %next_player,
        );
        let _guard = span.enter();

        if let Some(client) = self.rabbitmq.as_ref() {
            let service = GameService::new_with_redis(self.db.clone(), self.redis.clone());
            let cid_uuid = correlation_id.map(|c| c.0);
            match service.build_ai_task(game_id, next_player, cid_uuid).await {
                Ok(task) => {
                    if let Err(e) = client.publish_ai_task(&task).await {
                        error!(
                            correlation_id = %correlation_id.map(|c| c.to_string()).unwrap_or_default(),
                            "Failed to publish AI task to RabbitMQ: {}, falling back to sync chain",
                            e
                        );
                        let db = self.db.clone();
                        let redis = self.redis.clone();
                        tokio::spawn(async move {
                            Self::run_sync_chain(db, redis, game_id, next_player).await;
                        });
                    } else {
                        info!(
                            correlation_id = %correlation_id.map(|c| c.to_string()).unwrap_or_default(),
                            "Published comprehensive AI task for bot {} in game {}",
                            next_player, game_id
                        );
                    }
                }
                Err(e) => {
                    error!(
                        correlation_id = %correlation_id.map(|c| c.to_string()).unwrap_or_default(),
                        "Failed to build AI task for bot {}: {}, falling back to minimal task",
                        next_player, e
                    );
                    let task = AITask::minimal(game_id, next_player);
                    if let Err(e) = client.publish_ai_task(&task).await {
                        error!(
                            correlation_id = %correlation_id.map(|c| c.to_string()).unwrap_or_default(),
                            "Failed to publish minimal AI task: {}, falling back to sync chain",
                            e
                        );
                        let db = self.db.clone();
                        let redis = self.redis.clone();
                        tokio::spawn(async move {
                            Self::run_sync_chain(db, redis, game_id, next_player).await;
                        });
                    } else {
                        info!(
                            correlation_id = %correlation_id.map(|c| c.to_string()).unwrap_or_default(),
                            "Published minimal AI task as fallback for bot {} in game {}",
                            next_player, game_id
                        );
                    }
                }
            }
        } else {
            info!("RabbitMQ not available, running internal bot chain");
            let db = self.db.clone();
            let redis = self.redis.clone();
            tokio::spawn(async move {
                Self::run_sync_chain(db, redis, game_id, next_player).await;
            });
        }
    }

    /// Synchronous bot chain — each bot in turn plays a card, then the next
    /// bot continues the loop.  Stops when the next player is human or the
    /// game ends.  This is the fallback path when RabbitMQ is unavailable.
    pub async fn run_sync_chain(
        db: DatabaseConnection,
        redis: Option<RedisClient>,
        game_id: Uuid,
        player_id: Uuid,
    ) {
        let span = tracing::info_span!(
            "bot_sync_chain",
            game_id = %game_id,
        );
        let _guard = span.enter();

        let mut current_player = player_id;

        loop {
            tokio::time::sleep(std::time::Duration::from_secs(BOT_THINKING_DELAY_SECS)).await;
            info!(
                "Executing synchronous bot move for player {} in game {}",
                current_player, game_id
            );

            let service = GameService::new_with_redis(db.clone(), redis.clone());
            let game_repo = GameRepository::new(db.clone());
            let player_repo = PlayerRepository::new(db.clone());
            let game_card_repo = GameCardRepository::new(db.clone());

            // Fetch game and check if still active
            let game = match game_repo.find_by_id(game_id).await {
                Ok(Some(g)) => g,
                _ => {
                    error!("Failed to fetch game {} for bot move", game_id);
                    break;
                }
            };

            if game.status == GameStatus::Finished
                || game.status == GameStatus::Kora
                || game.status == GameStatus::DoubleKora
            {
                info!(
                    "Game {} has ended (status: {:?}), stopping bot chain",
                    game_id, game.status
                );
                break;
            }

            let round = game.roll;

            // Fetch bot's unplayed cards
            let bot_cards: Vec<i32> = match game_card_repo.list_by_player(current_player).await {
                Ok(cards) => cards
                    .into_iter()
                    .filter(|gc| gc.round.is_none())
                    .map(|gc| gc.card_index)
                    .collect(),
                Err(_) => {
                    error!("Failed to fetch bot cards for player {}", current_player);
                    break;
                }
            };

            if bot_cards.is_empty() {
                info!(
                    "Bot {} has no cards left, stopping bot chain for game {}",
                    current_player, game_id
                );
                break;
            }

            // Fetch played cards this round
            let round_cards: Vec<i32> =
                match game_card_repo.list_by_game_and_round(game_id, round).await {
                    Ok(cards) => cards.into_iter().map(|gc| gc.card_index).collect(),
                    Err(_) => {
                        error!("Failed to fetch round cards for game {}", game_id);
                        break;
                    }
                };

            let chosen = compute_strategy(&bot_cards, &round_cards, game.current_winning_card);
            info!("Bot {} selected card index {}", current_player, chosen);

            // Play the card
            if let Err(e) = service
                .update_card_play(game_id, current_player, chosen, None)
                .await
            {
                error!("Bot {} failed to play card: {}", current_player, e);
                break;
            }
            info!("Bot {} played card {}", current_player, chosen);

            // Determine next player after this bot's move
            let players = match player_repo.list_by_game(game_id).await {
                Ok(list) => list,
                Err(e) => {
                    error!("Failed to fetch players for game {}: {}", game_id, e);
                    break;
                }
            };

            match service.next_player(game_id).await {
                Ok(next_player) => {
                    let next_is_bot = players
                        .iter()
                        .find(|p| p.id == next_player)
                        .map(|p| matches!(p.player_type, PlayerType::Bot))
                        .unwrap_or(false);

                    if next_is_bot {
                        current_player = next_player;
                        continue;
                    } else {
                        info!("Next player {} is human, stopping bot chain", next_player);
                        break;
                    }
                }
                Err(e) => {
                    error!("Failed to determine next player after bot move: {}", e);
                    break;
                }
            }
        }
    }
}
