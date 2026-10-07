use crate::api::config::client_config;
use crate::config::Config;
use actix_web::{test, web, App};

#[actix_web::test]
async fn client_config_returns_config_values() {
    let config = Config::default();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config))
            .service(client_config),
    )
    .await;

    let req = test::TestRequest::get().uri("/config").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert!(body["paypal_donate_url"].is_string());
    assert!(body["bot_thinking_delay_ms"].is_u64());
    assert!(body["cashout_enabled"].is_boolean());
    assert!(body["cashout_min_credits"].is_i64());
}
