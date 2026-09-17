use super::*;
use crate::messaging::events::GameStartedPlayer;
use tokio::sync::mpsc;

fn make_manager() -> WebSocketManager {
    WebSocketManager::new(None, None)
}

async fn add_player_connection(
    manager: &WebSocketManager,
    game_id: Uuid,
    player_id: Uuid,
    position: i32,
) -> mpsc::UnboundedReceiver<String> {
    let (tx, rx) = mpsc::unbounded_channel();
    let conn_id = manager
        .add_connection(game_id, tx, CorrelationId::default())
        .await;
    manager
        .set_player_for_connection(game_id, conn_id, player_id, position)
        .await;
    rx
}

/// Add a connection without any player identity (simulates a lobby member whose
/// `join_game` identity has not yet been registered).
async fn add_unidentified_connection(
    manager: &WebSocketManager,
    game_id: Uuid,
) -> mpsc::UnboundedReceiver<String> {
    let (tx, rx) = mpsc::unbounded_channel();
    manager
        .add_connection(game_id, tx, CorrelationId::default())
        .await;
    rx
}

fn make_game_started_players(num: usize) -> Vec<GameStartedPlayer> {
    (0..num)
        .map(|i| GameStartedPlayer {
            id: Uuid::new_v4(),
            name: format!("Player {}", i),
            position: i as i32,
            display_position: i as i32,
            cards_count: 5,
            player_type: "human".to_string(),
        })
        .collect()
}

fn drain_receiver(rx: &mut mpsc::UnboundedReceiver<String>) -> Vec<String> {
    let mut events = Vec::new();
    while let Ok(msg) = rx.try_recv() {
        events.push(msg);
    }
    events
}

#[tokio::test]
async fn test_route_event_broadcasts_card_played() {
    let manager = make_manager();
    let game_id = Uuid::new_v4();
    let player_id = Uuid::new_v4();
    let mut rx = add_player_connection(&manager, game_id, player_id, 0).await;

    let event = GameEvent::CardPlayed {
        game_id,
        player_id,
        card_index: 3,
        next_turn: None,
        correlation_id: None,
    };
    manager.route_event(game_id, event).await;

    let events = drain_receiver(&mut rx);
    assert_eq!(events.len(), 1);
    let parsed: serde_json::Value = serde_json::from_str(&events[0]).unwrap();
    assert_eq!(parsed["type"], "card_played");
    assert_eq!(parsed["card_index"], 3);
}

#[tokio::test]
async fn test_route_event_sends_cards_dealt_to_target_player() {
    let manager = make_manager();
    let game_id = Uuid::new_v4();
    let target_player = Uuid::new_v4();
    let other_player = Uuid::new_v4();

    let mut rx_target = add_player_connection(&manager, game_id, target_player, 0).await;
    let mut rx_other = add_player_connection(&manager, game_id, other_player, 1).await;

    let event = GameEvent::CardsDealt {
        game_id,
        player_id: target_player,
        cards: vec![1, 2, 3, 4, 5],
        special_cards: crate::game::special_cards::SpecialCards {
            check_triple_seven: false,
            check_sum_value_under_21: false,
            check_a_square: false,
        },
    };
    manager.route_event(game_id, event).await;

    let target_events = drain_receiver(&mut rx_target);
    let other_events = drain_receiver(&mut rx_other);

    assert_eq!(target_events.len(), 1);
    let parsed: serde_json::Value = serde_json::from_str(&target_events[0]).unwrap();
    assert_eq!(parsed["type"], "cards_dealt");

    assert!(other_events.is_empty());
}

