use sea_orm::DatabaseConnection;
use std::sync::{Arc, LazyLock};
use std::time::Instant;
use tokio::sync::Semaphore;
use tracing::{error, info};
use uuid::Uuid;

use crate::database::models::{GameStatus, PlayerType};
use crate::database::repositories::{GameCardRepository, GameRepository, PlayerRepository};
use crate::game::service::types::BotMoveOutcome;
use crate::game::service::GameService;
use crate::game::strategy::compute_strategy;
use crate::messaging::{AITask, RabbitMQClient, RedisClient};
use crate::observability::metrics::{
    BOT_CHAIN_BREAKS_TOTAL, BOT_CHAIN_RETRIES_TOTAL, BOT_ERRORS_TOTAL, BOT_MOVE_DURATION_SECONDS,
};
use crate::observability::CorrelationId;

static SYNC_CHAIN_SEMAPHORE: LazyLock<Arc<Semaphore>> =
    LazyLock::new(|| Arc::new(Semaphore::new(10)));

/// Handles scheduling and execution of bot moves, either via RabbitMQ (async)
/// or synchronously as a fallback. Extracted from the API layer so handlers
/// remain thin — they only coordinate, never touch repositories or bot logic.
pub struct BotScheduler {
    db: DatabaseConnection,
    rabbitmq: Option<RabbitMQClient>,
    redis: Option<RedisClient>,
    freeze_duration_secs: u64,
    unfreeze_credit_no_payment: i32,
}

impl BotScheduler {
    pub fn new(
        db: DatabaseConnection,
        rabbitmq: Option<RabbitMQClient>,
        redis: Option<RedisClient>,
        freeze_duration_secs: u64,
        unfreeze_credit_no_payment: i32,
    ) -> Self {
        Self {
            db,
            rabbitmq,
            redis,
            freeze_duration_secs,
            unfreeze_credit_no_payment,
        }
    }

