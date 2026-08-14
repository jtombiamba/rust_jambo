use std::sync::Arc;
use std::time::Duration;

use sea_orm::{ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, QueryFilter};
use tokio::sync::watch;
use tracing;

use crate::cache::leaderboard;
use crate::config::Config;
use crate::database::models::{player_profile, user};
use crate::database::repositories::{PlayerProfileRepository, UserRepository};
use crate::game::service::GameService;
use crate::i18n::Lang;
use crate::mailer::Mailer;
use crate::messaging::RedisClient;
use crate::observability::metrics;

macro_rules! record_task_metrics {
    ($task_name:expr, $start:expr) => {
        let duration = $start.elapsed();
        metrics::SCHEDULER_TASK_DURATION
            .with_label_values(&[$task_name])
            .observe(duration.as_secs_f64());
        metrics::SCHEDULER_LAST_RUN
            .with_label_values(&[$task_name])
            .set(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs_f64(),
            );
    };
}

macro_rules! record_task_timeout {
    ($task_name:expr) => {
        metrics::SCHEDULER_TASK_TIMEOUTS
            .with_label_values(&[$task_name])
            .inc();
    };
}

macro_rules! record_task_error {
    ($task_name:expr) => {
        metrics::SCHEDULER_TASK_ERRORS
            .with_label_values(&[$task_name])
            .inc();
    };
}

pub async fn cancel_expired_games_loop(
    db: sea_orm::DatabaseConnection,
    redis: Option<RedisClient>,
    config: Config,
    mailer: Arc<dyn Mailer>,
    mut shutdown: watch::Receiver<bool>,
) {
    let service = GameService::new_with_redis(db, redis).with_config(&config, mailer);
    let mut interval = tokio::time::interval(Duration::from_secs(30));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = interval.tick() => {
                let start = std::time::Instant::now();
                match tokio::time::timeout(Duration::from_secs(15), service.cancel_expired_games()).await {
                    Ok(Ok(n)) => {
                        record_task_metrics!("cancel_expired_games", start);
                        if n > 0 {
                            tracing::info!("Cancelled {} expired multiplayer games", n);
                        }
                    }
                    Ok(Err(e)) => {
                        record_task_error!("cancel_expired_games");
                        tracing::error!("Error cancelling expired games: {}", e);
                    }
                    Err(_elapsed) => {
                        record_task_timeout!("cancel_expired_games");
                        tracing::warn!("cancel_expired_games timed out after 15s, skipping tick");
                    }
                }
            }
            Ok(()) = shutdown.changed() => {
                tracing::info!("cancel_expired_games_loop received shutdown signal");
                break;
            }
        }
    }
}

pub async fn detect_stalled_games_loop(
    db: sea_orm::DatabaseConnection,
    redis: Option<RedisClient>,
    config: Config,
    mailer: Arc<dyn Mailer>,
    mut shutdown: watch::Receiver<bool>,
) {
    let staleness_secs = config.game_staleness_threshold_secs;
    let staleness_threshold = chrono::Duration::seconds(staleness_secs as i64);
    let human_alert_threshold =
        chrono::Duration::seconds(config.game_human_staleness_alert_secs as i64);
    let human_kick_threshold =
        chrono::Duration::seconds(config.game_human_staleness_kick_secs as i64);

    let check_interval =
        std::cmp::max(Duration::from_secs(60), Duration::from_secs(staleness_secs));
    let mut interval = tokio::time::interval(check_interval);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    let game_service =
        GameService::new_with_redis(db.clone(), redis.clone()).with_config(&config, mailer);

    loop {
        tokio::select! {
            _ = interval.tick() => {
                let start = std::time::Instant::now();
                match tokio::time::timeout(
                    Duration::from_secs(30),
                    GameService::detect_and_recover_stalled_games(
                        db.clone(),
                        redis.clone(),
                        staleness_threshold,
                    ),
                )
                .await
                {
                    Ok(recovered) => {
                        record_task_metrics!("detect_stalled_games", start);
                        if recovered > 0 {
                            tracing::info!("Recovered {} stalled games", recovered);
                        }
                    }
                    Err(_elapsed) => {
                        record_task_timeout!("detect_stalled_games");
                        tracing::warn!("detect_stalled_games timed out after 30s");
                    }
                }

                let start = std::time::Instant::now();
                match tokio::time::timeout(
                    Duration::from_secs(15),
                    game_service.check_human_staleness(
                        redis.clone(),
                        human_alert_threshold,
                        human_kick_threshold,
                    ),
                )
                .await
                {
                    Ok(n) => {
                        record_task_metrics!("check_human_staleness", start);
                        if n > 0 {
                            tracing::info!("Processed {} human staleness checks", n);
                        }
                    }
                    Err(_elapsed) => {
                        record_task_timeout!("check_human_staleness");
                        tracing::warn!("check_human_staleness timed out after 15s");
                    }
                }
            }
            Ok(()) = shutdown.changed() => {
                tracing::info!("detect_stalled_games_loop received shutdown signal");
                break;
            }
        }
    }
}