#[tokio::test]
async fn test_send_game_started_per_player_rotates_display_positions() {
    let manager = make_manager();
    let game_id = Uuid::new_v4();
    let players = make_game_started_players(4);

    let mut receivers: Vec<(Uuid, mpsc::UnboundedReceiver<String>)> = Vec::new();
    for p in &players {
        let rx = add_player_connection(&manager, game_id, p.id, p.position).await;
        receivers.push((p.id, rx));
    }

    let current_turn = players[0].id;
    let event = GameEvent::GameStarted {
        game_id,
        players: players.clone(),
        current_turn,
        game_mode: "multiplayer".to_string(),
        correlation_id: None,
    };
    manager.send_game_started_per_player(game_id, &event).await;

    for (perspective_idx, (_player_id, rx)) in receivers.iter_mut().enumerate() {
        let events = drain_receiver(rx);
        assert_eq!(
            events.len(),
            1,
            "Perspective player {} should receive exactly 1 event",
            perspective_idx
        );

        let parsed: serde_json::Value = serde_json::from_str(&events[0]).unwrap();
        assert_eq!(parsed["type"], "game_started");

        let parsed_players: Vec<serde_json::Value> =
            serde_json::from_value(parsed["players"].clone()).unwrap();
        assert_eq!(parsed_players.len(), 4);

        for p in parsed_players.iter() {
            let absolute_pos = p["position"].as_i64().unwrap() as usize;
            let display_pos = p["display_position"].as_i64().unwrap() as usize;
            let expected_display = (4 + absolute_pos - perspective_idx) % 4;
            assert_eq!(
                display_pos, expected_display,
                "Perspective {}: player at absolute {} has display {}, expected {}",
                perspective_idx, absolute_pos, display_pos, expected_display
            );
        }
    }
}

#[tokio::test]
async fn test_send_game_started_per_player_delivers_to_unidentified_connection() {
    let manager = make_manager();
    let game_id = Uuid::new_v4();
    let players = make_game_started_players(2);

    let mut identified_rx = add_player_connection(&manager, game_id, players[0].id, 0).await;
    let mut unidentified_rx = add_unidentified_connection(&manager, game_id).await;

    let current_turn = players[0].id;
    let event = GameEvent::GameStarted {
        game_id,
        players: players.clone(),
        current_turn,
        game_mode: "multiplayer".to_string(),
        correlation_id: None,
    };
    manager.send_game_started_per_player(game_id, &event).await;

    let identified_events = drain_receiver(&mut identified_rx);
    let unidentified_events = drain_receiver(&mut unidentified_rx);

    // The identified player receives exactly one personalized event.
    assert_eq!(identified_events.len(), 1);
    let identified_parsed: serde_json::Value = serde_json::from_str(&identified_events[0]).unwrap();
    assert_eq!(identified_parsed["type"], "game_started");

    // The unidentified connection also receives a (non-rotated) game_started.
    assert_eq!(unidentified_events.len(), 1);
    let unidentified_parsed: serde_json::Value =
        serde_json::from_str(&unidentified_events[0]).unwrap();
    assert_eq!(unidentified_parsed["type"], "game_started");
}

#[tokio::test]
async fn test_send_game_started_per_player_turn_player_consistent() {
    let manager = make_manager();
    let game_id = Uuid::new_v4();
    let players = make_game_started_players(4);

    let mut receivers: Vec<(Uuid, mpsc::UnboundedReceiver<String>)> = Vec::new();
    for p in &players {
        let rx = add_player_connection(&manager, game_id, p.id, p.position).await;
        receivers.push((p.id, rx));
    }

    let current_turn = players[2].id;
    let event = GameEvent::GameStarted {
        game_id,
        players: players.clone(),
        current_turn,
        game_mode: "multiplayer".to_string(),
        correlation_id: None,
    };
    manager.send_game_started_per_player(game_id, &event).await;

    for (perspective_idx, (_player_id, rx)) in receivers.iter_mut().enumerate() {
        let events = drain_receiver(rx);
        let parsed: serde_json::Value = serde_json::from_str(&events[0]).unwrap();
        assert_eq!(parsed["current_turn"], current_turn.to_string());
        assert_eq!(parsed["correlation_id"], serde_json::Value::Null);

        let parsed_players: Vec<serde_json::Value> =
            serde_json::from_value(parsed["players"].clone()).unwrap();
        assert_eq!(parsed_players.len(), 4);

        for p in &parsed_players {
            let absolute_pos = p["position"].as_i64().unwrap() as usize;
            let display_pos = p["display_position"].as_i64().unwrap() as usize;
            let expected_display = (4 + absolute_pos - perspective_idx) % 4;
            assert_eq!(display_pos, expected_display);
        }
    }
}

