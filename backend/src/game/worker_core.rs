use anyhow::Result;
use sea_orm::DatabaseConnection;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::database::models::PlayerType;
use crate::game::bot::execute_bot_move_from_task;
use crate::game::bot_scheduler::BotScheduler;
use crate::game::service::GameService;
use crate::messaging::redis::PublishResult;
use crate::messaging::{AITask, RabbitMQClient, RedisClient};
use crate::observability::metrics;

#[allow(dead_code)]
pub async fn process_bot_move(
    task: AITask,
    db_connection: DatabaseConnection,
    redis_client: Option<RedisClient>,
    rabbitmq_client: Option<RabbitMQClient>,
) -> Result<()> {
    let game_id = task.game_id;
    let player_id = task.player_id;
    let correlation_id = task.correlation_id;

    let span = tracing::info_span!(
        "ai_task",
        correlation_id = %correlation_id.map(|id| id.to_string()).unwrap_or_default(),
        game_id = %game_id,
        player_id = %player_id,
    );
    let _guard = span.enter();

    info!(
        "Processing AI task for bot {} in game {} using extended game state",
        player_id, game_id
    );

    debug!(
        "AITask details: round={}, roll={}, status={}, bot_hand_cards={:?}, played_cards={:?}",
        task.current_round,
        task.current_roll,
        task.game_status,
        task.bot_hand_cards.len(),
        task.played_cards_this_round.len()
    );

    let start_time = std::time::Instant::now();

    let bot_result = match execute_bot_move_from_task(&task).await {
        Ok(result) => result,
        Err(e) => {
            error!("Failed to compute bot move from AITask: {}", e);
            info!("Falling back to database-based bot execution");
            let fallback_start = std::time::Instant::now();
            let result = process_bot_move_with_db(
                game_id,
                player_id,
                db_connection,
                redis_client,
                rabbitmq_client,
                correlation_id,
            )
            .await;
            let fallback_duration = fallback_start.elapsed();
            info!(
                "Fallback to database execution took {:?}, success={}",
                fallback_duration,
                result.is_ok()
            );
            return result;
        }
    };

    let chosen_card = bot_result.chosen_card;
    let compute_duration = start_time.elapsed();

    info!(
        task_processing_time_ms = compute_duration.as_millis(),
        task_processing_time_secs = compute_duration.as_secs_f64(),
        game_id = game_id.to_string(),
        player_id = player_id.to_string(),
        round = task.current_round,
        bot_hand_size = task.bot_hand_cards.len(),
        played_cards_count = task.played_cards_this_round.len(),
        chosen_card = chosen_card,
        execution_method = "ai_task",
        "Bot strategy computation completed"
    );

    let service = GameService::new_with_redis(db_connection.clone(), redis_client.clone());

    match service
        .update_card_play(game_id, player_id, chosen_card, correlation_id)
        .await
    {
        Ok(result) => {
            info!(
                "Bot {} successfully played card {} in game {}",
                player_id, chosen_card, game_id
            );

            if !result.game_ended {
                let next_is_bot = result
                    .players
                    .iter()
                    .find(|p| p.id == result.next_player_id)
                    .map(|p| matches!(p.player_type, PlayerType::Bot))
                    .unwrap_or(false);

                if next_is_bot {
                    info!(
                        "Next player {} is also a bot, scheduling next AI task for game {}",
                        result.next_player_id, game_id
                    );
                    if let Some(ref client) = rabbitmq_client {
                        let next_task = match service
                            .build_ai_task(game_id, result.next_player_id, correlation_id)
                            .await
                        {
                            Ok(task) => {
                                info!(
                                    "Built comprehensive AI task for bot {} in game {}",
                                    result.next_player_id, game_id
                                );
                                task
                            }
                            Err(e) => {
                                error!(
                                    "Failed to build comprehensive AI task: {}, falling back to minimal",
                                    e
                                );
                                AITask::minimal(game_id, result.next_player_id)
                            }
                        };
                        match client.publish_ai_task(&next_task).await {
                            Ok(()) => info!(
                                "Published AI task for bot {} in game {}",
                                result.next_player_id, game_id
                            ),
                            Err(e) => {
                                error!(
                                    "Failed to publish AI task to RabbitMQ: {}, falling back to sync chain",
                                    e
                                );
                                metrics::BOT_CHAIN_PUBLISH_FAILURES_TOTAL.inc();
                                metrics::BOT_CHAIN_FALLBACK_TOTAL.inc();
                                let db = db_connection.clone();
                                let redis = redis_client.clone();
                                let next_id = result.next_player_id;
                                tokio::spawn(async move {
                                    BotScheduler::run_sync_chain(
                                        db, redis, game_id, next_id, 86400, 250, None,
                                    )
                                    .await;
                                });
                            }
                        }
                    } else {
                        warn!("RabbitMQ client not available, cannot schedule next bot move");
                    }
                } else {
                    info!(
                        "Next player {} is human, stopping bot chain",
                        result.next_player_id
                    );
                }
            }

            let total_duration = start_time.elapsed();
            info!(
                "Total bot move processing took {:?} (computation: {:?})",
                total_duration, compute_duration
            );

            Ok(())
        }
        Err(e) => {
            error!("Failed to play bot card in database: {}", e);
            Err(anyhow::anyhow!("Database play failed: {}", e))
        }
    }
}

