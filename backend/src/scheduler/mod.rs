use std::sync::Arc;

use sea_orm::DatabaseConnection;
use tokio::sync::watch;
use tokio::task::JoinSet;
use tracing::{error, info, warn};

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

        let task_max_restarts = self.config.scheduler_task_max_restarts;

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
            let mut restart_count: u32 = 0;
            let mut window_start = tokio::time::Instant::now();
            loop {
                tasks::cancel_expired_games_loop(
                    db1.clone(),
                    redis1.clone(),
                    config1.clone(),
                    mailer1.clone(),
                    shutdown_rx1.clone(),
                )
                .await;
                warn!(task = "cancel_expired_games", "Task exited, restarting");
                restart_count += 1;
                if window_start.elapsed() > std::time::Duration::from_secs(300) {
                    restart_count = 0;
                    window_start = tokio::time::Instant::now();
                }
                if restart_count > task_max_restarts {
                    error!(
                        task = "cancel_expired_games",
                        restart_count,
                        max_restarts = task_max_restarts,
                        "Task exceeded max restarts in 5min window, sleeping 60s"
                    );
                    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                    restart_count = 0;
                    window_start = tokio::time::Instant::now();
                }
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        });

        tasks.spawn(async move {
            info!(task = "detect_stalled_games", "Task started");
            let mut restart_count: u32 = 0;
            let mut window_start = tokio::time::Instant::now();
            loop {
                tasks::detect_stalled_games_loop(
                    db2.clone(),
                    redis2.clone(),
                    config2.clone(),
                    mailer2.clone(),
                    shutdown_rx2.clone(),
                )
                .await;
                warn!(task = "detect_stalled_games", "Task exited, restarting");
                restart_count += 1;
                if window_start.elapsed() > std::time::Duration::from_secs(300) {
                    restart_count = 0;
                    window_start = tokio::time::Instant::now();
                }
                if restart_count > task_max_restarts {
                    error!(
                        task = "detect_stalled_games",
                        restart_count,
                        max_restarts = task_max_restarts,
                        "Task exceeded max restarts in 5min window, sleeping 60s"
                    );
                    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                    restart_count = 0;
                    window_start = tokio::time::Instant::now();
                }
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        });

        tasks.spawn(async move {
            info!(task = "check_expired_freezes", "Task started");
            let mut restart_count: u32 = 0;
            let mut window_start = tokio::time::Instant::now();
            loop {
                tasks::check_expired_freezes_loop(
                    db3.clone(),
                    mailer3.clone(),
                    unfreeze_credit,
                    shutdown_rx3.clone(),
                )
                .await;
                warn!(task = "check_expired_freezes", "Task exited, restarting");
                restart_count += 1;
                if window_start.elapsed() > std::time::Duration::from_secs(300) {
                    restart_count = 0;
                    window_start = tokio::time::Instant::now();
                }
                if restart_count > task_max_restarts {
                    error!(
                        task = "check_expired_freezes",
                        restart_count,
                        max_restarts = task_max_restarts,
                        "Task exceeded max restarts in 5min window, sleeping 60s"
                    );
                    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                    restart_count = 0;
                    window_start = tokio::time::Instant::now();
                }
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        });

        tasks.spawn(async move {
            info!(task = "refresh_leaderboard", "Task started");
            let mut restart_count: u32 = 0;
            let mut window_start = tokio::time::Instant::now();
            loop {
                tasks::refresh_leaderboard_loop(
                    db4.clone(),
                    redis3.clone(),
                    config4.clone(),
                    user_cache1.clone(),
                    shutdown_rx4.clone(),
                )
                .await;
                warn!(task = "refresh_leaderboard", "Task exited, restarting");
                restart_count += 1;
                if window_start.elapsed() > std::time::Duration::from_secs(300) {
                    restart_count = 0;
                    window_start = tokio::time::Instant::now();
                }
                if restart_count > task_max_restarts {
                    error!(
                        task = "refresh_leaderboard",
                        restart_count,
                        max_restarts = task_max_restarts,
                        "Task exceeded max restarts in 5min window, sleeping 60s"
                    );
                    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                    restart_count = 0;
                    window_start = tokio::time::Instant::now();
                }
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        });

        tasks.spawn(async move {
            info!(task = "db_pool_metrics", "Task started");
            let mut restart_count: u32 = 0;
            let mut window_start = tokio::time::Instant::now();
            loop {
                tasks::db_pool_metrics_loop(db5.clone(), db_pool_interval, shutdown_rx5.clone())
                    .await;
                warn!(task = "db_pool_metrics", "Task exited, restarting");
                restart_count += 1;
                if window_start.elapsed() > std::time::Duration::from_secs(300) {
                    restart_count = 0;
                    window_start = tokio::time::Instant::now();
                }
                if restart_count > task_max_restarts {
                    error!(
                        task = "db_pool_metrics",
                        restart_count,
                        max_restarts = task_max_restarts,
                        "Task exceeded max restarts in 5min window, sleeping 60s"
                    );
                    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                    restart_count = 0;
                    window_start = tokio::time::Instant::now();
                }
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        });

        tasks.spawn(async move {
            info!(task = "check_stalled_runs", "Task started");
            let mut restart_count: u32 = 0;
            let mut window_start = tokio::time::Instant::now();
            loop {
                tasks::check_stalled_runs_loop(
                    db6.clone(),
                    mailer4.clone(),
                    run_staleness_val,
                    shutdown_rx6.clone(),
                )
                .await;
                warn!(task = "check_stalled_runs", "Task exited, restarting");
                restart_count += 1;
                if window_start.elapsed() > std::time::Duration::from_secs(300) {
                    restart_count = 0;
                    window_start = tokio::time::Instant::now();
                }
                if restart_count > task_max_restarts {
                    error!(
                        task = "check_stalled_runs",
                        restart_count,
                        max_restarts = task_max_restarts,
                        "Task exceeded max restarts in 5min window, sleeping 60s"
                    );
                    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                    restart_count = 0;
                    window_start = tokio::time::Instant::now();
                }
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        });

        (tasks, shutdown_tx)
    }
}