#[tokio::test]
async fn test_send_game_started_per_player_preserves_cards_count() {
    let manager = make_manager();
    let game_id = Uuid::new_v4();
    let players: Vec<GameStartedPlayer> = (0..4)
        .map(|i| GameStartedPlayer {
            id: Uuid::new_v4(),
            name: format!("Player {}", i),
            position: i,
            display_position: i,
            cards_count: 5 + i,
            player_type: "human".to_string(),
        })
        .collect();

    let mut receivers: Vec<(Uuid, mpsc::UnboundedReceiver<String>)> = Vec::new();
    for p in &players {
        let rx = add_player_connection(&manager, game_id, p.id, p.position).await;
        receivers.push((p.id, rx));
    }

    let event = GameEvent::GameStarted {
        game_id,
        players: players.clone(),
        current_turn: players[0].id,
        game_mode: "multiplayer".to_string(),
        correlation_id: None,
    };
    manager.send_game_started_per_player(game_id, &event).await;

    for (_player_id, rx) in &mut receivers {
        let events = drain_receiver(rx);
        let parsed: serde_json::Value = serde_json::from_str(&events[0]).unwrap();
        let parsed_players: Vec<serde_json::Value> =
            serde_json::from_value(parsed["players"].clone()).unwrap();

        for (i, p) in parsed_players.iter().enumerate() {
            assert_eq!(p["cards_count"].as_i64().unwrap(), (5 + i as i64));
            assert_eq!(p["position"].as_i64().unwrap(), i as i64);
        }
    }
}

#[tokio::test]
async fn test_send_game_started_per_player_two_players() {
    let manager = make_manager();
    let game_id = Uuid::new_v4();
    let players = make_game_started_players(2);

    let mut receivers: Vec<(Uuid, mpsc::UnboundedReceiver<String>)> = Vec::new();
    for p in &players {
        let rx = add_player_connection(&manager, game_id, p.id, p.position).await;
        receivers.push((p.id, rx));
    }

    let event = GameEvent::GameStarted {
        game_id,
        players: players.clone(),
        current_turn: players[0].id,
        game_mode: "multiplayer".to_string(),
        correlation_id: None,
    };
    manager.send_game_started_per_player(game_id, &event).await;

    for (perspective_idx, (_player_id, rx)) in receivers.iter_mut().enumerate() {
        let events = drain_receiver(rx);
        assert_eq!(events.len(), 1);

        let parsed: serde_json::Value = serde_json::from_str(&events[0]).unwrap();
        let parsed_players: Vec<serde_json::Value> =
            serde_json::from_value(parsed["players"].clone()).unwrap();
        assert_eq!(parsed_players.len(), 2);

        for p in parsed_players.iter() {
            let absolute_pos = p["position"].as_i64().unwrap() as usize;
            let display_pos = p["display_position"].as_i64().unwrap() as usize;
            let expected_display = (2 + absolute_pos - perspective_idx) % 2;
            assert_eq!(display_pos, expected_display);
        }
    }
}

#[tokio::test]
async fn test_send_game_started_per_player_current_turn_preserved() {
    let manager = make_manager();
    let game_id = Uuid::new_v4();
    let players = make_game_started_players(4);

    let mut receivers: Vec<(Uuid, mpsc::UnboundedReceiver<String>)> = Vec::new();
    for p in &players {
        let rx = add_player_connection(&manager, game_id, p.id, p.position).await;
        receivers.push((p.id, rx));
    }

    let current_turn = players[3].id;
    let correlation_id = Uuid::new_v4();
    let event = GameEvent::GameStarted {
        game_id,
        players: players.clone(),
        current_turn,
        game_mode: "multiplayer".to_string(),
        correlation_id: Some(correlation_id),
    };
    manager.send_game_started_per_player(game_id, &event).await;

    for (_player_id, rx) in &mut receivers {
        let events = drain_receiver(rx);
        let parsed: serde_json::Value = serde_json::from_str(&events[0]).unwrap();
        assert_eq!(parsed["current_turn"], current_turn.to_string());
        assert_eq!(parsed["correlation_id"], correlation_id.to_string());
        assert_eq!(parsed["game_id"], game_id.to_string());
        assert_eq!(parsed["type"], "game_started");
    }
}

