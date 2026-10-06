use crate::api::leaderboard::get_leaderboard;
use crate::auth::extractors::AuthenticatedUser;
use crate::cache::UserCache;
use crate::messaging::RedisClient;
use actix_web::{test, web, App, HttpMessage};
use std::sync::Arc;

fn authenticated_user() -> AuthenticatedUser {
    AuthenticatedUser {
        user_id: uuid::Uuid::new_v4(),
        pseudo: "TestPlayer".to_string(),
    }
}

async fn make_app() -> impl actix_web::dev::Service<
    actix_http::Request,
    Response = actix_web::dev::ServiceResponse,
    Error = actix_web::Error,
> {
    let user_cache = Arc::new(UserCache::new());
    test::init_service(
        App::new()
            .app_data(web::Data::new(user_cache))
            .app_data(web::Data::new(None::<RedisClient>))
            .service(web::resource("/leaderboard").route(web::get().to(get_leaderboard))),
    )
    .await
}

#[actix_web::test]
async fn leaderboard_without_redis_returns_500() {
    let app = make_app().await;
    let user = authenticated_user();
    let req = test::TestRequest::get().uri("/leaderboard").to_request();
    req.extensions_mut().insert(user);

    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 500);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], false);
}