pub async fn check_expired_freezes_loop(
    db: sea_orm::DatabaseConnection,
    mailer: Arc<dyn Mailer>,
    credit: i32,
    mut shutdown: watch::Receiver<bool>,
) {
    let mut interval = tokio::time::interval(Duration::from_secs(60));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = interval.tick() => {
                let start = std::time::Instant::now();
                match tokio::time::timeout(
                    Duration::from_secs(30),
                    check_expired_freezes(&db, &*mailer, credit),
                )
                .await
                {
                    Ok(()) => {
                        record_task_metrics!("check_expired_freezes", start);
                    }
                    Err(_elapsed) => {
                        record_task_timeout!("check_expired_freezes");
                        tracing::warn!("check_expired_freezes timed out after 30s");
                    }
                }
            }
            Ok(()) = shutdown.changed() => {
                tracing::info!("check_expired_freezes_loop received shutdown signal");
                break;
            }
        }
    }
}

pub async fn refresh_leaderboard_loop(
    db: sea_orm::DatabaseConnection,
    redis: Option<RedisClient>,
    config: Config,
    user_cache: Arc<crate::cache::UserCache>,
    mut shutdown: watch::Receiver<bool>,
) {
    let mut interval = tokio::time::interval(Duration::from_secs(5 * 60));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = interval.tick() => {
                let start = std::time::Instant::now();
                match tokio::time::timeout(
                    Duration::from_secs(60),
                    refresh_leaderboard_inner(
                        db.clone(),
                        redis.clone(),
                        config.default_credit,
                        &user_cache,
                    ),
                )
                .await
                {
                    Ok(()) => {
                        record_task_metrics!("refresh_leaderboard", start);
                    }
                    Err(_elapsed) => {
                        record_task_timeout!("refresh_leaderboard");
                        tracing::warn!("refresh_leaderboard timed out after 60s");
                    }
                }
            }
            Ok(()) = shutdown.changed() => {
                tracing::info!("refresh_leaderboard_loop received shutdown signal");
                break;
            }
        }
    }
}

async fn refresh_leaderboard_inner(
    db: sea_orm::DatabaseConnection,
    redis: Option<RedisClient>,
    default_credit: i32,
    user_cache: &crate::cache::UserCache,
) {
    let redis = match redis {
        Some(r) => r,
        None => {
            tracing::warn!("Skipping leaderboard refresh: Redis unavailable");
            record_task_error!("refresh_leaderboard");
            return;
        }
    };

    let profile_repo = PlayerProfileRepository::new(db.clone());
    let profiles = match profile_repo.list_all().await {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("Failed to list all profiles for leaderboard: {}", e);
            return;
        }
    };

    leaderboard::refresh_leaderboard(redis.clone(), &profiles).await;

    let user_repo = UserRepository::new(db, default_credit);
    for profile in &profiles {
        match user_repo.find_by_id(profile.user_id).await {
            Ok(Some(user)) => {
                user_cache.put(user.id, user.pseudo, user.email).await;
            }
            Ok(None) => {
                tracing::warn!("No user found for profile {}", profile.user_id);
            }
            Err(e) => {
                tracing::error!(
                    "Failed to look up user for profile {}: {}",
                    profile.user_id,
                    e
                );
            }
        }
    }
}

