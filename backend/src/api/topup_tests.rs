use super::*;
use crate::database::models::{PlayerProfile, PlayerType};
use sea_orm::{DatabaseBackend, MockDatabase, MockExecResult};

fn make_profile(user_id: Uuid, credit: i32) -> PlayerProfile {
    PlayerProfile {
        id: Uuid::now_v7(),
        user_id,
        player_type: PlayerType::Human,
        credit,
        game_played: 0,
        wins: 0,
        kora_wins: 0,
        winning_streak: 0,
        latitude: None,
        longitude: None,
        country_code: None,
        city: None,
        frozen_until: None,
        cashout_locked: false,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

#[tokio::test]
async fn finalize_topup_records_transaction_and_credits() {
    let user_id = Uuid::now_v7();
    let db = MockDatabase::new(DatabaseBackend::Postgres)
        .append_exec_results(vec![
            MockExecResult {
                last_insert_id: 0,
                rows_affected: 1,
            },
            MockExecResult {
                last_insert_id: 0,
                rows_affected: 1,
            },
        ])
        .append_query_results(vec![
            vec![make_profile(user_id, 100)],
            vec![make_profile(user_id, 350)],
        ])
        .into_connection();

    let credit = finalize_topup(
        &web::Data::new(db),
        None,
        "topup_capture:key",
        user_id,
        "ORDER_1",
        250,
        100,
    )
    .await
    .expect("finalize should succeed");

    assert_eq!(credit, 350);
}

#[tokio::test]
async fn finalize_topup_skips_credit_when_order_already_recorded() {
    let user_id = Uuid::now_v7();
    let db = MockDatabase::new(DatabaseBackend::Postgres)
        .append_exec_results(vec![MockExecResult {
            last_insert_id: 0,
            rows_affected: 0,
        }])
        .append_query_results(vec![vec![make_profile(user_id, 100)]])
        .into_connection();

    let credit = finalize_topup(
        &web::Data::new(db),
        None,
        "topup_capture:key",
        user_id,
        "ORDER_1",
        250,
        100,
    )
    .await
    .expect("finalize should succeed");

    assert_eq!(credit, 100);
}

#[tokio::test]
async fn finalize_topup_returns_not_found_when_profile_missing() {
    let user_id = Uuid::now_v7();
    let db = MockDatabase::new(DatabaseBackend::Postgres)
        .append_exec_results(vec![MockExecResult {
            last_insert_id: 0,
            rows_affected: 1,
        }])
        .append_query_results(vec![Vec::<PlayerProfile>::new()])
        .into_connection();

    let result = finalize_topup(
        &web::Data::new(db),
        None,
        "topup_capture:key",
        user_id,
        "ORDER_1",
        250,
        100,
    )
    .await;

    assert!(matches!(result, Err(AppError::NotFound(_))));
}
