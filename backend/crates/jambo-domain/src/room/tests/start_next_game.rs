use super::super::start_next_game::assemble_game_creation_plan;
use std::collections::HashMap;
use uuid::Uuid;

use jambo_db::models::{GameRunPlayer, PlayerProfile};

use jambo_db::models::PlayerType;

fn make_run_player(user_id: Uuid) -> GameRunPlayer {
    GameRunPlayer {
        id: Uuid::now_v7(),
        game_run_id: Uuid::nil(),
        user_id,
        position: 0,
        provisioned_credits: 0,
        kicked: false,
        joined_at: chrono::Utc::now(),
    }
}

fn make_profile(user_id: Uuid) -> PlayerProfile {
    PlayerProfile {
        id: Uuid::now_v7(),
        user_id,
        player_type: PlayerType::Human,
        credit: 1000,
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

#[test]
fn assemble_plan_keeps_all_players_and_orders_positions() {
    let players: Vec<GameRunPlayer> = (0..4).map(|_| make_run_player(Uuid::now_v7())).collect();
    let user_ids: Vec<Uuid> = players.iter().map(|p| p.user_id).collect();

    let user_map: HashMap<Uuid, String> = user_ids
        .iter()
        .enumerate()
        .map(|(i, id)| (*id, format!("user{}", i)))
        .collect();
    let profile_map: HashMap<Uuid, PlayerProfile> =
        user_ids.iter().map(|id| (*id, make_profile(*id))).collect();

    let plan = assemble_game_creation_plan(&players, user_map, profile_map, 25);

    assert_eq!(plan.player_ids, user_ids);
    assert_eq!(plan.bet, 25);
    assert_eq!(plan.shuffled_positions.len(), 4);
    assert_eq!(plan.position_map.len(), 4);

    // shuffled_positions is a permutation of 0..3
    let mut sorted_positions = plan.shuffled_positions.clone();
    sorted_positions.sort_unstable();
    assert_eq!(sorted_positions, vec![0, 1, 2, 3]);

    // position_map covers every position with a distinct player
    let mut assigned: Vec<Uuid> = (0..4).map(|i| plan.position_map[&i]).collect();
    assigned.sort();
    let mut expected = user_ids.clone();
    expected.sort();
    assert_eq!(assigned, expected);

    // every player is represented in the run_player_map
    for id in &user_ids {
        assert!(plan.run_player_map.contains_key(id));
    }
}