async fn process_bot_move_with_db(
    game_id: Uuid,
    player_id: Uuid,
    db_connection: DatabaseConnection,
    redis_client: Option<RedisClient>,
    rabbitmq_client: Option<RabbitMQClient>,
    correlation_id: Option<Uuid>,
) -> Result<()> {
    warn!("Using database-based bot move execution (fallback)");

    use crate::game::bot::execute_bot_move;
    use crate::messaging::events::GameEvent;

    match execute_bot_move(game_id, player_id, &db_connection).await {
        Ok(result) => {
            info!(
                "Database-based bot execution successful, card {}",
                result.chosen_card
            );

            let redis_for_event = redis_client.clone();
            if let Some(mut client) = redis_for_event {
                let event = GameEvent::CardPlayed {
                    game_id,
                    player_id,
                    card_index: result.chosen_card,
                    next_turn: result.next_player,
                    correlation_id,
                };
                if let PublishResult::RetryExhausted(e) =
                    client.publish_game_event_with_retry(&event).await
                {
                    error!("Failed to publish bot game event: {}", e);
                }
            }

            if result.should_continue {
                info!("Next player is also a bot, scheduling next AI task (fallback)");
                if let Some(next_player) = result.next_player {
                    if let Some(ref client) = rabbitmq_client {
                        let next_task = AITask::minimal(game_id, next_player);
                        match client.publish_ai_task(&next_task).await {
                            Ok(()) => info!(
                                "Published AI task for bot player {} in game {}",
                                next_player, game_id
                            ),
                            Err(e) => {
                                error!(
                                    "Failed to publish AI task to RabbitMQ: {}, falling back to sync chain",
                                    e
                                );
                                metrics::BOT_CHAIN_PUBLISH_FAILURES_TOTAL.inc();
                                metrics::BOT_CHAIN_FALLBACK_TOTAL.inc();
                                let db = db_connection.clone();
                                let redis = redis_client.clone();
                                let next_id = next_player;
                                tokio::spawn(async move {
                                    BotScheduler::run_sync_chain(
                                        db, redis, game_id, next_id, 86400, 250, None,
                                    )
                                    .await;
                                });
                            }
                        }
                    } else {
                        warn!("RabbitMQ client not available, cannot schedule next bot move");
                    }
                } else {
                    warn!("No next player indicated, cannot schedule next bot move");
                }
            }

            Ok(())
        }
        Err(e) => {
            error!("Database-based bot execution failed: {}", e);
            Err(anyhow::anyhow!("Bot execution failed: {}", e))
        }
    }
}
