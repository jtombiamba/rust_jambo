use uuid::Uuid;

use sea_orm::prelude::DateTimeUtc;
use sea_orm::{DatabaseBackend, MockDatabase};

use super::helpers::*;
use crate::database::models::{game_run, game_run_player, RunStatus};
use crate::room::service::RoomService;

#[tokio::test]
async fn create_run_not_member_returns_error() {
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
        .create_run(
            room_id,
            Uuid::new_v4(),
            3,
            100,
            &[Uuid::new_v4(), Uuid::new_v4()],
        )
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn create_run_requires_positive_num_games() {
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

    let result = service
        .create_run(
            Uuid::new_v4(),
            Uuid::new_v4(),
            0,
            100,
            &[Uuid::new_v4(), Uuid::new_v4()],
        )
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn create_run_requires_positive_bet() {
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

    let result = service
        .create_run(
            Uuid::new_v4(),
            Uuid::new_v4(),
            3,
            0,
            &[Uuid::new_v4(), Uuid::new_v4()],
        )
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn create_run_requires_at_least_two_players() {
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

    let result = service
        .create_run(Uuid::new_v4(), Uuid::new_v4(), 3, 100, &[Uuid::new_v4()])
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn create_run_too_many_players_rejected() {
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

    let player_ids: Vec<Uuid> = (0..10).map(|_| Uuid::new_v4()).collect();
    let result = service
        .create_run(Uuid::new_v4(), Uuid::new_v4(), 3, 100, &player_ids)
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn create_run_run_already_active_returns_error() {
    let user_id = Uuid::new_v4();
    let room_id = Uuid::new_v4();
    let room = make_room_model(room_id, user_id, "Room", "CODE");
    let member = make_room_member(Uuid::new_v4(), room_id, user_id);

    let active_run = game_run::Model {
        id: Uuid::new_v4(),
        room_id,
        num_games: 3,
        bet_per_game: 100,
        num_players: 2,
        current_game_index: 0,
        status: RunStatus::Active,
        created_by: user_id,
        next_game_auto_start_at: None,
        stall_warning_sent_at: None,
        stall_cancelled_at: None,
        created_at: DateTimeUtc::from_timestamp(0, 0).unwrap(),
        updated_at: DateTimeUtc::from_timestamp(0, 0).unwrap(),
    };

    let db = MockDatabase::new(DatabaseBackend::Postgres)
        .append_query_results(vec![vec![room]])
        .append_query_results(vec![vec![member]])
        .append_query_results(vec![vec![active_run]])
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
        .create_run(room_id, user_id, 3, 100, &[user_id, Uuid::new_v4()])
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn create_run_not_all_players_are_members() {
    let user_id = Uuid::new_v4();
    let room_id = Uuid::new_v4();
    let player2 = Uuid::new_v4();
    let room = make_room_model(room_id, user_id, "Room", "CODE");
    let member = make_room_member(Uuid::new_v4(), room_id, user_id);

    let db = MockDatabase::new(DatabaseBackend::Postgres)
        .append_query_results(vec![vec![room]])
        .append_query_results(vec![vec![member.clone()]])
        .append_query_results(vec![Vec::<game_run::Model>::new()])
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

    let result = service
        .create_run(room_id, user_id, 3, 100, &[user_id, player2])
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn list_runs_not_member_returns_error() {
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

    let result = service.list_runs(room_id, Uuid::new_v4()).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn join_run_not_found_returns_error() {
    let db = MockDatabase::new(DatabaseBackend::Postgres)
        .append_query_results(vec![Vec::<game_run::Model>::new()])
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

    let result = service.join_run(Uuid::new_v4(), Uuid::new_v4()).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn leave_run_require_existing_run_player() {
    let run_id = Uuid::new_v4();
    let run = game_run::Model {
        id: run_id,
        room_id: Uuid::new_v4(),
        num_games: 3,
        bet_per_game: 100,
        num_players: 2,
        current_game_index: 1,
        status: RunStatus::Active,
        created_by: Uuid::new_v4(),
        next_game_auto_start_at: None,
        stall_warning_sent_at: None,
        stall_cancelled_at: None,
        created_at: DateTimeUtc::from_timestamp(0, 0).unwrap(),
        updated_at: DateTimeUtc::from_timestamp(0, 0).unwrap(),
    };

    let db = MockDatabase::new(DatabaseBackend::Postgres)
        .append_query_results(vec![vec![run]])
        .append_query_results(vec![Vec::<game_run_player::Model>::new()])
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

    let result = service.leave_run(run_id, Uuid::new_v4()).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn join_run_require_active_status() {
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

    let result = service.join_run(Uuid::new_v4(), Uuid::new_v4()).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn get_active_run_not_member_returns_error() {
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

    let result = service.get_active_run(room_id, Uuid::new_v4()).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn get_current_game_not_found() {
    let run_id = Uuid::new_v4();
    let db = MockDatabase::new(DatabaseBackend::Postgres)
        .append_query_results(vec![Vec::<game_run::Model>::new()])
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

    let result = service.get_current_game(run_id, Uuid::new_v4()).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn start_next_game_not_found() {
    let db = MockDatabase::new(DatabaseBackend::Postgres)
        .append_query_results(vec![Vec::<game_run::Model>::new()])
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
        .start_next_game(Uuid::new_v4(), Uuid::new_v4())
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn check_stalled_runs_returns_zero_on_empty_db() {
    let db = MockDatabase::new(DatabaseBackend::Postgres)
        .append_query_results(vec![Vec::<game_run::Model>::new()])
        .into_connection();
    let mailer = make_mailer();

    let count = RoomService::check_stalled_runs(db, mailer, 1800).await;
    assert_eq!(count, 0);
}
