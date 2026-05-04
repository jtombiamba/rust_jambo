use std::sync::Arc;

use actix_web::{get, web, App, HttpResponse, HttpServer, Responder};
use tracing::info;

mod api;
mod config;
mod database;
mod error;
mod game;
mod messaging;
mod observability;
mod websocket;

use config::Config;
use api::anonymous::get_anonymous_stats;
use api::quickie::create_quick_game;
use api::game::{list_games, get_my_cards, play_card, start_game};
use game::orchestrator::{GameOrchestrator, GameOrchestratorTrait};
use messaging::{RabbitMQClient, RedisClient};
use observability::middleware::CorrelationIdMiddleware;
use websocket::manager::WebSocketManager;

#[get("/health")]
pub async fn health_check() -> impl Responder {
    HttpResponse::Ok().body("OK")
}

#[get("/metrics")]
async fn metrics(rabbitmq_client: web::Data<Option<RabbitMQClient>>) -> impl Responder {
    use serde_json::json;

    let mut metrics_data = json!({
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "service": "jambo-backend",
        "rabbitmq_available": rabbitmq_client.is_some(),
    });

    if let Some(client) = rabbitmq_client.get_ref() {
        let rabbitmq_metrics = client.get_metrics();
        metrics_data["rabbitmq_metrics"] = json!({
            "publish_success_count": rabbitmq_metrics.publish_success_count,
            "publish_failure_count": rabbitmq_metrics.publish_failure_count,
            "publish_retry_count": rabbitmq_metrics.publish_retry_count,
            "consume_success_count": rabbitmq_metrics.consume_success_count,
            "consume_failure_count": rabbitmq_metrics.consume_failure_count,
            "connection_error_count": rabbitmq_metrics.connection_error_count,
        });

        if let Ok(length) = client.get_queue_length("ai_tasks").await {
            metrics_data["queue_length"] = json!(length);
        }

        let is_healthy = client.check_health().await;
        metrics_data["rabbitmq_healthy"] = json!(is_healthy);
    }

    HttpResponse::Ok()
        .content_type("application/json")
        .json(metrics_data)
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

    let config = Config::from_env().expect("Failed to load configuration");
    let cpu_count = num_cpus::get();
    info!(
        "Starting server on {}:{} — CPU cores: {}, Actix workers: 2 (production-optimized)",
        config.host, config.port, cpu_count
    );

    let db_connection = database::create_connection(&config.database_url)
        .await
        .expect("Failed to create database connection");

    database::run_migrations(&db_connection)
        .await
        .expect("Failed to run migrations");

    let redis_client = match config.redis_url {
        Some(url) => match RedisClient::new(&url).await {
            Ok(client) => Some(client),
            Err(e) => {
                tracing::warn!("Failed to connect to Redis: {}, proceeding without Redis", e);
                None
            }
        },
        None => None,
    };

    let rabbitmq_client = match messaging::connect_to_rabbitmq_with_retry(&config.rabbitmq_url, 10).await {
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

    let orchestrator: Arc<dyn GameOrchestratorTrait> = Arc::new(GameOrchestrator::new(
        db_connection.clone(),
        redis_client.clone(),
        rabbitmq_client.clone(),
    ));

    let orchestrator_data = web::Data::new(orchestrator);
    let redis_client_data = web::Data::new(redis_client.clone());
    let rabbitmq_client_data = web::Data::new(rabbitmq_client.clone());

    let ws_manager_instance = WebSocketManager::new(redis_client);
    if let Err(e) = ws_manager_instance.start_redis_subscriber().await {
        tracing::warn!("Failed to start Redis subscriber: {}", e);
    }
    use std::time::Duration;
    ws_manager_instance
        .start_connection_cleanup_task(
            Duration::from_secs(5 * 60),
            Duration::from_secs(10 * 60),
        )
        .await;
    let ws_manager = web::Data::new(ws_manager_instance);

    HttpServer::new(move || {
        info!("Registering routes");
        App::new()
            .wrap(CorrelationIdMiddleware)
            .app_data(web::Data::new(db_connection.clone()))
            .app_data(redis_client_data.clone())
            .app_data(rabbitmq_client_data.clone())
            .app_data(ws_manager.clone())
            .app_data(orchestrator_data.clone())
            .service(health_check)
            .service(metrics)
            .service(
                web::scope("/api")
                    .service(get_anonymous_stats)
                    .service(create_quick_game)
                    .service(list_games)
                    .service(get_my_cards)
                    .service(play_card)
                    .service(start_game),
            )
            .service(websocket::scope())
    })
    .bind((config.host.as_str(), config.port))?
    .workers(2)
    .run()
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, web, App};

    #[actix_web::test]
    async fn test_health_check() {
        let app = test::init_service(
            App::new().service(health_check),
        ).await;
        let req = test::TestRequest::get()
            .uri("/health")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body = test::read_body(resp).await;
        assert_eq!(body, "OK");
    }

    #[actix_web::test]
    async fn test_metrics_no_rabbitmq() {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(None::<crate::messaging::RabbitMQClient>))
                .service(metrics),
        ).await;
        let req = test::TestRequest::get()
            .uri("/metrics")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["service"], "jambo-backend");
        assert_eq!(body["rabbitmq_available"], false);
        assert!(body["timestamp"].is_string());
    }
}
