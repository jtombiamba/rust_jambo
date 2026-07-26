use super::compute_display_position;
use super::is_unique_violation;
use super::GameService;
use sea_orm::{DatabaseBackend, DbErr, MockDatabase};
use uuid::Uuid;

fn make_service_without_redis() -> GameService {
    let db = MockDatabase::new(DatabaseBackend::Postgres).into_connection();
    GameService::new(db)
}

#[tokio::test]
async fn test_invalidate_dashboard_caches_no_redis() {
    let service = make_service_without_redis();
    let user_ids = vec![Uuid::new_v4()];
    service.invalidate_dashboard_caches(&user_ids).await;
}

#[tokio::test]
async fn test_invalidate_dashboard_caches_empty_user_ids() {
    let service = make_service_without_redis();
    let user_ids: Vec<Uuid> = vec![];
    service.invalidate_dashboard_caches(&user_ids).await;
}

#[tokio::test]
async fn test_user_id_collection_filters_bots() {
    let user_ids: Vec<Uuid> = vec![Some(Uuid::new_v4()), None, Some(Uuid::new_v4()), None]
        .into_iter()
        .flatten()
        .collect();
    assert_eq!(user_ids.len(), 2);
}

#[test]
fn test_display_position_self_is_zero() {
    assert_eq!(compute_display_position(0, 4, 0), 0);
    assert_eq!(compute_display_position(1, 4, 1), 0);
    assert_eq!(compute_display_position(2, 4, 2), 0);
    assert_eq!(compute_display_position(3, 4, 3), 0);
    assert_eq!(compute_display_position(0, 2, 0), 0);
    assert_eq!(compute_display_position(1, 2, 1), 0);
    assert_eq!(compute_display_position(0, 3, 0), 0);
    assert_eq!(compute_display_position(1, 3, 1), 0);
    assert_eq!(compute_display_position(2, 3, 2), 0);
}

#[test]
fn test_display_position_rotation_four_players() {
    assert_eq!(compute_display_position(0, 4, 0), 0);
    assert_eq!(compute_display_position(1, 4, 0), 1);
    assert_eq!(compute_display_position(2, 4, 0), 2);
    assert_eq!(compute_display_position(3, 4, 0), 3);

    assert_eq!(compute_display_position(0, 4, 1), 3);
    assert_eq!(compute_display_position(1, 4, 1), 0);
    assert_eq!(compute_display_position(2, 4, 1), 1);
    assert_eq!(compute_display_position(3, 4, 1), 2);

    assert_eq!(compute_display_position(0, 4, 2), 2);
    assert_eq!(compute_display_position(1, 4, 2), 3);
    assert_eq!(compute_display_position(2, 4, 2), 0);
    assert_eq!(compute_display_position(3, 4, 2), 1);

    assert_eq!(compute_display_position(0, 4, 3), 1);
    assert_eq!(compute_display_position(1, 4, 3), 2);
    assert_eq!(compute_display_position(2, 4, 3), 3);
    assert_eq!(compute_display_position(3, 4, 3), 0);
}

#[test]
fn test_display_position_rotation_two_players() {
    assert_eq!(compute_display_position(0, 2, 0), 0);
    assert_eq!(compute_display_position(1, 2, 0), 1);

    assert_eq!(compute_display_position(0, 2, 1), 1);
    assert_eq!(compute_display_position(1, 2, 1), 0);
}

#[test]
fn test_display_position_rotation_three_players() {
    assert_eq!(compute_display_position(0, 3, 0), 0);
    assert_eq!(compute_display_position(1, 3, 0), 1);
    assert_eq!(compute_display_position(2, 3, 0), 2);

    assert_eq!(compute_display_position(0, 3, 1), 2);
    assert_eq!(compute_display_position(1, 3, 1), 0);
    assert_eq!(compute_display_position(2, 3, 1), 1);

    assert_eq!(compute_display_position(0, 3, 2), 1);
    assert_eq!(compute_display_position(1, 3, 2), 2);
    assert_eq!(compute_display_position(2, 3, 2), 0);
}

#[test]
fn test_display_position_single_player() {
    assert_eq!(compute_display_position(0, 1, 0), 0);
}

#[test]
fn test_is_unique_violation_postgres() {
    let err = DbErr::Exec(sea_orm::RuntimeErr::Internal(
        "duplicate key value violates unique constraint \"23505\"".to_string(),
    ));
    assert!(is_unique_violation(&err));
}

#[test]
fn test_is_unique_violation_other_db_error() {
    let err = DbErr::RecordNotFound("not found".to_string());
    assert!(!is_unique_violation(&err));
}

#[test]
fn test_is_unique_violation_non_exec_error() {
    let err = DbErr::Custom("custom error".to_string());
    assert!(!is_unique_violation(&err));
}
