use std::sync::Arc;

use sea_orm::DatabaseConnection;
use tokio::sync::watch;
use tokio::task::JoinSet;
use tracing::info;

use crate::cache::UserCache;
use crate::config::Config;
use crate::mailer::Mailer;
use crate::messaging::RedisClient;

pub mod tasks;

pub struct Scheduler {
    db: DatabaseConnection,
    redis: Option<RedisClient>,
    mailer: Arc<dyn Mailer>,
    user_cache: Arc<UserCache>,
    config: Config,
}

impl Scheduler {
    pub fn new(
        db: DatabaseConnection,
        redis: Option<RedisClient>,
        mailer: Arc<dyn Mailer>,
        user_cache: Arc<UserCache>,
        config: Config,
    ) -> Self {
        Self {
            db,
            redis,
            mailer,
            user_cache,
            config,
        }
    }

    pub fn run_all(self) -> (JoinSet<()>, watch::Sender<bool>) {
        let (shutdown_tx, shutdown_rx1) = watch::channel(false);
        let shutdown_rx2 = shutdown_rx1.clone();
        let shutdown_rx3 = shutdown_rx1.clone();
        let shutdown_rx4 = shutdown_rx1.clone();
        let shutdown_rx5 = shutdown_rx1.clone();
        let shutdown_rx6 = shutdown_rx1.clone();

        let db1 = self.db.clone();
        let db2 = self.db.clone();
        let db3 = self.db.clone();
        let db4 = self.db.clone();
        let db5 = self.db.clone();
        let db6 = self.db;

        let redis1 = self.redis.clone();
        let redis2 = self.redis.clone();
        let redis3 = self.redis.clone();
        let _redis4 = self.redis;

        let mailer1 = self.mailer.clone();
        let mailer2 = self.mailer.clone();
        let mailer3 = self.mailer.clone();
        let mailer4 = self.mailer;

        let user_cache1 = self.user_cache.clone();
        let _user_cache2 = self.user_cache;

        let config1 = self.config.clone();
        let config2 = self.config.clone();
        let config3 = self.config;
        let unfreeze_credit = config3.unfreeze_credit_no_payment;
        let db_pool_interval = config3.db_pool_metrics_interval_secs;
        let run_staleness_val = config3.run_staleness_timeout_secs;
        let config4 = config3;

        let mut tasks = JoinSet::new();

        tasks.spawn(async move {
            info!(task = "cancel_expired_games", "Task started");
            tasks::cancel_expired_games_loop(db1, redis1, config1, mailer1, shutdown_rx1).await;
            tracing::error!(task = "cancel_expired_games", "Task exited unexpectedly");
        });

        tasks.spawn(async move {
            info!(task = "detect_stalled_games", "Task started");
            tasks::detect_stalled_games_loop(db2, redis2, config2, mailer2, shutdown_rx2).await;
            tracing::error!(task = "detect_stalled_games", "Task exited unexpectedly");
        });

        tasks.spawn(async move {
            info!(task = "check_expired_freezes", "Task started");
            tasks::check_expired_freezes_loop(db3, mailer3, unfreeze_credit, shutdown_rx3).await;
            tracing::error!(task = "check_expired_freezes", "Task exited unexpectedly");
        });

        tasks.spawn(async move {
            info!(task = "refresh_leaderboard", "Task started");
            tasks::refresh_leaderboard_loop(db4, redis3, config4, user_cache1, shutdown_rx4).await;
            tracing::error!(task = "refresh_leaderboard", "Task exited unexpectedly");
        });

        tasks.spawn(async move {
            info!(task = "db_pool_metrics", "Task started");
            tasks::db_pool_metrics_loop(db5, db_pool_interval, shutdown_rx5).await;
            tracing::error!(task = "db_pool_metrics", "Task exited unexpectedly");
        });

        tasks.spawn(async move {
            info!(task = "check_stalled_runs", "Task started");
            tasks::check_stalled_runs_loop(db6, mailer4, run_staleness_val, shutdown_rx6).await;
            tracing::error!(task = "check_stalled_runs", "Task exited unexpectedly");
        });

        (tasks, shutdown_tx)
    }
}
