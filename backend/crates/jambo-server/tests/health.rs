#[actix_web::test]
async fn test_health_check() {
    let app = actix_web::test::init_service(
        actix_web::App::new().service(jambo_api::api::system::health_check),
    )
    .await;
    let req = actix_web::test::TestRequest::get()
        .uri("/health")
        .to_request();
    let resp = actix_web::test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body = actix_web::test::read_body(resp).await;
    assert_eq!(body, "OK");
}

#[actix_web::test]
async fn test_metrics() {
    let app = actix_web::test::init_service(
        actix_web::App::new().service(jambo_api::api::system::metrics),
    )
    .await;
    let req = actix_web::test::TestRequest::get()
        .uri("/metrics")
        .to_request();
    let resp = actix_web::test::call_service(&app, req).await;
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
    jambo_core::observability::metrics_init::init_all();
    let app = actix_web::test::init_service(
        actix_web::App::new().service(jambo_api::api::system::metrics),
    )
    .await;
    let req = actix_web::test::TestRequest::get()
        .uri("/metrics")
        .to_request();
    let resp = actix_web::test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body = actix_web::test::read_body(resp).await;
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
    assert!(
        body_str.contains("ws_send_failed_total"),
        "Expected ws_send_failed_total in metrics, got: {body_str}"
    );
    assert!(
        body_str.contains("ws_messages_dropped_total"),
        "Expected ws_messages_dropped_total in metrics, got: {body_str}"
    );
    assert!(
        body_str.contains("ws_slow_consumer_disconnects_total"),
        "Expected ws_slow_consumer_disconnects_total in metrics, got: {body_str}"
    );
}
