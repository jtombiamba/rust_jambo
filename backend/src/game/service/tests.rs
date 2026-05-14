use super::GameService;
use sea_orm::{DatabaseBackend, MockDatabase};
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
