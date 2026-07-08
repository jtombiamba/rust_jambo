use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder};
use tracing::{debug, error, info};
use uuid::Uuid;

use crate::database::models::{game, game_card, player, PlayerType};
use crate::game::service::compute_display_position;

use super::manager::WebSocketManager;
use super::messages::{GameStateCard, GameStatePlayer, OutgoingMessage};

pub(super) async fn send_game_state_snapshot(
    manager: &WebSocketManager,
    db: &sea_orm::DatabaseConnection,
    game_id: Uuid,
    player_id: Uuid,
    player_position: i32,
) {
    let game_model = match game::Entity::find_by_id(game_id).one(db).await {
        Ok(Some(g)) => g,
        _ => {
            debug!("Game {} not found for state snapshot", game_id);
            return;
        }
    };

    let players = match player::Entity::find()
        .filter(player::Column::GameId.eq(game_id))
        .order_by_asc(player::Column::Position)
        .all(db)
        .await
    {
        Ok(p) => p,
        Err(e) => {
            error!("Failed to fetch players for game {}: {}", game_id, e);
            return;
        }
    };

    let num_players = players.len();
    let my_pos = player_position as usize;

    let game_state_players: Vec<GameStatePlayer> = players
        .iter()
        .map(|p| {
            let display_pos = compute_display_position(p.position as usize, num_players, my_pos);
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
            }
        })
        .collect();

    let played_cards: Vec<GameStateCard> = match game_card::Entity::find()
        .filter(game_card::Column::GameId.eq(game_id))
        .filter(game_card::Column::Played.eq(true))
        .filter(game_card::Column::Round.eq(game_model.roll))
        .all(db)
        .await
    {
        Ok(cards) => cards
            .into_iter()
            .filter_map(|c| {
                c.player_id.map(|pid| GameStateCard {
                    player_id: pid,
                    card_index: c.card_index,
                })
            })
            .collect(),
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
    let game_model = match game::Entity::find_by_id(game_id).one(db).await {
        Ok(Some(g)) => g,
        _ => {
            debug!("Game {} not found for state snapshot", game_id);
            return;
        }
    };

    let players = match player::Entity::find()
        .filter(player::Column::GameId.eq(game_id))
        .order_by_asc(player::Column::Position)
        .all(db)
        .await
    {
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
    let played_cards: Vec<GameStateCard> = match game_card::Entity::find()
        .filter(game_card::Column::GameId.eq(game_id))
        .filter(game_card::Column::Played.eq(true))
        .filter(game_card::Column::Round.eq(game_model.roll))
        .all(db)
        .await
    {
        Ok(cards) => cards
            .into_iter()
            .filter_map(|c| {
                c.player_id.map(|pid| GameStateCard {
                    player_id: pid,
                    card_index: c.card_index,
                })
            })
            .collect(),
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
