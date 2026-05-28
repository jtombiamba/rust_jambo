use std::sync::Arc;
use std::time::Duration;

use actix_web::web;
use tracing::info;

use crate::api::auth::AuthServiceType;
use crate::api::dashboard::DashboardServiceType;
use crate::api::middleware::rate_limiter::RateLimitConfigs;
use crate::api::services::room_service::RoomService;
use crate::auth::config::AuthConfig;
use crate::auth::middleware::AuthMiddleware;
use crate::cache::UserCache;
use crate::config::Config;
use crate::database::repositories::{DashboardRepository, UserRepository};
use crate::game::orchestrator::{GameOrchestrator, GameOrchestratorTrait};
use crate::i18n::Translator;
use crate::mailer::{self, Mailer, MailerConfig};
use crate::messaging::{self, RabbitMQClient, RabbitMQPublishConfig, RedisClient};
use crate::payment::PaymentService;
use crate::websocket::manager::WebSocketManager;

#[derive(Clone)]
pub struct AppState {
    pub db: web::Data<sea_orm::DatabaseConnection>,
    pub redis: web::Data<Option<RedisClient>>,
    pub rabbitmq: web::Data<Option<RabbitMQClient>>,
    pub ws_manager: web::Data<WebSocketManager>,
    pub orchestrator: web::Data<Arc<dyn GameOrchestratorTrait>>,
    pub auth_config: web::Data<AuthConfig>,
    pub auth_service: web::Data<Arc<AuthServiceType>>,
    pub dashboard_service: web::Data<Arc<DashboardServiceType>>,
    pub user_cache: web::Data<Arc<UserCache>>,
    pub mailer: web::Data<Arc<dyn Mailer>>,
    pub payment_service: web::Data<Arc<PaymentService>>,
    pub room_service: web::Data<Arc<RoomService>>,
    pub config: web::Data<Config>,
    pub auth_middleware: AuthMiddleware,
    pub rate_limit_configs: RateLimitConfigs,
    pub translator: web::Data<Arc<Translator>>,
}

