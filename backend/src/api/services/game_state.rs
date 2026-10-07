use std::collections::HashMap;

use uuid::Uuid;

use crate::api::dto::responses::{PlayerInfoDto, QuickGameResponse};
use crate::database::models::{GameMode, GameStatus, PlayerType};
use crate::database::traits::{GameCardRepoTrait, GameRepoTrait};
use crate::error::AppError;
use crate::game::service::{build_played_card_slots, compute_display_position};
use crate::game::special_cards::compute_special_cards;

pub(crate) async fn build_game_state_response(
    game_repo: &dyn GameRepoTrait,
    card_repo: &dyn GameCardRepoTrait,
    game: &crate::database::models::Game,
    user_id: Uuid,
) -> Result<QuickGameResponse, AppError> {
    let all_players = game_repo
        .list_players(game.id)
        .await
        .map_err(AppError::Database)?;

    let num_players = all_players.len();

    let my_player = all_players.iter().find(|p| p.user_id == Some(user_id));
    let my_position = my_player.map(|p| p.position as usize).unwrap_or(0);

    let my_cards: Vec<i32> = if let Some(mp) = my_player {
        match card_repo.list_by_player(mp.id).await {
            Ok(cards) => cards
                .into_iter()
                .filter(|c| !c.played)
                .map(|c| c.card_index)
                .collect(),
            Err(_) => Vec::new(),
        }
    } else {
        Vec::new()
    };

    let my_special_cards: Option<crate::game::special_cards::SpecialCards> =
        if matches!(game.game_mode, GameMode::Multiplayer) {
            match my_player {
                Some(mp) => match card_repo.list_by_player(mp.id).await {
                    Ok(cards) => {
                        let hand: Vec<i32> = cards.iter().map(|c| c.card_index).collect();
                        Some(compute_special_cards(&hand))
                    }
                    Err(_) => None,
                },
                None => None,
            }
        } else {
            None
        };

    let all_game_cards = card_repo.list_by_game(game.id).await.unwrap_or_default();

    let mut remaining_counts: HashMap<Uuid, usize> = HashMap::new();
    for player in &all_players {
        let unplayed = all_game_cards
            .iter()
            .filter(|c| c.player_id == Some(player.id) && !c.played)
            .count();
        remaining_counts.insert(player.id, unplayed);
    }

    let current_round = game.roll;
    let mut played_pairs: Vec<(i32, usize)> = Vec::new();
    for card in &all_game_cards {
        if card.played && card.round == Some(current_round) {
            if let Some(pid) = card.player_id {
                if let Some(pos) = all_players.iter().position(|p| p.id == pid) {
                    played_pairs.push((card.card_index, pos));
                }
            }
        }
    }

    let deck_slots: Vec<Option<i32>> = if played_pairs.is_empty() {
        vec![None; num_players]
    } else {
        let winner_pos = game.current_winning_player_position.unwrap_or(0) as usize;
        build_played_card_slots(played_pairs, num_players, winner_pos)
    };

    let players_json: Vec<PlayerInfoDto> = all_players
        .iter()
        .map(|player| {
            let player_type = match player.player_type {
                PlayerType::Human => "human",
                PlayerType::Bot => "bot",
            };
            let is_me = player.user_id == Some(user_id);
            let cards = if is_me { my_cards.clone() } else { Vec::new() };
            let cards_count = *remaining_counts.get(&player.id).unwrap_or(&0) as i32;
            let display_pos =
                compute_display_position(player.position as usize, num_players, my_position);
            PlayerInfoDto {
                id: player.id,
                player_type: player_type.to_string(),
                name: player.name.clone(),
                position: player.position,
                display_position: display_pos as i32,
                cards,
                cards_count,
                is_current_user: is_me,
            }
        })
        .collect();

    let current_turn = game.rank.unwrap_or(0);
    let current_turn_display =
        compute_display_position(current_turn as usize, num_players, my_position) as i32;

    Ok(QuickGameResponse {
        game_id: game.id,
        players: players_json,
        status: match game.status {
            GameStatus::Active => "active".to_string(),
            GameStatus::Ready => "ready".to_string(),
            _ => "pending".to_string(),
        },
        current_turn: current_turn_display,
        bet: game.bet,
        max_players: game.max_players as i32,
        invite_expires_at: game.invite_expires_at.map(|t| t.to_rfc3339()),
        deck_slots: Some(deck_slots),
        ws_token: None,
        step_by_step: game.step_by_step,
        special_cards: my_special_cards,
    })
}
