use chrono::Utc;
use sea_orm::TransactionTrait;
use uuid::Uuid;

use crate::database::models::{game_card, player, GameStatus};
use crate::error::GameError;
use crate::game::service::card_play::engine;
use crate::game::service::card_play::side_effects::PostCommitContext;
use crate::game::service::card_play::validator;
use crate::game::service::types::RoundEvaluationResult;

fn make_test_game(id: Uuid) -> crate::database::models::game::Model {
    crate::database::models::game::Model {
        id,
        status: GameStatus::Active,
        bet: 10,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        finished_at: None,
        rank: Some(0),
        roll: 1,
        auto: false,
        winner_id: None,
        player_positions: serde_json::json!({}),
        current_winning_card: None,
        current_winning_player_position: None,
        creator_id: None,
        game_mode: crate::database::models::GameMode::Solo,
        max_players: 4,
        invite_expires_at: None,
        stall_warning_sent_at: None,
        game_run_id: None,
        step_by_step: false,
        kicked_players: serde_json::json!([]),
    }
}

fn make_test_player(id: Uuid, game_id: Uuid, position: i32) -> player::Model {
    player::Model {
        id,
        game_id,
        user_id: None,
        name: format!("player_{id}"),
        position,
        credits: 100,
        player_type: crate::database::models::PlayerType::Human,
        kicked: false,
        kicked_at: None,
        created_at: Utc::now(),
    }
}

fn make_test_card(id: Uuid, player_id: Uuid, card_index: i32) -> game_card::Model {
    game_card::Model {
        id,
        player_id: Some(player_id),
        game_id: Uuid::nil(),
        card_index,
        played: false,
        played_at: None,
        round: None,
        created_at: Utc::now(),
    }
}

fn make_played_card(id: Uuid, player_id: Uuid, card_index: i32, round: i32) -> game_card::Model {
    game_card::Model {
        id,
        player_id: Some(player_id),
        game_id: Uuid::nil(),
        card_index,
        played: true,
        played_at: Some(Utc::now()),
        round: Some(round),
        created_at: Utc::now(),
    }
}

// ── engine tests ────────────────────────────────────────────────

#[test]
fn test_compute_winning_card_no_current() {
    assert_eq!(engine::compute_winning_card(None, 5), Some(5));
}

#[test]
fn test_compute_winning_card_same_suit_higher() {
    assert_eq!(engine::compute_winning_card(Some(3), 5), Some(5));
}

#[test]
fn test_compute_winning_card_same_suit_lower() {
    assert_eq!(engine::compute_winning_card(Some(7), 3), Some(7));
}

#[test]
fn test_compute_winning_card_different_suit() {
    assert_eq!(engine::compute_winning_card(Some(3), 12), Some(3));
}

#[test]
fn test_compute_winning_position_no_current() {
    assert_eq!(engine::compute_winning_position(None, None, 5, 2), Some(2));
}

#[test]
fn test_compute_winning_position_same_suit_higher_rank() {
    assert_eq!(
        engine::compute_winning_position(Some(3), Some(1), 7, 2),
        Some(2)
    );
}

#[test]
fn test_compute_winning_position_same_suit_lower_rank() {
    assert_eq!(
        engine::compute_winning_position(Some(7), Some(1), 3, 2),
        Some(1)
    );
}

#[test]
fn test_compute_winning_position_different_suit() {
    assert_eq!(
        engine::compute_winning_position(Some(3), Some(1), 12, 2),
        Some(1)
    );
}

#[test]
fn test_determine_next_player_no_round_result() {
    let players = vec![
        make_test_player(Uuid::now_v7(), Uuid::nil(), 0),
        make_test_player(Uuid::now_v7(), Uuid::nil(), 1),
        make_test_player(Uuid::now_v7(), Uuid::nil(), 2),
    ];
    let next_id = engine::determine_next_player_id(None, &players, 0, 3).unwrap();
    assert_eq!(next_id, players[1].id);
}

