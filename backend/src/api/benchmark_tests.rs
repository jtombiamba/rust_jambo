#[cfg(test)]
mod tests {
    use crate::api::benchmark::{cleanup_benchmark_data, create_benchmark_game};
    use crate::config::Config;
    use crate::error::GameError;
    use crate::game::service::mock::MockGameService;
    use actix_web::{test, web, App};
    use std::sync::Arc;
    use uuid::Uuid;

    fn test_config() -> Config {
        let mut config = Config::default();
        config.benchmark_mode = true;
        config.benchmark_api_token = "test-token".to_string();
        config
    }

    async fn make_create_app(
        mock: Arc<dyn crate::game::service::BenchmarkService>,
    ) -> impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    > {
        test::init_service(
            App::new()
                .app_data(web::Data::new(mock))
                .app_data(web::Data::new(test_config()))
                .service(
                    web::resource("/benchmark/create-multiplayer-game")
                        .route(web::post().to(create_benchmark_game)),
                ),
        )
        .await
    }

    async fn make_cleanup_app(
        mock: Arc<dyn crate::game::service::BenchmarkService>,
    ) -> impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    > {
        test::init_service(
            App::new()
                .app_data(web::Data::new(mock))
                .app_data(web::Data::new(test_config()))
                .service(
                    web::resource("/benchmark/cleanup")
                        .route(web::post().to(cleanup_benchmark_data)),
                ),
        )
        .await
    }

    #[actix_web::test]
    async fn create_benchmark_game_success() {
        let mock = Arc::new(MockGameService::ok());
        let app = make_create_app(mock).await;
        let player_id = Uuid::new_v4();
        let req = test::TestRequest::post()
            .uri("/benchmark/create-multiplayer-game")
            .insert_header(("X-Benchmark-Token", "test-token"))
            .set_json(serde_json::json!({ "user_ids": [player_id], "bet": 10 }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
    }

    #[actix_web::test]
    async fn create_benchmark_game_unauthorized() {
        let mock = Arc::new(MockGameService::ok());
        let app = make_create_app(mock).await;
        let player_id = Uuid::new_v4();
        let req = test::TestRequest::post()
            .uri("/benchmark/create-multiplayer-game")
            .insert_header(("X-Benchmark-Token", "wrong-token"))
            .set_json(serde_json::json!({ "user_ids": [player_id], "bet": 10 }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 401);
    }

    #[actix_web::test]
    async fn create_benchmark_game_missing_token() {
        let mock = Arc::new(MockGameService::ok());
        let app = make_create_app(mock).await;
        let player_id = Uuid::new_v4();
        let req = test::TestRequest::post()
            .uri("/benchmark/create-multiplayer-game")
            .set_json(serde_json::json!({ "user_ids": [player_id], "bet": 10 }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 401);
    }

    #[actix_web::test]
    async fn create_benchmark_game_with_default_bet() {
        let mock = Arc::new(MockGameService::ok());
        let app = make_create_app(mock).await;
        let player_id = Uuid::new_v4();
        let req = test::TestRequest::post()
            .uri("/benchmark/create-multiplayer-game")
            .insert_header(("X-Benchmark-Token", "test-token"))
            .set_json(serde_json::json!({ "user_ids": [player_id] }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
    }

    #[actix_web::test]
    async fn create_benchmark_game_internal_error() {
        let mock = Arc::new(MockGameService::ok());
        mock.set_create_benchmark_result(Err(GameError::internal("benchmark creation failed")));
        let app = make_create_app(mock).await;
        let player_id = Uuid::new_v4();
        let req = test::TestRequest::post()
            .uri("/benchmark/create-multiplayer-game")
            .insert_header(("X-Benchmark-Token", "test-token"))
            .set_json(serde_json::json!({ "user_ids": [player_id], "bet": 10 }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_server_error());
    }

    #[actix_web::test]
    async fn cleanup_benchmark_data_success() {
        let mock = Arc::new(MockGameService::ok());
        let app = make_cleanup_app(mock).await;
        let req = test::TestRequest::post()
            .uri("/benchmark/cleanup")
            .insert_header(("X-Benchmark-Token", "test-token"))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["success"], true);
    }

    #[actix_web::test]
    async fn cleanup_benchmark_data_unauthorized() {
        let mock = Arc::new(MockGameService::ok());
        let app = make_cleanup_app(mock).await;
        let req = test::TestRequest::post()
            .uri("/benchmark/cleanup")
            .insert_header(("X-Benchmark-Token", "wrong"))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 401);
    }
}
