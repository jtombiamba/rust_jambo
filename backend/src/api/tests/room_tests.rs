use crate::api::room::{
    create_room, create_run, get_active_run, get_current_game, get_room, invite_to_room, join_room,
    join_run, leave_room, leave_run, list_rooms, list_runs, start_next_game, to_room_created,
};
use crate::auth::extractors::AuthenticatedUser;
use crate::room::error::RoomServiceError;
use crate::room::service_trait::RoomServiceTrait;
use crate::test_helpers::MockRoomService;
use actix_web::{test, web, App, HttpMessage};
use chrono::Utc;
use std::sync::Arc;
use uuid::Uuid;

fn authenticated_user() -> AuthenticatedUser {
    AuthenticatedUser {
        user_id: Uuid::new_v4(),
        pseudo: "TestPlayer".to_string(),
    }
}

async fn make_room_app(
    mock: Arc<dyn RoomServiceTrait>,
) -> impl actix_web::dev::Service<
    actix_http::Request,
    Response = actix_web::dev::ServiceResponse,
    Error = actix_web::Error,
> {
    test::init_service(
        App::new()
            .app_data(web::Data::new(mock))
            .service(
                web::resource("/rooms")
                    .route(web::post().to(create_room))
                    .route(web::get().to(list_rooms)),
            )
            .service(web::resource("/rooms/join").route(web::post().to(join_room)))
            .service(web::resource("/rooms/{room_id}").route(web::get().to(get_room)))
            .service(web::resource("/rooms/{room_id}/invite").route(web::post().to(invite_to_room)))
            .service(web::resource("/rooms/{room_id}/leave").route(web::post().to(leave_room)))
            .service(
                web::resource("/rooms/{room_id}/runs")
                    .route(web::post().to(create_run))
                    .route(web::get().to(list_runs)),
            )
            .service(
                web::resource("/rooms/{room_id}/runs/active").route(web::get().to(get_active_run)),
            )
            .service(web::resource("/runs/{run_id}/join").route(web::post().to(join_run)))
            .service(web::resource("/runs/{run_id}/leave").route(web::post().to(leave_run)))
            .service(
                web::resource("/runs/{run_id}/next-game").route(web::post().to(start_next_game)),
            )
            .service(
                web::resource("/runs/{run_id}/current-game").route(web::get().to(get_current_game)),
            ),
    )
    .await
}

#[actix_web::test]
async fn to_room_created_maps_entity_to_response() {
    let id = Uuid::new_v4();
    let creator_id = Uuid::new_v4();
    let now = Utc::now();

    let room = crate::database::models::room::Model {
        id,
        name: "Test Room".to_string(),
        creator_id,
        invitation_code: "ABC123".to_string(),
        created_at: now,
        updated_at: now,
    };

    let response = to_room_created(room);
    assert_eq!(response.id, id);
    assert_eq!(response.name, "Test Room");
    assert_eq!(response.creator_id, creator_id);
    assert_eq!(response.invitation_code, "ABC123");
    assert_eq!(response.created_at, now);
    assert_eq!(response.updated_at, now);
}

#[actix_web::test]
async fn create_room_success() {
    let app = make_room_app(Arc::new(MockRoomService::new())).await;
    let user = authenticated_user();
    let req = test::TestRequest::post()
        .uri("/rooms")
        .set_json(serde_json::json!({ "name": "My Room" }))
        .to_request();
    req.extensions_mut().insert(user);
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert!(body["id"].is_string());
}

