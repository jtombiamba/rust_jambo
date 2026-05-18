use uuid::Uuid;

use crate::database::models::GameStatus;
use crate::database::repositories::{GameCardRepository, GameRepository, PlayerRepository};
use crate::game::service::types::GameServiceError;
use crate::messaging::ai_task::{AITask, PlayerInfo};

use super::GameService;

impl GameService {
    pub async fn build_ai_task(
        &self,
        game_id: Uuid,
        player_id: Uuid,
        correlation_id: Option<Uuid>,
    ) -> Result<AITask, GameServiceError> {
        let span = tracing::info_span!(
            "build_ai_task",
            correlation_id = %correlation_id.map(|id| id.to_string()).unwrap_or_default(),
            game_id = %game_id,
            player_id = %player_id,
        );
        let _guard = span.enter();
        let game_repo = GameRepository::new(self.db.clone());
        let player_repo = PlayerRepository::new(self.db.clone());
        let card_repo = GameCardRepository::new(self.db.clone());

        let game = game_repo
            .find_by_id(game_id)
            .await
            .map_err(|e| {
                GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
            })?
            .ok_or(GameServiceError::GameNotFound)?;

        let players_future = player_repo.list_by_game(game_id);
        let bot_cards_future = card_repo.list_by_player(player_id);

        let current_round = game.roll;

        let round_cards_future = card_repo.list_by_game_and_round(game_id, current_round);

        let players = players_future.await.map_err(|e| {
            GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
        })?;

        let bot_cards = bot_cards_future.await.map_err(|e| {
            GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
        })?;

        let round_cards = round_cards_future.await.map_err(|e| {
            GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
        })?;

        let player_positions: std::collections::HashMap<Uuid, i32> = {
            if game.player_positions.is_null() {
                players.iter().map(|p| (p.id, p.position)).collect()
            } else {
                serde_json::from_value(game.player_positions.clone()).map_err(|e| {
                    GameServiceError::Internal(format!("Failed to parse player positions: {}", e))
                })?
            }
        };

        let player_info_list: Vec<PlayerInfo> = players
            .iter()
            .map(|player| {
                let position = player_positions
                    .get(&player.id)
                    .copied()
                    .unwrap_or(player.position);
                let player_type_str = match player.player_type {
                    crate::database::models::PlayerType::Human => "human".to_string(),
                    crate::database::models::PlayerType::Bot => "bot".to_string(),
                };
                PlayerInfo {
                    player_id: player.id,
                    position,
                    player_type: player_type_str,
                    credits: player.credits,
                    name: player.name.clone(),
                }
            })
            .collect();

        let bot_hand_cards: Vec<i32> = bot_cards
            .iter()
            .filter(|gc| !gc.played)
            .map(|gc| gc.card_index)
            .collect();

        let played_cards_this_round: Vec<i32> = round_cards
            .iter()
            .filter(|gc| gc.played)
            .map(|gc| gc.card_index)
            .collect();

        let current_player_turn = if game.status == GameStatus::Active {
            let current_rank = game.rank.unwrap_or(0) as usize;
            players.get(current_rank).map(|p| p.id)
        } else {
            None
        };

        let task = AITask::new(
            game_id,
            player_id,
            correlation_id,
            current_round,
            current_round,
            format!("{:?}", game.status),
            current_player_turn,
            played_cards_this_round,
            bot_hand_cards,
            player_info_list,
            game.current_winning_card,
            game.current_winning_player_position,
            game.bet,
            game.auto,
        );

        Ok(task)
    }
}