#[test]
fn test_determine_next_player_wraps_around() {
    let players = vec![
        make_test_player(Uuid::now_v7(), Uuid::nil(), 0),
        make_test_player(Uuid::now_v7(), Uuid::nil(), 1),
    ];
    let next_id = engine::determine_next_player_id(None, &players, 1, 2).unwrap();
    assert_eq!(next_id, players[0].id);
}

#[test]
fn test_determine_next_player_with_round_result() {
    let players = vec![
        make_test_player(Uuid::now_v7(), Uuid::nil(), 0),
        make_test_player(Uuid::now_v7(), Uuid::nil(), 1),
        make_test_player(Uuid::now_v7(), Uuid::nil(), 2),
    ];
    let winner = players[1].id;
    let result = RoundEvaluationResult {
        round: 1,
        winner_id: winner,
        winner_position: 1,
        game_ended: false,
        final_status: GameStatus::Active,
        players: players.clone(),
    };
    let next_id = engine::determine_next_player_id(Some(&result), &players, 0, 3).unwrap();
    assert_eq!(next_id, winner);
}

#[test]
fn test_determine_next_player_wraps_to_first() {
    let players = vec![make_test_player(Uuid::now_v7(), Uuid::nil(), 0)];
    let result = engine::determine_next_player_id(None, &players, 1, 1);
    assert!(result.is_ok());
}

// ── validator tests ─────────────────────────────────────────────

#[test]
fn test_validate_follows_suit_no_current() {
    let cards = vec![
        make_test_card(Uuid::now_v7(), Uuid::nil(), 0),
        make_test_card(Uuid::now_v7(), Uuid::nil(), 8),
    ];
    assert!(validator::validate_follows_suit(5, None, &cards));
}

#[test]
fn test_validate_follows_suit_same_suit() {
    let cards = vec![
        make_test_card(Uuid::now_v7(), Uuid::nil(), 3),
        make_test_card(Uuid::now_v7(), Uuid::nil(), 8),
    ];
    assert!(validator::validate_follows_suit(3, Some(5), &cards));
}

#[test]
fn test_validate_follows_suit_different_suit_has_match() {
    let cards = vec![
        make_test_card(Uuid::now_v7(), Uuid::nil(), 3),
        make_test_card(Uuid::now_v7(), Uuid::nil(), 8),
    ];
    assert!(!validator::validate_follows_suit(8, Some(5), &cards));
}

#[test]
fn test_validate_follows_suit_different_suit_no_match() {
    let cards = vec![
        make_test_card(Uuid::now_v7(), Uuid::nil(), 3),
        make_test_card(Uuid::now_v7(), Uuid::nil(), 4),
    ];
    assert!(validator::validate_follows_suit(4, Some(8), &cards));
}

#[test]
fn test_validate_follows_suit_played_cards_ignored() {
    let cards = vec![
        make_played_card(Uuid::now_v7(), Uuid::nil(), 3, 1),
        make_test_card(Uuid::now_v7(), Uuid::nil(), 10),
    ];
    assert!(validator::validate_follows_suit(10, Some(5), &cards));
}

#[test]
fn test_validate_follows_suit_all_unplayed_no_match() {
    let cards = vec![
        make_test_card(Uuid::now_v7(), Uuid::nil(), 3),
        make_test_card(Uuid::now_v7(), Uuid::nil(), 4),
        make_test_card(Uuid::now_v7(), Uuid::nil(), 5),
    ];
    assert!(validator::validate_follows_suit(5, Some(8), &cards));
}

#[test]
fn test_validate_follows_suit_must_follow_suit_violation() {
    let cards = vec![
        make_test_card(Uuid::now_v7(), Uuid::nil(), 3),
        make_test_card(Uuid::now_v7(), Uuid::nil(), 4),
        make_test_card(Uuid::now_v7(), Uuid::nil(), 10),
    ];
    assert!(!validator::validate_follows_suit(10, Some(5), &cards));
}

// ── fetch_and_validate_game tests (MockDatabase) ────────────────