pub async fn db_pool_metrics_loop(
    db: sea_orm::DatabaseConnection,
    interval_secs: u64,
    mut shutdown: watch::Receiver<bool>,
) {
    let pool_metrics_interval = Duration::from_secs(interval_secs);
    let mut interval = tokio::time::interval(pool_metrics_interval);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = interval.tick() => {
                let start = std::time::Instant::now();
                metrics::update_db_pool_metrics(&db, "scheduler_worker");
                record_task_metrics!("db_pool_metrics", start);
            }
            Ok(()) = shutdown.changed() => {
                tracing::info!("db_pool_metrics_loop received shutdown signal");
                break;
            }
        }
    }
}

pub async fn check_stalled_runs_loop(
    db: sea_orm::DatabaseConnection,
    mailer: Arc<dyn Mailer>,
    timeout_secs: u64,
    mut shutdown: watch::Receiver<bool>,
) {
    let mut interval = tokio::time::interval(Duration::from_secs(120));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = interval.tick() => {
                let start = std::time::Instant::now();
                match tokio::time::timeout(
                    Duration::from_secs(30),
                    crate::room::RoomService::check_stalled_runs(
                        db.clone(),
                        mailer.clone(),
                        timeout_secs,
                    ),
                )
                .await
                {
                    Ok(n) => {
                        record_task_metrics!("check_stalled_runs", start);
                        if n > 0 {
                            tracing::info!("Processed {} stalled runs", n);
                        }
                    }
                    Err(_elapsed) => {
                        record_task_timeout!("check_stalled_runs");
                        tracing::warn!("check_stalled_runs timed out after 30s");
                    }
                }
            }
            Ok(()) = shutdown.changed() => {
                tracing::info!("check_stalled_runs_loop received shutdown signal");
                break;
            }
        }
    }
}

async fn check_expired_freezes(db: &sea_orm::DatabaseConnection, mailer: &dyn Mailer, credit: i32) {
    let now = chrono::Utc::now();

    let expired_profiles = match player_profile::Entity::find()
        .filter(player_profile::Column::FrozenUntil.is_not_null())
        .filter(player_profile::Column::FrozenUntil.lte(now))
        .all(db)
        .await
    {
        Ok(profiles) => profiles,
        Err(e) => {
            tracing::error!("Failed to query expired freezes: {}", e);
            return;
        }
    };

    for profile in expired_profiles {
        let mut active: player_profile::ActiveModel = profile.clone().into();
        active.credit = ActiveValue::Set(credit);
        active.frozen_until = ActiveValue::Set(None);
        active.updated_at = ActiveValue::Set(chrono::Utc::now());

        if let Err(e) = active.update(db).await {
            tracing::error!(
                "Failed to update profile {} for expired freeze: {}",
                profile.user_id,
                e
            );
            continue;
        }

        match user::Entity::find_by_id(profile.user_id).one(db).await {
            Ok(Some(user_model)) => {
                if let Err(e) = mailer
                    .send_freeze_expired(
                        &user_model.email,
                        credit,
                        Lang::parse(&user_model.language).unwrap_or_default(),
                    )
                    .await
                {
                    tracing::error!(
                        "Failed to send unfreeze email to {}: {}",
                        user_model.email,
                        e
                    );
                }
            }
            Ok(None) => {
                tracing::warn!("No user found for profile {}", profile.user_id);
            }
            Err(e) => {
                tracing::error!(
                    "Failed to look up user for profile {}: {}",
                    profile.user_id,
                    e
                );
            }
        }
    }
}
