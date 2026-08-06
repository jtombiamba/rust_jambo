use std::collections::HashMap;
use tracing::{error, info};
use uuid::Uuid;

use crate::database::models::PlayerType;
use crate::database::repositories::{GameCardRepository, GameRepository, PlayerRepository};
use crate::game::constants::CARDS_PER_PLAYER;
use crate::game::service::compute_display_position;

use super::manager::WebSocketManager;
use super::messages::{GameStatePlayer, OutgoingMessage};

pub(super) async fn send_game_state_snapshot(
    manager: &WebSocketManager,
    db: &sea_orm::DatabaseConnection,
    game_id: Uuid,
    player_id: Uuid,
    player_position: i32,
) {
    let game_repo = GameRepository::new(db.clone());
    let player_repo = PlayerRepository::new(db.clone());
    let game_card_repo = GameCardRepository::new(db.clone());

    let game_model = match game_repo.find_by_id(game_id).await {
        Ok(Some(g)) => g,
        Ok(None) => {
            tracing::warn!("Game {} not found for state snapshot", game_id);
            let error_msg = OutgoingMessage::Error {
                message: "Game not found".to_string(),
                source: "ws:game_not_found".to_string(),
            };
            if let Ok(json) = serde_json::to_string(&error_msg) {
                manager.send_to_player(game_id, player_id, &json).await;
            }
            return;
        }
        Err(e) => {
            tracing::error!("DB error fetching game {} for snapshot: {}", game_id, e);
            let error_msg = OutgoingMessage::Error {
                message: "Failed to load game state".to_string(),
                source: "ws:db_error".to_string(),
            };
            if let Ok(json) = serde_json::to_string(&error_msg) {
                manager.send_to_player(game_id, player_id, &json).await;
            }
            return;
        }
    };

    let players = match player_repo.list_by_game(game_id).await {
        Ok(p) => p,
        Err(e) => {
            error!("Failed to fetch players for game {}: {}", game_id, e);
            return;
        }
    };

    let num_players = players.len();
    let my_pos = player_position as usize;

    let played_counts: HashMap<Uuid, usize> = count_played_cards_per_player(db, game_id).await;

    let game_state_players: Vec<GameStatePlayer> = players
        .iter()
        .map(|p| {
            let display_pos = compute_display_position(p.position as usize, num_players, my_pos);
            let player_type_str = match p.player_type {
                PlayerType::Human => "human",
                PlayerType::Bot => "bot",
            };
            let cards_count =
                CARDS_PER_PLAYER as i32 - *played_counts.get(&p.id).unwrap_or(&0) as i32;
            GameStatePlayer {
                id: p.id,
                name: p.name.clone(),
                position: p.position,
                display_position: display_pos as i32,
                player_type: player_type_str.to_string(),
                cards_count,
            }
        })
        .collect();

    let played_cards: Vec<Option<i32>> = match game_card_repo
        .list_by_game_and_round(game_id, game_model.roll)
        .await
    {
        Ok(cards) => {
            let mut played_pairs: Vec<(i32, usize)> = Vec::new();
            let winner_pos = game_model.current_winning_player_position.unwrap_or(0) as usize;

            for card in cards {
                if let Some(pid) = card.player_id {
                    if let Some(pos) = players.iter().position(|p| p.id == pid) {
                        played_pairs.push((card.card_index, pos));
                    }
                }
            }
            played_pairs.sort_by_key(|(_, pos)| (num_players + *pos - winner_pos) % num_players);
            let mut slots: Vec<Option<i32>> = played_pairs
                .into_iter()
                .map(|(card_idx, _)| Some(card_idx))
                .collect();
            if slots.len() < 4 {
                slots.resize(4, None);
            }
            slots
        }
        Err(e) => {
            error!("Failed to fetch played cards for game {}: {}", game_id, e);
            vec![]
        }
    };

    let snapshot = OutgoingMessage::GameStateSnapshot {
        game_id,
        roll: game_model.roll,
        rank: game_model.rank,
        status: game_model.status.to_string(),
        current_winning_card: game_model.current_winning_card,
        current_winning_player_position: game_model.current_winning_player_position,
        players: game_state_players,
        played_cards: played_cards.clone(),
        step_by_step: game_model.step_by_step,
    };

    match serde_json::to_string(&snapshot) {
        Ok(json) => {
            manager.send_to_player(game_id, player_id, &json).await;
            info!(
                "Sent game state snapshot for game {} to player {}",
                game_id, player_id
            );
        }
        Err(e) => {
            error!("Failed to serialize game state snapshot: {}", e);
        }
    }
}

