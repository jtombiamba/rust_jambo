use crate::api::anonymous::get_anonymous_stats;
use actix_web::{test, App};

#[actix_web::test]
async fn test_get_anonymous_stats() {
    let app = test::init_service(App::new().service(get_anonymous_stats)).await;
    let req = test::TestRequest::get().uri("/anonymous").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["games_allowed"], 10);
    assert_eq!(body["games_played"], 0);
    assert_eq!(body["total_wins"], 0);
    assert_eq!(body["credits"], 100);
}