#[tokio::test]
async fn test_fetch_game_not_found() {
    let db = sea_orm::MockDatabase::new(sea_orm::DatabaseBackend::Postgres)
        .append_query_results(vec![Vec::<crate::database::models::game::Model>::new()])
        .into_connection();
    let txn = db.begin().await.unwrap();

    let result = validator::fetch_and_validate_game(&txn, Uuid::now_v7()).await;
    assert!(matches!(result, Err(GameError::GameNotFound)));
}

#[tokio::test]
async fn test_fetch_game_finished() {
    let game_id = Uuid::now_v7();
    let mut game = make_test_game(game_id);
    game.status = GameStatus::Finished;

    let db = sea_orm::MockDatabase::new(sea_orm::DatabaseBackend::Postgres)
        .append_query_results(vec![vec![game]])
        .into_connection();
    let txn = db.begin().await.unwrap();

    let result = validator::fetch_and_validate_game(&txn, game_id).await;
    assert!(matches!(result, Err(GameError::GameFinished)));
}

#[tokio::test]
async fn test_fetch_game_kora() {
    let game_id = Uuid::now_v7();
    let mut game = make_test_game(game_id);
    game.status = GameStatus::Kora;

    let db = sea_orm::MockDatabase::new(sea_orm::DatabaseBackend::Postgres)
        .append_query_results(vec![vec![game]])
        .into_connection();
    let txn = db.begin().await.unwrap();

    let result = validator::fetch_and_validate_game(&txn, game_id).await;
    assert!(matches!(result, Err(GameError::GameFinished)));
}

#[tokio::test]
async fn test_fetch_game_double_kora() {
    let game_id = Uuid::now_v7();
    let mut game = make_test_game(game_id);
    game.status = GameStatus::DoubleKora;

    let db = sea_orm::MockDatabase::new(sea_orm::DatabaseBackend::Postgres)
        .append_query_results(vec![vec![game]])
        .into_connection();
    let txn = db.begin().await.unwrap();

    let result = validator::fetch_and_validate_game(&txn, game_id).await;
    assert!(matches!(result, Err(GameError::GameFinished)));
}

#[tokio::test]
async fn test_fetch_game_active() {
    let game_id = Uuid::now_v7();
    let game = make_test_game(game_id);

    let db = sea_orm::MockDatabase::new(sea_orm::DatabaseBackend::Postgres)
        .append_query_results(vec![vec![game.clone()]])
        .into_connection();
    let txn = db.begin().await.unwrap();

    let result = validator::fetch_and_validate_game(&txn, game_id)
        .await
        .unwrap();
    assert_eq!(result.id, game_id);
}

// ── fetch_and_validate_turn tests ───────────────────────────────

#[tokio::test]
async fn test_fetch_turn_player_not_found() {
    let db = sea_orm::MockDatabase::new(sea_orm::DatabaseBackend::Postgres)
        .append_query_results(vec![Vec::<player::Model>::new()])
        .into_connection();
    let txn = db.begin().await.unwrap();

    let result = validator::fetch_and_validate_turn(&txn, Uuid::now_v7(), Uuid::now_v7()).await;
    assert!(matches!(result, Err(GameError::PlayerNotFound)));
}

#[tokio::test]
async fn test_fetch_turn_player_found() {
    let player_id = Uuid::now_v7();
    let game_id = Uuid::now_v7();
    let players = vec![
        make_test_player(player_id, game_id, 0),
        make_test_player(Uuid::now_v7(), game_id, 1),
    ];

    let db = sea_orm::MockDatabase::new(sea_orm::DatabaseBackend::Postgres)
        .append_query_results(vec![players.clone()])
        .into_connection();
    let txn = db.begin().await.unwrap();

    let (result_players, position, _active_count) =
        validator::fetch_and_validate_turn(&txn, game_id, player_id)
            .await
            .unwrap();
    assert_eq!(result_players.len(), 2);
    assert_eq!(position, 0);
}

