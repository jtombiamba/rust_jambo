use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder};
use tracing::{error, info};
use uuid::Uuid;

use crate::database::models::{game, player, GameStatus, PlayerType};
use crate::database::repositories::PlayerRepository;
use crate::i18n::Lang;
use crate::messaging::events::GameEvent;
use crate::messaging::redis::PublishResult;
use crate::messaging::RedisClient;
use crate::observability::metrics;
use crate::observability::metrics::EMAIL_SEND_ERRORS_TOTAL;

use super::GameService;

impl GameService {
    #[allow(dead_code)]
    pub async fn detect_and_recover_stalled_games(
        db: sea_orm::DatabaseConnection,
        redis_client: Option<RedisClient>,
        staleness_threshold: chrono::Duration,
    ) -> u64 {
        let now = chrono::Utc::now();
        let cutoff = now - staleness_threshold;

        let stalled_games = match game::Entity::find()
            .filter(game::Column::Status.eq(GameStatus::Active))
            .filter(game::Column::UpdatedAt.lt(cutoff))
            .all(&db)
            .await
        {
            Ok(games) => games,
            Err(e) => {
                error!("Failed to query stalled games: {}", e);
                return 0;
            }
        };

        let mut recovered = 0u64;
        for g in stalled_games {
            let player_repo = PlayerRepository::new(db.clone());
            let players = match player_repo.list_by_game(g.id).await {
                Ok(players) => players,
                Err(e) => {
                    error!("Failed to fetch players for game {}: {}", g.id, e);
                    continue;
                }
            };

            let current_rank = g.rank.unwrap_or(0) as usize;
            let current_player = match players.get(current_rank) {
                Some(player) if matches!(player.player_type, PlayerType::Bot) => player,
                _ => continue,
            };

            info!(
                "Detected stalled game {}: current bot player {}, last updated {:?} seconds ago",
                g.id,
                current_player.id,
                (now - g.updated_at).num_seconds()
            );

            metrics::GAMES_STALLED_TOTAL.inc();

            let db_clone = db.clone();
            let redis_clone = redis_client.clone();
            let game_id = g.id;
            let player_id = current_player.id;
            tokio::spawn(async move {
                crate::game::bot_scheduler::BotScheduler::run_sync_chain(
                    db_clone,
                    redis_clone,
                    game_id,
                    player_id,
                )
                .await;
            });

            recovered += 1;
        }

        if recovered > 0 {
            info!("Recovered {} stalled games", recovered);
        }
        recovered
    }

    #[allow(dead_code)]
    pub async fn check_human_staleness(
        &self,
        redis_client: Option<RedisClient>,
        alert_threshold: chrono::Duration,
        kick_threshold: chrono::Duration,
    ) -> u64 {
        let now = chrono::Utc::now();
        let alert_cutoff = now - alert_threshold;
        let kick_cutoff = now - kick_threshold;

        let stalled_games = match game::Entity::find()
            .filter(game::Column::Status.eq(GameStatus::Active))
            .filter(game::Column::UpdatedAt.lt(alert_cutoff))
            .all(&self.db)
            .await
        {
            Ok(games) => games,
            Err(e) => {
                error!("Failed to query human-stalled games: {}", e);
                return 0;
            }
        };

        let mut processed = 0u64;

        for g in stalled_games {
            let players = match player::Entity::find()
                .filter(player::Column::GameId.eq(g.id))
                .order_by_asc(player::Column::Position)
                .all(&self.db)
                .await
            {
                Ok(p) => p,
                Err(e) => {
                    error!("Failed to fetch players for game {}: {}", g.id, e);
                    continue;
                }
            };

            let current_rank = g.rank.unwrap_or(0) as usize;
            let current_player = match players.get(current_rank) {
                Some(p) if matches!(p.player_type, PlayerType::Human) && !p.kicked => p,
                _ => continue,
            };

            if g.stall_warning_sent_at.is_none() {
                let remaining_seconds = (kick_threshold - alert_threshold).num_seconds();
                let inactive_minutes = alert_threshold.num_minutes();
                let remaining_minutes = remaining_seconds / 60;

                info!(
                    "Human staleness warning for player {} in game {} (inactive {} min, kick in {} min)",
                    current_player.id, g.id, inactive_minutes, remaining_minutes
                );

                if let Some(ref redis) = redis_client {
                    let event = GameEvent::StalenessWarning {
                        game_id: g.id,
                        player_id: current_player.id,
                        player_name: current_player.name.clone(),
                        kicked_after_seconds: remaining_seconds,
                    };
                    if let PublishResult::RetryExhausted(e) =
                        redis.clone().publish_game_event_with_retry(&event).await
                    {
                        error!("Failed to publish StalenessWarning event: {}", e);
                    }
                }

                // Fire-and-forget the staleness email so it doesn't block the scheduler loop
                if let Some(user_id) = current_player.user_id {
                    let mailer = self.mailer.clone();
                    let db = self.db.clone();
                    // let user_id = current_player.user_id.unwrap();
                    let game_id = g.id;
                    tokio::spawn(async move {
                        Self::send_staleness_email_impl(
                            mailer,
                            db,
                            user_id,
                            game_id,
                            inactive_minutes,
                            remaining_minutes,
                        )
                        .await;
                    });
                }

                match game::Entity::update_many()
                    .col_expr(
                        game::Column::StallWarningSentAt,
                        sea_orm::sea_query::Expr::value(sea_orm::Value::ChronoDateTimeUtc(Some(
                            now,
                        ))),
                    )
                    .filter(game::Column::Id.eq(g.id))
                    .exec(&self.db)
                    .await
                {
                    Ok(result) => {
                        if result.rows_affected == 0 {
                            tracing::warn!(
                                "Stall warning update affected 0 rows for game {} (stale for {:?})",
                                g.id,
                                (now - g.updated_at).num_seconds()
                            );
                        } else {
                            info!(
                                "Stall warning sent for player {} in game {} (inactive {:?})",
                                current_player.id,
                                g.id,
                                (now - g.updated_at).num_seconds()
                            );
                        }
                    }
                    Err(e) => {
                        error!(
                            "Failed to set stall_warning_sent_at for game {}: {}",
                            g.id, e
                        );
                    }
                }

                processed += 1;
            } else if g.updated_at < kick_cutoff {
                info!(
                    "Kicking stalled human player {} ({}) from game {} (inactive for {:?}, total {}s)",
                    current_player.id,
                    current_player.name,
                    g.id,
                    now - g.updated_at,
                    (now - g.updated_at).num_seconds()
                );

                match self
                    .kick_player_from_game(g.id, current_player.id, &players)
                    .await
                {
                    Ok(()) => {
                        info!(
                            "Successfully kicked player {} from game {}",
                            current_player.id, g.id
                        );
                        processed += 1;
                    }
                    Err(e) => {
                        error!(
                            "Failed to kick player {} from game {}: {}",
                            current_player.id, g.id, e
                        );
                    }
                }
            } else {
                tracing::debug!(
                    "Game {} has stall_warning_sent_at but not yet past kick_cutoff (last_active={:?}, kick_cutoff={:?})",
                    g.id,
                    g.updated_at,
                    kick_cutoff
                );
            }
        }

        if processed > 0 {
            info!("Processed {} human-stalled games", processed);
        }
        processed
    }