/// Send a personalized GameStateSnapshot to every connected player in a game.
/// Queries the database once for game state, players, and played cards,
/// then computes a rotated display_position for each connected player.
pub(super) async fn send_snapshots_to_all_players(
    manager: &WebSocketManager,
    db: &sea_orm::DatabaseConnection,
    game_id: Uuid,
) {
    let game_repo = GameRepository::new(db.clone());
    let player_repo = PlayerRepository::new(db.clone());
    let game_card_repo = GameCardRepository::new(db.clone());

    let game_model = match game_repo.find_by_id(game_id).await {
        Ok(Some(g)) => g,
        Ok(None) => {
            tracing::warn!("Game {} not found for state snapshot (broadcast)", game_id);
            return;
        }
        Err(e) => {
            tracing::error!(
                "DB error fetching game {} for broadcast snapshot: {}",
                game_id,
                e
            );
            return;
        }
    };

    let players = match player_repo.list_by_game(game_id).await {
        Ok(p) => p,
        Err(e) => {
            error!("Failed to fetch players for game {}: {}", game_id, e);
            return;
        }
    };

    if players.is_empty() {
        return;
    }

    let num_players = players.len();
    let played_counts: HashMap<Uuid, usize> = count_played_cards_per_player(db, game_id).await;
    let played_cards: Vec<Option<i32>> = match game_card_repo
        .list_by_game_and_round(game_id, game_model.roll)
        .await
    {
        Ok(cards) => {
            let mut played_pairs: Vec<(i32, usize)> = Vec::new();
            let winner_pos = game_model.current_winning_player_position.unwrap_or(0) as usize;

            for card in cards {
                if let Some(pid) = card.player_id {
                    if let Some(pos) = players.iter().position(|p| p.id == pid) {
                        played_pairs.push((card.card_index, pos));
                    }
                }
            }
            played_pairs.sort_by_key(|(_, pos)| (num_players + *pos - winner_pos) % num_players);
            let mut slots: Vec<Option<i32>> = played_pairs
                .into_iter()
                .map(|(card_idx, _)| Some(card_idx))
                .collect();
            if slots.len() < 4 {
                slots.resize(4, None);
            }
            slots
        }
        Err(e) => {
            error!("Failed to fetch played cards for game {}: {}", game_id, e);
            vec![]
        }
    };

    let connected: Vec<(Uuid, i32)> = manager.get_connected_player_info(game_id).await;

    for (player_id, player_position) in connected {
        let my_pos = player_position as usize;

        let game_state_players: Vec<GameStatePlayer> = players
            .iter()
            .map(|p| {
                let display_pos =
                    compute_display_position(p.position as usize, num_players, my_pos);
                let player_type_str = match p.player_type {
                    PlayerType::Human => "human",
                    PlayerType::Bot => "bot",
                };
                GameStatePlayer {
                    id: p.id,
                    name: p.name.clone(),
                    position: p.position,
                    display_position: display_pos as i32,
                    player_type: player_type_str.to_string(),
                    cards_count: CARDS_PER_PLAYER as i32
                        - *played_counts.get(&p.id).unwrap_or(&0) as i32,
                }
            })
            .collect();

        let snapshot = OutgoingMessage::GameStateSnapshot {
            game_id,
            roll: game_model.roll,
            rank: game_model.rank,
            status: game_model.status.to_string(),
            current_winning_card: game_model.current_winning_card,
            current_winning_player_position: game_model.current_winning_player_position,
            players: game_state_players,
            played_cards: played_cards.clone(),
            step_by_step: game_model.step_by_step,
        };

        match serde_json::to_string(&snapshot) {
            Ok(json) => {
                manager.send_to_player(game_id, player_id, &json).await;
                info!(
                    "Sent game state snapshot for game {} to player {}",
                    game_id, player_id
                );
            }
            Err(e) => {
                error!(
                    "Failed to serialize game state snapshot for player {}: {}",
                    player_id, e
                );
            }
        }
    }
}

async fn count_played_cards_per_player(
    db: &sea_orm::DatabaseConnection,
    game_id: Uuid,
) -> HashMap<Uuid, usize> {
    let mut counts = HashMap::new();
    let gc_repo = GameCardRepository::new(db.clone());
    match gc_repo.list_played_by_game(game_id).await {
        Ok(cards) => {
            for card in cards {
                if let Some(pid) = card.player_id {
                    *counts.entry(pid).or_insert(0) += 1;
                }
            }
        }
        Err(e) => {
            error!("Failed to count played cards for game {}: {}", game_id, e);
        }
    }
    counts
}