#[tokio::test]
async fn test_broadcast_to_user_delivers_to_matching_user_only() {
    let manager = make_manager();
    let user_a = Uuid::new_v4();
    let user_b = Uuid::new_v4();

    let (tx_a, mut rx_a) = mpsc::unbounded_channel();
    let (tx_b, mut rx_b) = mpsc::unbounded_channel();
    manager
        .add_user_connection(user_a, tx_a, CorrelationId::default())
        .await;
    manager
        .add_user_connection(user_b, tx_b, CorrelationId::default())
        .await;

    let event = UserEvent::InviteRemoved {
        user_id: user_a,
        game_id: Uuid::new_v4(),
    };
    manager.route_user_event(user_a, event).await;

    let received = drain_receiver(&mut rx_a);
    assert_eq!(received.len(), 1);
    let parsed: serde_json::Value = serde_json::from_str(&received[0]).unwrap();
    assert_eq!(parsed["type"], "invite_removed");

    assert!(drain_receiver(&mut rx_b).is_empty());
}

#[tokio::test]
async fn test_remove_user_connection_stops_delivery() {
    let manager = make_manager();
    let user_id = Uuid::new_v4();

    let (tx, mut rx) = mpsc::unbounded_channel();
    let conn_id = manager
        .add_user_connection(user_id, tx, CorrelationId::default())
        .await;
    manager.remove_user_connection(user_id, conn_id).await;

    let event = UserEvent::InviteReceived {
        user_id,
        invite_id: Uuid::new_v4(),
        game_id: Uuid::new_v4(),
        creator_pseudo: "alice".to_string(),
        bet: 10,
        player_count: 1,
        max_players: 4,
        created_at: "2026-01-01T00:00:00Z".to_string(),
        expires_at: None,
    };
    manager.route_user_event(user_id, event).await;

    assert!(drain_receiver(&mut rx).is_empty());
}

#[tokio::test]
async fn test_spectator_gauge_tracks_join_and_removal() {
    fn spectator_series_present(game_id: &Uuid) -> bool {
        prometheus::gather().iter().any(|family| {
            family.get_name() == "ws_spectators_per_game"
                && family.get_metric().iter().any(|m| {
                    m.get_label().iter().any(|lp| {
                        lp.get_name() == "game_id" && lp.get_value() == game_id.to_string()
                    })
                })
        })
    }

    let manager = make_manager();
    let game_id = Uuid::new_v4();

    let (tx_player, _rx_player) = mpsc::unbounded_channel();
    let player_id = Uuid::new_v4();
    let player_conn = manager
        .add_connection(game_id, tx_player, CorrelationId::default())
        .await;
    manager
        .set_player_for_connection(game_id, player_conn, player_id, 0)
        .await;

    let (tx_spectator, _rx_spectator) = mpsc::unbounded_channel();
    let spectator_conn = manager
        .add_connection(game_id, tx_spectator, CorrelationId::default())
        .await;
    WebSocketManager::mark_spectator_for_latest_connection(&manager, game_id).await;

    let label = game_id.to_string();
    let gauge = crate::observability::metrics::WS_SPECTATORS_PER_GAME
        .get_metric_with_label_values(&[&label])
        .expect("spectator gauge series should exist after marking a spectator");
    assert_eq!(gauge.get(), 1.0);
    assert!(spectator_series_present(&game_id));

    // Removing the non-spectator player leaves the spectator count unchanged.
    manager.remove_connection(game_id, player_conn).await;
    let gauge = crate::observability::metrics::WS_SPECTATORS_PER_GAME
        .get_metric_with_label_values(&[&label])
        .expect("spectator gauge series should still exist while a spectator remains");
    assert_eq!(gauge.get(), 1.0);

    // Removing the last spectator removes the series entirely.
    manager.remove_connection(game_id, spectator_conn).await;
    assert!(
        !spectator_series_present(&game_id),
        "spectator gauge series should be removed when the last spectator leaves"
    );
}