#[actix_web::test]
async fn list_rooms_success() {
    let app = make_room_app(Arc::new(MockRoomService::new())).await;
    let user = authenticated_user();
    let req = test::TestRequest::get().uri("/rooms").to_request();
    req.extensions_mut().insert(user);
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn get_room_success() {
    let app = make_room_app(Arc::new(MockRoomService::new())).await;
    let user = authenticated_user();
    let room_id = Uuid::new_v4();
    let req = test::TestRequest::get()
        .uri(&format!("/rooms/{room_id}"))
        .to_request();
    req.extensions_mut().insert(user);
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn get_room_not_found_returns_error() {
    let mock = Arc::new(MockRoomService::new());
    mock.set_get_room_detail_result(Err(RoomServiceError::RoomNotFound));
    let app = make_room_app(mock).await;
    let user = authenticated_user();
    let room_id = Uuid::new_v4();
    let req = test::TestRequest::get()
        .uri(&format!("/rooms/{room_id}"))
        .to_request();
    req.extensions_mut().insert(user);
    let resp = test::call_service(&app, req).await;
    assert!(!resp.status().is_success());
}

#[actix_web::test]
async fn join_room_success() {
    let app = make_room_app(Arc::new(MockRoomService::new())).await;
    let user = authenticated_user();
    let req = test::TestRequest::post()
        .uri("/rooms/join")
        .set_json(serde_json::json!({ "invitation_code": "ABC123" }))
        .to_request();
    req.extensions_mut().insert(user);
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn invite_to_room_success() {
    let app = make_room_app(Arc::new(MockRoomService::new())).await;
    let user = authenticated_user();
    let room_id = Uuid::new_v4();
    let req = test::TestRequest::post()
        .uri(&format!("/rooms/{room_id}/invite"))
        .set_json(serde_json::json!({ "email": "a@b.co" }))
        .to_request();
    req.extensions_mut().insert(user);
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn leave_room_success() {
    let app = make_room_app(Arc::new(MockRoomService::new())).await;
    let user = authenticated_user();
    let room_id = Uuid::new_v4();
    let req = test::TestRequest::post()
        .uri(&format!("/rooms/{room_id}/leave"))
        .to_request();
    req.extensions_mut().insert(user);
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn create_run_success() {
    let app = make_room_app(Arc::new(MockRoomService::new())).await;
    let user = authenticated_user();
    let room_id = Uuid::new_v4();
    let req = test::TestRequest::post()
        .uri(&format!("/rooms/{room_id}/runs"))
        .set_json(serde_json::json!({
            "num_games": 1,
            "bet": 10,
            "player_ids": []
        }))
        .to_request();
    req.extensions_mut().insert(user);
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
}

#[actix_web::test]
async fn list_runs_success() {
    let app = make_room_app(Arc::new(MockRoomService::new())).await;
    let user = authenticated_user();
    let room_id = Uuid::new_v4();
    let req = test::TestRequest::get()
        .uri(&format!("/rooms/{room_id}/runs"))
        .to_request();
    req.extensions_mut().insert(user);
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn get_active_run_success() {
    let app = make_room_app(Arc::new(MockRoomService::new())).await;
    let user = authenticated_user();
    let room_id = Uuid::new_v4();
    let req = test::TestRequest::get()
        .uri(&format!("/rooms/{room_id}/runs/active"))
        .to_request();
    req.extensions_mut().insert(user);
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn join_run_success() {
    let app = make_room_app(Arc::new(MockRoomService::new())).await;
    let user = authenticated_user();
    let run_id = Uuid::new_v4();
    let req = test::TestRequest::post()
        .uri(&format!("/runs/{run_id}/join"))
        .to_request();
    req.extensions_mut().insert(user);
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn leave_run_success() {
    let app = make_room_app(Arc::new(MockRoomService::new())).await;
    let user = authenticated_user();
    let run_id = Uuid::new_v4();
    let req = test::TestRequest::post()
        .uri(&format!("/runs/{run_id}/leave"))
        .to_request();
    req.extensions_mut().insert(user);
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn start_next_game_success() {
    let app = make_room_app(Arc::new(MockRoomService::new())).await;
    let user = authenticated_user();
    let run_id = Uuid::new_v4();
    let req = test::TestRequest::post()
        .uri(&format!("/runs/{run_id}/next-game"))
        .to_request();
    req.extensions_mut().insert(user);
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn get_current_game_success() {
    let app = make_room_app(Arc::new(MockRoomService::new())).await;
    let user = authenticated_user();
    let run_id = Uuid::new_v4();
    let req = test::TestRequest::get()
        .uri(&format!("/runs/{run_id}/current-game"))
        .to_request();
    req.extensions_mut().insert(user);
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}
