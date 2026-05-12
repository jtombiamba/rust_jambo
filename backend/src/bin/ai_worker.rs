use anyhow::{Context, Result};
use futures::StreamExt;
use lapin::{
    options::{BasicAckOptions, BasicNackOptions},
    Consumer,
};
use sea_orm::DatabaseConnection;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use jambo_backend::config::Config;
use jambo_backend::database;
use jambo_backend::database::models::PlayerType;
use jambo_backend::game::bot::execute_bot_move_from_task;
use jambo_backend::game::bot_scheduler::BotScheduler;
use jambo_backend::game::constants::BOT_THINKING_DELAY_SECS;
use jambo_backend::game::service::GameService;
use jambo_backend::messaging::{self, AITask, RabbitMQClient, RabbitMQPublishConfig, RedisClient};
use jambo_backend::observability::metrics;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("ai_worker=info".parse().unwrap()),
        )
        .json()
        .init();

    let config = Config::from_env().context("Failed to load configuration")?;
    let cpu_count = num_cpus::get();
    info!(
        "Starting AI worker — CPU cores: {}, Tokio runtime workers: {}",
        cpu_count,
        std::env::var("TOKIO_WORKER_THREADS").unwrap_or_else(|_| "default (num_cpus)".to_string())
    );

    let db_connection: DatabaseConnection = database::create_connection_with_pool_size(&config, 5)
        .await
        .context("Failed to create database connection")?;
    info!("Connected to database");

    let redis_client = match config.redis_url {
        Some(url) => match RedisClient::new(&url).await {
            Ok(client) => {
                info!("Connected to Redis");
                Some(client)
            }
            Err(e) => {
                warn!(
                    "Failed to connect to Redis: {}, proceeding without Redis",
                    e
                );
                None
            }
        },
        None => None,
    };

    let publish_config = RabbitMQPublishConfig {
        max_retries: config.rabbitmq_publish_max_retries,
        initial_retry_delay_ms: config.rabbitmq_publish_initial_retry_delay_ms,
        max_retry_delay_ms: config.rabbitmq_publish_max_retry_delay_ms,
        circuit_breaker_failure_threshold: config.circuit_breaker_failure_threshold,
        circuit_breaker_cooldown_secs: config.circuit_breaker_cooldown_secs,
    };

    let rabbitmq_client = messaging::connect_to_rabbitmq_with_retry(
        &config.rabbitmq_url,
        config.max_rabbitmq_connection_retries,
        publish_config,
    )
    .await
    .context("Failed to connect to RabbitMQ after retries")?;
    info!("Connected to RabbitMQ");

    let db_for_pool_metrics = db_connection.clone();
    let pool_metrics_interval =
        std::time::Duration::from_secs(config.db_pool_metrics_interval_secs);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(pool_metrics_interval);
        loop {
            interval.tick().await;
            metrics::update_db_pool_metrics(&db_for_pool_metrics);
        }
    });

    let mut consumer: Consumer = rabbitmq_client
        .consume_ai_tasks()
        .await
        .context("Failed to start consuming AI tasks")?;
    info!("Waiting for AI tasks...");

    let mut tasks_processed = 0u64;
    let mut tasks_failed = 0u64;
    let mut parse_errors = 0u64;
    let mut delivery_errors = 0u64;
    let start_time = std::time::Instant::now();

    while let Some(delivery_result) = consumer.next().await {
        let delivery = match delivery_result {
            Ok(d) => d,
            Err(e) => {
                error!("Error receiving delivery: {}", e);
                delivery_errors += 1;
                continue;
            }
        };
        let task = match AITask::from_json_bytes(&delivery.data) {
            Ok(task) => task,
            Err(e) => {
                error!("Failed to parse AI task: {}", e);
                parse_errors += 1;
                let _ = delivery
                    .nack(BasicNackOptions {
                        multiple: false,
                        requeue: false,
                    })
                    .await;
                continue;
            }
        };
        let game_id = task.game_id;
        let player_id = task.player_id;
        info!("Processing AI task: game={}, player={}", game_id, player_id);

        let task_start_time = std::time::Instant::now();
        let process_result = process_bot_move(
            task,
            db_connection.clone(),
            redis_client.clone(),
            Some(rabbitmq_client.clone()),
        )
        .await;

        if let Err(e) = process_result {
            error!("Failed to process bot move: {}", e);
            tasks_failed += 1;
            let _ = delivery
                .nack(BasicNackOptions {
                    multiple: false,
                    requeue: false,
                })
                .await;
        } else {
            tasks_processed += 1;
            let task_duration = task_start_time.elapsed();
            info!(
                "Successfully processed bot move for game {}, player {} in {:?}",
                game_id, player_id, task_duration
            );
            if let Err(e) = delivery.ack(BasicAckOptions::default()).await {
                error!("Failed to acknowledge message: {}", e);
            }
        }

        let total_tasks = tasks_processed + tasks_failed + parse_errors;
        if total_tasks.is_multiple_of(10) && total_tasks > 0 {
            let uptime = start_time.elapsed();
            info!(
                tasks_processed = tasks_processed,
                tasks_failed = tasks_failed,
                parse_errors = parse_errors,
                delivery_errors = delivery_errors,
                total_tasks = total_tasks,
                success_rate = if total_tasks > 0 {
                    tasks_processed as f64 / total_tasks as f64
                } else {
                    0.0
                },
                uptime_seconds = uptime.as_secs(),
                "Periodic metrics"
            );
        }
    }

    let uptime = start_time.elapsed();
    let total_tasks = tasks_processed + tasks_failed + parse_errors;
    info!(
        tasks_processed = tasks_processed,
        tasks_failed = tasks_failed,
        parse_errors = parse_errors,
        delivery_errors = delivery_errors,
        total_tasks = total_tasks,
        success_rate = if total_tasks > 0 {
            tasks_processed as f64 / total_tasks as f64
        } else {
            0.0
        },
        uptime_seconds = uptime.as_secs(),
        "AI worker shutting down - final metrics"
    );

    Ok(())
}

async fn process_bot_move(
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

    info!(
        "Bot {} thinking for {} second(s) before playing in game {}",
        player_id, BOT_THINKING_DELAY_SECS, game_id
    );
    tokio::time::sleep(std::time::Duration::from_secs(BOT_THINKING_DELAY_SECS)).await;

    let start_time = std::time::Instant::now();

    // Compute bot move using AITask (no DB queries)
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

            // CardPlayResult already contains next_player_id and players — no extra DB query needed
            if !result.game_ended {
                let next_is_bot = result
                    .players
                    .iter()
                    .find(|p| p.id == result.next_player_id)
                    .map(|p| matches!(p.player_type, PlayerType::Bot))
                    .unwrap_or(false);

                if next_is_bot {
                    info!(
                        "Next player {} is also a bot, scheduling next AI task",
                        result.next_player_id
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
                                    BotScheduler::run_sync_chain(db, redis, game_id, next_id).await;
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

/// Fallback function that uses database queries (for when AITask is incomplete or invalid)
async fn process_bot_move_with_db(
    game_id: Uuid,
    player_id: Uuid,
    db_connection: DatabaseConnection,
    redis_client: Option<RedisClient>,
    rabbitmq_client: Option<RabbitMQClient>,
    correlation_id: Option<Uuid>,
) -> Result<()> {
    warn!("Using database-based bot move execution (fallback)");

    use jambo_backend::game::bot::execute_bot_move;
    use jambo_backend::messaging::events::GameEvent;

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
                if let Err(e) = client.publish_game_event(&event).await {
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
                                    BotScheduler::run_sync_chain(db, redis, game_id, next_id).await;
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