#[tokio::test]
async fn test_fetch_turn_active_count_excludes_kicked() {
    let game_id = Uuid::now_v7();
    let mut kicked_player = make_test_player(Uuid::now_v7(), game_id, 2);
    kicked_player.kicked = true;
    let players = vec![
        make_test_player(Uuid::now_v7(), game_id, 0),
        make_test_player(Uuid::now_v7(), game_id, 1),
        kicked_player,
    ];

    let db = sea_orm::MockDatabase::new(sea_orm::DatabaseBackend::Postgres)
        .append_query_results(vec![players.clone()])
        .into_connection();
    let txn = db.begin().await.unwrap();

    let (_result_players, _position, active_count) =
        validator::fetch_and_validate_turn(&txn, game_id, players[0].id)
            .await
            .unwrap();
    assert_eq!(active_count, 2);
}

// ── fetch_and_validate_card tests ───────────────────────────────

#[tokio::test]
async fn test_fetch_card_not_found() {
    let db = sea_orm::MockDatabase::new(sea_orm::DatabaseBackend::Postgres)
        .append_query_results(vec![Vec::<game_card::Model>::new()])
        .into_connection();
    let txn = db.begin().await.unwrap();

    let result = validator::fetch_and_validate_card(&txn, Uuid::now_v7(), 0).await;
    assert!(matches!(result, Err(GameError::CardNotFound)));
}

#[tokio::test]
async fn test_fetch_card_already_played() {
    let player_id = Uuid::now_v7();
    let cards = vec![make_played_card(Uuid::now_v7(), player_id, 0, 1)];

    let db = sea_orm::MockDatabase::new(sea_orm::DatabaseBackend::Postgres)
        .append_query_results(vec![cards])
        .into_connection();
    let txn = db.begin().await.unwrap();

    let result = validator::fetch_and_validate_card(&txn, player_id, 0).await;
    assert!(matches!(result, Err(GameError::CardNotFound)));
}

#[tokio::test]
async fn test_fetch_card_found() {
    let player_id = Uuid::now_v7();
    let card = make_test_card(Uuid::now_v7(), player_id, 5);
    let cards = vec![card.clone()];

    let db = sea_orm::MockDatabase::new(sea_orm::DatabaseBackend::Postgres)
        .append_query_results(vec![cards.clone()])
        .into_connection();
    let txn = db.begin().await.unwrap();

    let (result_card, all_cards) = validator::fetch_and_validate_card(&txn, player_id, 5)
        .await
        .unwrap();
    assert_eq!(result_card.card_index, 5);
    assert_eq!(all_cards.len(), 1);
}

#[tokio::test]
async fn test_fetch_card_finds_correct_index() {
    let player_id = Uuid::now_v7();
    let cards = vec![
        make_test_card(Uuid::now_v7(), player_id, 0),
        make_test_card(Uuid::now_v7(), player_id, 5),
        make_test_card(Uuid::now_v7(), player_id, 10),
    ];

    let db = sea_orm::MockDatabase::new(sea_orm::DatabaseBackend::Postgres)
        .append_query_results(vec![cards])
        .into_connection();
    let txn = db.begin().await.unwrap();

    let (result_card, _) = validator::fetch_and_validate_card(&txn, player_id, 5)
        .await
        .unwrap();
    assert_eq!(result_card.card_index, 5);
}

// ── PostCommitContext tests ─────────────────────────────────────

#[tokio::test]
async fn test_post_commit_context_handle_no_redis() {
    let db = sea_orm::MockDatabase::new(sea_orm::DatabaseBackend::Postgres).into_connection();
    let svc = crate::game::service::GameService::new(db);

    let ctx = PostCommitContext {
        game_id: Uuid::now_v7(),
        player_id: Uuid::now_v7(),
        card_index: 0,
        next_player_id: Uuid::now_v7(),
        players: vec![],
        game_ended: false,
        round_result: None,
        correlation_id: None,
    };
    ctx.handle(&svc).await;
}