    /// Called after a human plays. If the next player is a bot, dispatch an AI
    /// task via RabbitMQ, or fall back to the synchronous chain when RabbitMQ
    /// is unavailable.
    #[tracing::instrument(level = "info", skip(self), fields(correlation_id = %correlation_id.map(|c| c.to_string()).unwrap_or_default(), game_id = %game_id, next_player = %next_player))]
    pub async fn schedule_if_next_bot(
        &self,
        game_id: Uuid,
        next_player: Uuid,
        correlation_id: Option<CorrelationId>,
    ) {
        if let Some(client) = self.rabbitmq.as_ref() {
            let service = GameService::new_with_redis(self.db.clone(), self.redis.clone())
                .with_freeze_params(self.freeze_duration_secs, self.unfreeze_credit_no_payment);
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
                        let fds = self.freeze_duration_secs;
                        let ucnp = self.unfreeze_credit_no_payment;
                        let cid = correlation_id;
                        tokio::spawn(async move {
                            Self::run_sync_chain(db, redis, game_id, next_player, fds, ucnp, cid)
                                .await;
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
                        let fds = self.freeze_duration_secs;
                        let ucnp = self.unfreeze_credit_no_payment;
                        let cid = correlation_id;
                        tokio::spawn(async move {
                            Self::run_sync_chain(db, redis, game_id, next_player, fds, ucnp, cid)
                                .await;
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
            let fds = self.freeze_duration_secs;
            let ucnp = self.unfreeze_credit_no_payment;
            let cid = correlation_id;
            tokio::spawn(async move {
                Self::run_sync_chain(db, redis, game_id, next_player, fds, ucnp, cid).await;
            });
        }
    }

    pub async fn execute_one_bot_move(
        db: &DatabaseConnection,
        redis: &Option<RedisClient>,
        game_id: Uuid,
        bot_player_id: Uuid,
        freeze_duration_secs: u64,
        unfreeze_credit_no_payment: i32,
        correlation_id: Option<CorrelationId>,
    ) -> Result<BotMoveOutcome, crate::error::GameError> {
        let service = GameService::new_with_redis(db.clone(), redis.clone())
            .with_freeze_params(freeze_duration_secs, unfreeze_credit_no_payment);
        let game_repo = GameRepository::new(db.clone());
        let game_card_repo = GameCardRepository::new(db.clone());

        let game = game_repo
            .find_by_id(game_id)
            .await?
            .ok_or(crate::error::GameError::GameNotFound)?;
        if game.status == GameStatus::Finished
            || game.status == GameStatus::Kora
            || game.status == GameStatus::DoubleKora
        {
            return Err(crate::error::GameError::GameFinished);
        }

        let bot_cards: Vec<i32> = game_card_repo
            .list_by_player(bot_player_id)
            .await?
            .into_iter()
            .filter(|gc| gc.round.is_none())
            .map(|gc| gc.card_index)
            .collect();

        let round_cards: Vec<i32> = game_card_repo
            .list_by_game_and_round(game_id, game.roll)
            .await?
            .into_iter()
            .map(|gc| gc.card_index)
            .collect();

        let chosen = compute_strategy(&bot_cards, &round_cards, game.current_winning_card);
        info!(
            "Executing synchronous bot move for player {} in game {}: chose card {}",
            bot_player_id, game_id, chosen
        );

        let move_start = Instant::now();
        let result = service
            .update_card_play(game_id, bot_player_id, chosen, correlation_id.map(|c| c.0))
            .await?;

        BOT_MOVE_DURATION_SECONDS
            .with_label_values(&["sync_chain"])
            .observe(move_start.elapsed().as_secs_f64());

        let player_repo = PlayerRepository::new(db.clone());
        let players = player_repo.list_by_game(game_id).await?;
        let next_player_id = service.next_player(game_id).await?;

        Ok(BotMoveOutcome {
            card_played: chosen,
            next_player_id,
            round_complete: result.round_completed,
            game_ended: result.game_ended,
            players,
        })
    }

    /// Synchronous bot chain — each bot in turn plays a card, then the next
    /// bot continues the loop. Stops when the next player is human or the
    /// game ends. Limited to 10 concurrent chains via a global semaphore.
    /// This is the fallback path when RabbitMQ is unavailable.
    #[tracing::instrument(level = "info", skip(db, redis), fields(game_id = %game_id))]
    pub async fn run_sync_chain(
        db: DatabaseConnection,
        redis: Option<RedisClient>,
        game_id: Uuid,
        player_id: Uuid,
        freeze_duration_secs: u64,
        unfreeze_credit_no_payment: i32,
        correlation_id: Option<CorrelationId>,
    ) {
        let _permit = match SYNC_CHAIN_SEMAPHORE.clone().acquire_owned().await {
            Ok(p) => p,
            Err(_) => {
                error!("Sync chain semaphore closed, cannot process bot move");
                return;
            }
        };

        let mut current_player = player_id;

        loop {
            let outcome = {
                let mut retries = 0u32;
                loop {
                    match Self::execute_one_bot_move(
                        &db,
                        &redis,
                        game_id,
                        current_player,
                        freeze_duration_secs,
                        unfreeze_credit_no_payment,
                        correlation_id,
                    )
                    .await
                    {
                        Ok(o) => break o,
                        Err(e) => {
                            retries += 1;
                            if retries > 3 {
                                error!(
                                    "Bot {} failed after {} retries in game {}: {}",
                                    current_player, retries, game_id, e
                                );
                                BOT_CHAIN_BREAKS_TOTAL.inc();
                                BOT_ERRORS_TOTAL.with_label_values(&["execution"]).inc();
                                return;
                            }
                            BOT_CHAIN_RETRIES_TOTAL.inc();
                            tokio::time::sleep(std::time::Duration::from_millis(
                                500 * retries as u64,
                            ))
                            .await;
                        }
                    }
                }
            };

            let next_is_bot = outcome
                .players
                .iter()
                .find(|p| p.id == outcome.next_player_id)
                .map(|p| matches!(p.player_type, PlayerType::Bot))
                .unwrap_or(false);

            if next_is_bot {
                current_player = outcome.next_player_id;
            } else {
                info!(
                    "Next player {} is human, stopping bot chain",
                    outcome.next_player_id
                );
                break;
            }
        }
    }
}