    /// Fire-and-forget helper: sends a staleness warning email without blocking the caller.
    pub(crate) async fn send_staleness_email_impl(
        mailer: Option<std::sync::Arc<dyn crate::mailer::Mailer>>,
        db: sea_orm::DatabaseConnection,
        user_id: Uuid,
        game_id: Uuid,
        inactive_minutes: i64,
        remaining_minutes: i64,
    ) {
        let Some(mailer) = mailer else { return };

        let user = match crate::database::models::user::Entity::find_by_id(user_id)
            .one(&db)
            .await
        {
            Ok(Some(u)) => u,
            Ok(None) => {
                tracing::warn!("User {} not found for staleness warning email", user_id);
                EMAIL_SEND_ERRORS_TOTAL
                    .with_label_values(&["stall_warning"])
                    .inc();
                return;
            }
            Err(e) => {
                tracing::error!(
                    "DB error looking up user {} for staleness warning email: {}",
                    user_id,
                    e
                );
                EMAIL_SEND_ERRORS_TOTAL
                    .with_label_values(&["stall_warning"])
                    .inc();
                return;
            }
        };

        let lang = Lang::parse(&user.language).unwrap_or_default();
        let game_id_str = game_id.to_string();

        if let Err(e) = mailer
            .send_stall_warning(
                &user.email,
                &game_id_str,
                inactive_minutes,
                remaining_minutes,
                lang,
            )
            .await
        {
            tracing::error!(
                "Failed to send stall warning email to {}: {}",
                user.email,
                e
            );
            EMAIL_SEND_ERRORS_TOTAL
                .with_label_values(&["stall_warning"])
                .inc();
        }
    }

    /// Fire-and-forget helper: sends a kicked email without blocking the caller.
    pub(crate) async fn send_kicked_email_impl(
        mailer: Option<std::sync::Arc<dyn crate::mailer::Mailer>>,
        db: sea_orm::DatabaseConnection,
        user_id: Option<Uuid>,
        game_id: Uuid,
        bet: i32,
    ) {
        let Some(user_id) = user_id else { return };
        let Some(mailer) = mailer else { return };

        let user = match crate::database::models::user::Entity::find_by_id(user_id)
            .one(&db)
            .await
        {
            Ok(Some(u)) => u,
            Ok(None) => {
                tracing::warn!("User {} not found for kicked email", user_id);
                EMAIL_SEND_ERRORS_TOTAL
                    .with_label_values(&["stall_kicked"])
                    .inc();
                return;
            }
            Err(e) => {
                tracing::error!(
                    "DB error looking up user {} for kicked email: {}",
                    user_id,
                    e
                );
                EMAIL_SEND_ERRORS_TOTAL
                    .with_label_values(&["stall_kicked"])
                    .inc();
                return;
            }
        };

        let lang = Lang::parse(&user.language).unwrap_or_default();
        let game_id_str = game_id.to_string();

        if let Err(e) = mailer
            .send_stall_kicked(&user.email, &game_id_str, bet, lang)
            .await
        {
            tracing::error!("Failed to send stall kicked email to {}: {}", user.email, e);
            EMAIL_SEND_ERRORS_TOTAL
                .with_label_values(&["stall_kicked"])
                .inc();
        }
    }
}
