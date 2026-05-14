use std::sync::Arc;

use actix_web::{get, web, App, HttpResponse, HttpServer, Responder};
use prometheus::Encoder;
use tracing::info;

mod api;
mod auth;
mod cache;
mod config;
mod database;
mod error;
mod game;
mod mailer;
mod messaging;
mod observability;
mod websocket;

use api::anonymous::get_anonymous_stats;
use api::game::play_card;
use api::middleware::ip_forward::ForwardedIpMiddleware;
use api::middleware::rate_limiter::RateLimiterMiddleware;
use api::quickie::create_quick_game;
use auth::config::AuthConfig;
use auth::middleware::AuthMiddleware;
use cache::UserCache;
use config::Config;
use database::repositories::{DashboardRepository, UserRepository};
use game::orchestrator::{GameOrchestrator, GameOrchestratorTrait};
use mailer::{create_mailer, MailerConfig};
use messaging::RedisClient;
use observability::middleware::CorrelationIdMiddleware;
use websocket::manager::WebSocketManager;

#[get("/health")]
pub async fn health_check() -> impl Responder {
    HttpResponse::Ok().body("OK")
}

#[get("/metrics")]
async fn metrics() -> HttpResponse {
    let encoder = prometheus::TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = vec![];
    encoder.encode(&metric_families, &mut buffer).unwrap();
    HttpResponse::Ok()
        .content_type("text/plain; version=0.0.4")
        .body(buffer)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("jambo_backend=info".parse().unwrap()),
        )
        .json()
        .init();

    observability::metrics::init_all();

    let config = Config::from_env().expect("Failed to load configuration");
    let cpu_count = num_cpus::get();
    info!(
        "Starting server on {}:{} — CPU cores: {}, Actix workers: 2 (production-optimized)",
        config.host, config.port, cpu_count
    );

    let db_connection = database::create_connection(&config)
        .await
        .expect("Failed to create database connection");

    database::run_migrations(&db_connection)
        .await
        .expect("Failed to run migrations");

    let redis_client = match config.redis_url {
        Some(url) => match RedisClient::new(&url).await {
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

    let publish_config = messaging::RabbitMQPublishConfig {
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

    let auth_config = AuthConfig::from_env().expect("Failed to load auth configuration");
    let auth_config_data = web::Data::new(auth_config.clone());

    let mailer_config = MailerConfig::from_env();
    let mailer = create_mailer(mailer_config).expect("Failed to create mailer");
    let mailer_data = web::Data::new(mailer.clone());

    let db_clone = db_connection.clone();

    let user_repo = Arc::new(UserRepository::new(db_connection.clone()));
    let dashboard_repo = Arc::new(DashboardRepository::new(db_connection.clone()));
    let auth_service: Arc<api::auth::AuthServiceType> = Arc::new(
        api::services::auth_service::AuthService::new(user_repo, auth_config, mailer),
    );

    let user_cache = match redis_client.clone() {
        Some(rc) => Arc::new(UserCache::new_with_redis(rc)),
        None => Arc::new(UserCache::new()),
    };
    let dashboard_service: Arc<api::dashboard::DashboardServiceType> = match redis_client.clone() {
        Some(rc) => Arc::new(
            api::services::dashboard_service::DashboardService::new_with_redis(
                dashboard_repo,
                user_cache.clone(),
                rc,
            ),
        ),
        None => Arc::new(api::services::dashboard_service::DashboardService::new(
            dashboard_repo,
            user_cache.clone(),
        )),
    };

    let auth_service_data = web::Data::new(auth_service);
    let dashboard_service_data = web::Data::new(dashboard_service);
    let user_cache_data = web::Data::new(user_cache.clone());

    let orchestrator: Arc<dyn GameOrchestratorTrait> = Arc::new(GameOrchestrator::new(
        db_clone,
        redis_client.clone(),
        rabbitmq_client.clone(),
    ));

    let orchestrator_data = web::Data::new(orchestrator);
    let redis_client_data = web::Data::new(redis_client.clone());
    let rabbitmq_client_data = web::Data::new(rabbitmq_client.clone());

    let ws_manager_instance = WebSocketManager::new(redis_client.clone());
    if let Err(e) = ws_manager_instance.start_redis_subscriber().await {
        tracing::warn!("Failed to start Redis subscriber: {}", e);
    }
    use std::time::Duration;
    ws_manager_instance
        .start_connection_cleanup_task(Duration::from_secs(5 * 60), Duration::from_secs(10 * 60))
        .await;
    let ws_manager = web::Data::new(ws_manager_instance);

    let redis_for_canceller = redis_client.clone();
    let db_for_canceller = db_connection.clone();
    tokio::spawn(async move {
        let game_service =
            game::service::GameService::new_with_redis(db_for_canceller, redis_for_canceller);
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

    let db_for_staleness = db_connection.clone();
    let redis_for_staleness = redis_client.clone();
    let staleness_threshold =
        chrono::Duration::seconds(config.game_staleness_threshold_secs as i64);
    tokio::spawn(async move {
        let check_interval = std::cmp::max(
            Duration::from_secs(config.game_staleness_threshold_secs),
            Duration::from_secs(60),
        );
        let mut interval = tokio::time::interval(check_interval);
        loop {
            interval.tick().await;
            game::service::GameService::detect_and_recover_stalled_games(
                db_for_staleness.clone(),
                redis_for_staleness.clone(),
                staleness_threshold,
            )
            .await;
        }
    });

    let db_for_leaderboard = db_connection.clone();
    let redis_for_leaderboard = redis_client.clone();
    let user_cache_for_leaderboard = user_cache.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(5 * 60));
        loop {
            interval.tick().await;
            if let Some(redis) = redis_for_leaderboard.clone() {
                let profile_repo = database::repositories::PlayerProfileRepository::new(
                    db_for_leaderboard.clone(),
                );
                match profile_repo.list_all().await {
                    Ok(profiles) => {
                        // Refresh the Redis sorted sets with current scores
                        cache::leaderboard::refresh_leaderboard(redis, &profiles).await;

                        // Populate UserCache with pseudos for all leaderboard users
                        // so the leaderboard endpoint doesn't show "Unknown"
                        let user_repo =
                            database::repositories::UserRepository::new(db_for_leaderboard.clone());
                        for profile in &profiles {
                            if let Ok(Some(user)) = user_repo.find_by_id(profile.user_id).await {
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

    let auth_middleware = AuthMiddleware::new(redis_client.clone());
    let _rate_limiter_middleware = RateLimiterMiddleware::new(redis_client.clone());

    let db_for_pool_metrics = db_connection.clone();
    let pool_metrics_interval = Duration::from_secs(config.db_pool_metrics_interval_secs);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(pool_metrics_interval);
        loop {
            interval.tick().await;
            observability::metrics::update_db_pool_metrics(&db_for_pool_metrics);
        }
    });

    HttpServer::new(move || {
        info!("Registering routes");
        App::new()
            .wrap(CorrelationIdMiddleware)
            // .wrap(rate_limiter_middleware.clone()) // TODO: limit rate according to authenticated status
            .wrap(ForwardedIpMiddleware)
            .app_data(web::Data::new(db_connection.clone()))
            .app_data(redis_client_data.clone())
            .app_data(rabbitmq_client_data.clone())
            .app_data(ws_manager.clone())
            .app_data(orchestrator_data.clone())
            .app_data(auth_config_data.clone())
            .app_data(auth_service_data.clone())
            .app_data(dashboard_service_data.clone())
            .app_data(user_cache_data.clone())
            .app_data(mailer_data.clone())
            .service(health_check)
            .service(metrics)
            .service(
                web::scope("/api")
                    .service(get_anonymous_stats)
                    .service(create_quick_game)
                    .service(play_card)
                    .service(
                        web::scope("/auth")
                            .route("/register", web::post().to(api::auth::register))
                            .route("/login", web::post().to(api::auth::login))
                            .route(
                                "/forgot-password",
                                web::post().to(api::auth::forgot_password),
                            )
                            .route("/reset-password", web::post().to(api::auth::reset_password))
                            .route("/logout", web::post().to(api::auth::logout))
                            .service(
                                web::resource("/me")
                                    .wrap(auth_middleware.clone())
                                    .route(web::get().to(api::auth::me)),
                            ),
                    )
                    .service(
                        web::scope("/me")
                            .wrap(auth_middleware.clone())
                            .route("/profile", web::get().to(api::dashboard::get_profile))
                            .route("/games", web::get().to(api::dashboard::list_games))
                            .route("/games", web::post().to(api::dashboard::create_game))
                            .route("/games/{game_id}", web::get().to(api::dashboard::get_game))
                            .route(
                                "/active-game",
                                web::get().to(api::dashboard::get_active_game),
                            )
                            .route(
                                "/invitations",
                                web::get().to(api::dashboard::get_invitations),
                            )
                            .route(
                                "/leaderboard",
                                web::get().to(api::leaderboard::get_leaderboard),
                            ),
                    )
                    .service(
                        web::scope("/games")
                            .wrap(auth_middleware.clone())
                            .route(
                                "/{game_id}/invites",
                                web::post().to(api::dashboard::send_invites),
                            )
                            .route(
                                "/{game_id}/respond",
                                web::post().to(api::dashboard::respond_to_invite),
                            )
                            .route(
                                "/{game_id}/start",
                                web::post().to(api::dashboard::start_game),
                            )
                            .route("/{game_id}/play", web::post().to(api::dashboard::play_game))
                            .route("/{game_id}/me", web::get().to(api::dashboard::game_state)),
                    )
                    .service(
                        web::scope("/users")
                            .wrap(auth_middleware.clone())
                            .route("/search", web::get().to(api::dashboard::search_users)),
                    ),
            )
            .service(websocket::scope())
    })
    .bind((config.host.as_str(), config.port))?
    .workers(num_cpus::get())
    .run()
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, App};

    #[actix_web::test]
    async fn test_health_check() {
        let app = test::init_service(App::new().service(health_check)).await;
        let req = test::TestRequest::get().uri("/health").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body = test::read_body(resp).await;
        assert_eq!(body, "OK");
    }

    #[actix_web::test]
    async fn test_metrics() {
        let app = test::init_service(App::new().service(metrics)).await;
        let req = test::TestRequest::get().uri("/metrics").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let content_type = resp
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(
            content_type.starts_with("text/plain"),
            "Expected text/plain content-type, got: {content_type}"
        );
    }

    #[actix_web::test]
    async fn test_metrics_contains_default_metrics() {
        crate::observability::metrics::init_all();
        let app = test::init_service(App::new().service(metrics)).await;
        let req = test::TestRequest::get().uri("/metrics").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body = test::read_body(resp).await;
        let body_str = String::from_utf8(body.to_vec()).unwrap();
        assert!(
            body_str.contains("ws_messages_sent_total"),
            "Expected ws_messages_sent_total in metrics, got: {body_str}"
        );
        assert!(
            body_str.contains("ws_connections_active"),
            "Expected ws_connections_active in metrics, got: {body_str}"
        );
        assert!(
            body_str.contains("rabbitmq_consume_total"),
            "Expected rabbitmq_consume_total in metrics, got: {body_str}"
        );
        assert!(
            body_str.contains("rabbitmq_healthy"),
            "Expected rabbitmq_healthy in metrics, got: {body_str}"
        );
        assert!(
            body_str.contains("rabbitmq_publish_total"),
            "Expected rabbitmq_publish_total in metrics, got: {body_str}"
        );
        assert!(
            body_str.contains("games_finished_total"),
            "Expected games_finished_total in metrics, got: {body_str}"
        );
    }
}