pub async fn bootstrap(config: &Config) -> Result<AppState, Box<dyn std::error::Error>> {
    let db_connection = crate::database::create_connection(config)
        .await
        .map_err(|e| format!("Failed to create database connection: {e}"))?;

    crate::database::run_migrations(&db_connection)
        .await
        .map_err(|e| format!("Failed to run migrations: {e}"))?;

    let redis_client = match &config.redis_url {
        Some(url) => match RedisClient::new(url).await {
            Ok(client) => Some(client),
            Err(e) => {
                tracing::warn!(
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

    let rabbitmq_client = match messaging::connect_to_rabbitmq_with_retry(
        &config.rabbitmq_url,
        config.max_rabbitmq_connection_retries,
        publish_config,
    )
    .await
    {
        Ok(client) => {
            tracing::info!("Successfully connected to RabbitMQ");
            Some(client)
        }
        Err(e) => {
            tracing::warn!(
                "Failed to connect to RabbitMQ after retries: {}, proceeding without RabbitMQ",
                e
            );
            None
        }
    };

    let auth_config =
        AuthConfig::from_env().map_err(|e| format!("Failed to load auth configuration: {e}"))?;
    let mailer_config = MailerConfig::from_env();
    let mailer = mailer::create_mailer(mailer_config)
        .map_err(|e| format!("Failed to create mailer: {e}"))?;

    let db_clone = db_connection.clone();

    let user_repo = Arc::new(UserRepository::new(
        db_connection.clone(),
        config.default_credit,
    ));
    let dashboard_repo = Arc::new(DashboardRepository::new(db_connection.clone()));
    let translator = Arc::new(Translator::new());
    let auth_service: Arc<AuthServiceType> =
        Arc::new(crate::api::services::auth_service::AuthService::new(
            user_repo,
            auth_config.clone(),
            mailer.clone(),
            translator.clone(),
        ));

    let user_cache = match redis_client.clone() {
        Some(rc) => Arc::new(UserCache::new_with_redis(rc)),
        None => Arc::new(UserCache::new()),
    };
    let dashboard_service: Arc<DashboardServiceType> = match redis_client.clone() {
        Some(rc) => Arc::new(
            crate::api::services::dashboard_service::DashboardService::new_with_redis(
                dashboard_repo,
                user_cache.clone(),
                rc,
                config.default_credit,
            ),
        ),
        None => Arc::new(
            crate::api::services::dashboard_service::DashboardService::new(
                dashboard_repo,
                user_cache.clone(),
                config.default_credit,
            ),
        ),
    };

    let orchestrator: Arc<dyn GameOrchestratorTrait> = Arc::new(GameOrchestrator::new(
        db_clone,
        redis_client.clone(),
        rabbitmq_client.clone(),
        config.clone(),
        mailer.clone(),
    ));

    let ws_manager = WebSocketManager::new(redis_client.clone());
    if let Err(e) = ws_manager.start_redis_subscriber().await {
        tracing::warn!("Failed to start Redis subscriber: {}", e);
    }
    ws_manager
        .start_connection_cleanup_task(Duration::from_secs(5 * 60), Duration::from_secs(10 * 60))
        .await;

    {
        let redis_for_canceller = redis_client.clone();
        let db_for_canceller = db_connection.clone();
        let cancel_config = config.clone();
        let cancel_mailer = mailer.clone();
        tokio::spawn(async move {
            let game_service = crate::game::service::GameService::new_with_redis(
                db_for_canceller,
                redis_for_canceller,
            )
            .with_config(&cancel_config, cancel_mailer);
            let mut interval = tokio::time::interval(Duration::from_secs(30));
            loop {
                interval.tick().await;
                match game_service.cancel_expired_games().await {
                    Ok(0) => {}
                    Ok(n) => {
                        tracing::info!("Cancelled {} expired multiplayer games", n);
                    }
                    Err(e) => {
                        tracing::error!("Error cancelling expired games: {}", e);
                    }
                }
            }
        });
    }

    {
        let db_for_staleness = db_connection.clone();
        let redis_for_staleness = redis_client.clone();
        let config_for_staleness = config.clone();
        let staleness_secs = config.game_staleness_threshold_secs;
        let staleness_threshold = chrono::Duration::seconds(staleness_secs as i64);
        let human_alert_threshold =
            chrono::Duration::seconds(config.game_human_staleness_alert_secs as i64);
        let human_kick_threshold =
            chrono::Duration::seconds(config.game_human_staleness_kick_secs as i64);
        let mailer_for_staleness = mailer.clone();
        tokio::spawn(async move {
            let check_interval =
                std::cmp::max(Duration::from_secs(60), Duration::from_secs(staleness_secs));
            let mut interval = tokio::time::interval(check_interval);
            loop {
                interval.tick().await;
                crate::game::service::GameService::detect_and_recover_stalled_games(
                    db_for_staleness.clone(),
                    redis_for_staleness.clone(),
                    staleness_threshold,
                )
                .await;

                let game_service = crate::game::service::GameService::new_with_redis(
                    db_for_staleness.clone(),
                    redis_for_staleness.clone(),
                )
                .with_config(&config_for_staleness, mailer_for_staleness.clone());
                game_service
                    .check_human_staleness(
                        redis_for_staleness.clone(),
                        human_alert_threshold,
                        human_kick_threshold,
                    )
                    .await;
            }
        });
    }

    {
        let db_for_freeze = db_connection.clone();
        let mailer_for_freeze = mailer.clone();
        let freeze_config = config.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            loop {
                interval.tick().await;
                check_expired_freezes(
                    &db_for_freeze,
                    &*mailer_for_freeze,
                    freeze_config.unfreeze_credit_no_payment,
                )
                .await;
            }
        });
    }

    {
        let db_for_leaderboard = db_connection.clone();
        let redis_for_leaderboard = redis_client.clone();
        let user_cache_for_leaderboard = user_cache.clone();
        let leaderboard_config = config.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(5 * 60));
            loop {
                interval.tick().await;
                if let Some(redis) = redis_for_leaderboard.clone() {
                    let profile_repo = crate::database::repositories::PlayerProfileRepository::new(
                        db_for_leaderboard.clone(),
                    );
                    match profile_repo.list_all().await {
                        Ok(profiles) => {
                            crate::cache::leaderboard::refresh_leaderboard(redis, &profiles).await;
                            let user_repo = crate::database::repositories::UserRepository::new(
                                db_for_leaderboard.clone(),
                                leaderboard_config.default_credit,
                            );
                            for profile in &profiles {
                                if let Ok(Some(user)) = user_repo.find_by_id(profile.user_id).await
                                {
                                    user_cache_for_leaderboard
                                        .put(user.id, user.pseudo, user.email)
                                        .await;
                                }
                            }
                        }
                        Err(e) => {
                            tracing::error!("Failed to refresh leaderboard: {}", e);
                        }
                    }
                }
            }
        });
    }

    {
        let db_for_pool_metrics = db_connection.clone();
        let pool_metrics_interval = Duration::from_secs(config.db_pool_metrics_interval_secs);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(pool_metrics_interval);
            loop {
                interval.tick().await;
                crate::observability::metrics::update_db_pool_metrics(
                    &db_for_pool_metrics,
                    "backend",
                );
            }
        });
    }

    let auth_middleware = AuthMiddleware::new(redis_client.clone(), translator.clone());
    let rate_limit_configs = RateLimitConfigs::from_config(config);

    let payment_service = Arc::new(PaymentService::new(
        config.paypal_client_id.clone(),
        config.paypal_client_secret.clone(),
        config.paypal_mode.clone(),
        config.paypal_unfreeze_amount_eur.clone(),
        config.paypal_topup_amount_eur.clone(),
        config.paypal_sandbox_url.clone(),
        config.paypal_live_url.clone(),
    ));

    let room_service = Arc::new(RoomService::new(
        db_connection.clone(),
        mailer.clone(),
        config.clone(),
        redis_client.clone(),
    ));

    {
        let db_for_stalled_runs = db_connection.clone();
        let mailer_for_stalled_runs = mailer.clone();
        let timeout_secs = config.run_staleness_timeout_secs;
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(120));
            loop {
                interval.tick().await;
                RoomService::check_stalled_runs(
                    db_for_stalled_runs.clone(),
                    mailer_for_stalled_runs.clone(),
                    timeout_secs,
                )
                .await;
            }
        });
    }

    let db_data = web::Data::new(db_connection.clone());
    let redis_data = web::Data::new(redis_client);
    let rabbitmq_data = web::Data::new(rabbitmq_client);
    let ws_manager_data = web::Data::new(ws_manager);
    let orchestrator_data = web::Data::new(orchestrator);
    let auth_config_data = web::Data::new(auth_config);
    let auth_service_data = web::Data::new(auth_service);
    let dashboard_service_data = web::Data::new(dashboard_service);
    let user_cache_data = web::Data::new(user_cache);
    let mailer_data = web::Data::new(mailer);
    let payment_service_data = web::Data::new(payment_service);
    let room_service_data = web::Data::new(room_service);
    let config_data = web::Data::new(config.clone());
    let translator_data = web::Data::new(translator);

    info!("All resources initialized successfully");

    Ok(AppState {
        db: db_data,
        redis: redis_data,
        rabbitmq: rabbitmq_data,
        ws_manager: ws_manager_data,
        orchestrator: orchestrator_data,
        auth_config: auth_config_data,
        auth_service: auth_service_data,
        dashboard_service: dashboard_service_data,
        user_cache: user_cache_data,
        mailer: mailer_data,
        payment_service: payment_service_data,
        room_service: room_service_data,
        config: config_data,
        auth_middleware,
        rate_limit_configs,
        translator: translator_data,
    })
}

async fn check_expired_freezes(db: &sea_orm::DatabaseConnection, mailer: &dyn Mailer, credit: i32) {
    use crate::database::models::{player_profile, user};
    use sea_orm::{ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, QueryFilter};

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
                        crate::i18n::Lang::parse(&user_model.language).unwrap_or_default(),
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
