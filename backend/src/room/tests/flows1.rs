use uuid::Uuid;

use sea_orm::{DatabaseBackend, MockDatabase, MockExecResult};

use super::helpers::*;
use crate::room::service::RoomService;

#[tokio::test]
async fn create_room_rejects_empty_name() {
    let db = MockDatabase::new(DatabaseBackend::Postgres).into_connection();
    let mailer = make_mailer();
    let config = make_config();
    let service = RoomService::new_with_start_next_game(
        db.clone(),
        mailer.clone(),
        config,
        None,
        make_stub_start_next_game_svc(db),
    );

    let result = service.create_room(Uuid::new_v4(), "").await;
    assert!(result.is_err());

    let result = service.create_room(Uuid::new_v4(), "   ").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn list_user_rooms_empty_when_no_rooms() {
    let db = MockDatabase::new(DatabaseBackend::Postgres)
        .append_query_results(vec![Vec::<crate::database::models::room::Model>::new()])
        .into_connection();
    let mailer = make_mailer();
    let config = make_config();
    let service = RoomService::new_with_start_next_game(
        db.clone(),
        mailer.clone(),
        config,
        None,
        make_stub_start_next_game_svc(db),
    );

    let result = service.list_user_rooms(Uuid::new_v4()).await;
    assert!(result.is_ok());
    let rooms = result.unwrap();
    assert!(rooms.is_empty());
}

#[tokio::test]
async fn join_room_invalid_code_returns_error() {
    let db = MockDatabase::new(DatabaseBackend::Postgres)
        .append_query_results(vec![Vec::<crate::database::models::room::Model>::new()])
        .into_connection();
    let mailer = make_mailer();
    let config = make_config();
    let service = RoomService::new_with_start_next_game(
        db.clone(),
        mailer.clone(),
        config,
        None,
        make_stub_start_next_game_svc(db),
    );

    let result = service.join_room(Uuid::new_v4(), "INVALID").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn join_room_already_member_returns_error() {
    let user_id = Uuid::new_v4();
    let room_id = Uuid::new_v4();
    let room = make_room_model(room_id, user_id, "Room", "CODE1234");
    let member = make_room_member(Uuid::new_v4(), room_id, user_id);

    let db = MockDatabase::new(DatabaseBackend::Postgres)
        .append_query_results(vec![vec![room]])
        .append_query_results(vec![vec![member]])
        .into_connection();
    let mailer = make_mailer();
    let config = make_config();
    let service = RoomService::new_with_start_next_game(
        db.clone(),
        mailer.clone(),
        config,
        None,
        make_stub_start_next_game_svc(db),
    );

    let result = service.join_room(user_id, "CODE1234").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn leave_room_not_member_returns_error() {
    let room_id = Uuid::new_v4();
    let room = make_room_model(room_id, Uuid::new_v4(), "Room", "CODE");

    let db = MockDatabase::new(DatabaseBackend::Postgres)
        .append_query_results(vec![vec![room]])
        .append_query_results(vec![
            Vec::<crate::database::models::room_member::Model>::new(),
        ])
        .into_connection();
    let mailer = make_mailer();
    let config = make_config();
    let service = RoomService::new_with_start_next_game(
        db.clone(),
        mailer.clone(),
        config,
        None,
        make_stub_start_next_game_svc(db),
    );

    let result = service.leave_room(room_id, Uuid::new_v4()).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn leave_room_deletes_if_last_member() {
    let user_id = Uuid::new_v4();
    let room_id = Uuid::new_v4();
    let member_id = Uuid::new_v4();
    let room = make_room_model(room_id, user_id, "Room", "CODE");
    let member = make_room_member(member_id, room_id, user_id);

    let db = MockDatabase::new(DatabaseBackend::Postgres)
        .append_query_results(vec![vec![room]])
        .append_query_results(vec![vec![member]])
        .append_query_results(vec![Vec::<crate::database::models::game_run::Model>::new()])
        .append_exec_results(vec![
            MockExecResult {
                last_insert_id: 1,
                rows_affected: 1,
            },
            MockExecResult {
                last_insert_id: 2,
                rows_affected: 1,
            },
        ])
        .append_query_results(vec![
            Vec::<crate::database::models::room_member::Model>::new(),
        ])
        .append_exec_results(vec![MockExecResult {
            last_insert_id: 3,
            rows_affected: 1,
        }])
        .append_query_results(vec![Vec::<crate::database::models::user::Model>::new()])
        .into_connection();
    let mailer = make_mailer();
    let config = make_config();
    let service = RoomService::new_with_start_next_game(
        db.clone(),
        mailer.clone(),
        config,
        None,
        make_stub_start_next_game_svc(db),
    );

    let result = service.leave_room(room_id, user_id).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn invite_to_room_not_member_returns_error() {
    let room_id = Uuid::new_v4();
    let room = make_room_model(room_id, Uuid::new_v4(), "Room", "CODE");

    let db = MockDatabase::new(DatabaseBackend::Postgres)
        .append_query_results(vec![vec![room]])
        .append_query_results(vec![
            Vec::<crate::database::models::room_member::Model>::new(),
        ])
        .into_connection();
    let mailer = make_mailer();
    let config = make_config();
    let service = RoomService::new_with_start_next_game(
        db.clone(),
        mailer.clone(),
        config,
        None,
        make_stub_start_next_game_svc(db),
    );

    let result = service
        .invite_to_room(room_id, Uuid::new_v4(), "friend@test.com")
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn get_room_detail_room_not_found() {
    let db = MockDatabase::new(DatabaseBackend::Postgres)
        .append_query_results(vec![Vec::<crate::database::models::room::Model>::new()])
        .into_connection();
    let mailer = make_mailer();
    let config = make_config();
    let service = RoomService::new_with_start_next_game(
        db.clone(),
        mailer.clone(),
        config,
        None,
        make_stub_start_next_game_svc(db),
    );

    let result = service
        .get_room_detail(Uuid::new_v4(), Uuid::new_v4())
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn get_room_detail_not_member_returns_error() {
    let room_id = Uuid::new_v4();
    let room = make_room_model(room_id, Uuid::new_v4(), "Room", "CODE");

    let db = MockDatabase::new(DatabaseBackend::Postgres)
        .append_query_results(vec![vec![room]])
        .append_query_results(vec![
            Vec::<crate::database::models::room_member::Model>::new(),
        ])
        .into_connection();
    let mailer = make_mailer();
    let config = make_config();
    let service = RoomService::new_with_start_next_game(
        db.clone(),
        mailer.clone(),
        config,
        None,
        make_stub_start_next_game_svc(db),
    );

    let result = service.get_room_detail(room_id, Uuid::new_v4()).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn get_room_detail_success() {
    let user_id = Uuid::new_v4();
    let room_id = Uuid::new_v4();
    let member_id = Uuid::new_v4();
    let room = make_room_model(room_id, user_id, "Test Room", "INV001");
    let member = make_room_member(member_id, room_id, user_id);
    let user = make_user_model(user_id, "player1");

    let db = MockDatabase::new(DatabaseBackend::Postgres)
        .append_query_results(vec![vec![room]])
        .append_query_results(vec![vec![member.clone()]])
        .append_query_results(vec![vec![member.clone()]])
        .append_query_results(vec![vec![user]])
        .append_query_results(vec![Vec::<crate::database::models::game_run::Model>::new()])
        .into_connection();
    let mailer = make_mailer();
    let config = make_config();
    let service = RoomService::new_with_start_next_game(
        db.clone(),
        mailer.clone(),
        config,
        None,
        make_stub_start_next_game_svc(db),
    );

    let result = service.get_room_detail(room_id, user_id).await;
    assert!(result.is_ok());
    let detail = result.unwrap();
    assert_eq!(detail["id"], serde_json::json!(room_id));
    assert_eq!(detail["name"], serde_json::json!("Test Room"));
    assert_eq!(detail["member_count"], serde_json::json!(1));
}
