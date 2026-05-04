use sea_orm::DatabaseConnection;
use uuid::Uuid;
use tracing::{info, debug};

use crate::database::repositories::{GameCardRepository, GameRepository, PlayerRepository};
use crate::error::GameError;
use crate::game::service::GameService;
use crate::game::strategy::compute_strategy;
use crate::messaging::ai_task::{AITask, PlayerInfo};

/// Bot move execution result
#[derive(Debug)]
pub struct BotMoveResult {
    pub game_id: Uuid,
    pub player_id: Uuid,
    pub chosen_card: i32,
    pub next_player: Option<Uuid>,
    pub should_continue: bool,
}

/// Execute a bot move using database connection
pub async fn execute_bot_move(
    game_id: Uuid,
    player_id: Uuid,
    db: &DatabaseConnection,
) -> Result<BotMoveResult, GameError> {
    let service = GameService::new(db.clone());
    let player_repo = PlayerRepository::new(db.clone());
    let game_card_repo = GameCardRepository::new(db.clone());
    let game_repo = GameRepository::new(db.clone());

    let game = game_repo
        .find_by_id(game_id)
        .await?
        .ok_or(GameError::GameNotFound)?;

    let roll = game.roll;

    let bot_cards = game_card_repo
        .list_by_player(player_id)
        .await?
        .into_iter()
        .filter(|gc| gc.round.is_none())
        .map(|gc| gc.card_index)
        .collect::<Vec<i32>>();

    if bot_cards.is_empty() {
        return Err(GameError::Internal(Box::new(std::io::Error::new(
            std::io::ErrorKind::Other,
            "No cards available for bot",
        ))));
    }

    let round_cards = game_card_repo
        .list_by_game_and_round(game_id, roll)
        .await?
        .into_iter()
        .map(|gc| gc.card_index)
        .collect::<Vec<i32>>();

    let current_winning = game.current_winning_card;

    debug!(
        "Bot {} has {} unplayed cards: {:?}",
        player_id,
        bot_cards.len(),
        bot_cards
    );
    debug!(
        "Round {} has {} played cards: {:?}",
        roll,
        round_cards.len(),
        round_cards
    );
    debug!("Current winning card: {:?}", current_winning);

    let chosen = compute_strategy(&bot_cards, &round_cards, current_winning);
    info!("Bot {} selected card index {}", player_id, chosen);

    service
        .update_card_play(game_id, player_id, chosen, None)
        .await
        .map_err(|e| {
            GameError::Internal(Box::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to play card: {}", e),
            )))
        })?;

    info!("Bot {} played card {}", player_id, chosen);

    let next_player = service.next_player(game_id).await.map_err(|e| {
        GameError::Internal(Box::new(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Failed to determine next player: {}", e),
        )))
    })?;

    let players = player_repo.list_by_game(game_id).await?;

    let next_player_type = players
        .iter()
        .find(|p| p.id == next_player)
        .map(|p| p.player_type.clone());

    let should_continue =
        matches!(next_player_type, Some(crate::database::models::PlayerType::Bot));

    Ok(BotMoveResult {
        game_id,
        player_id,
        chosen_card: chosen,
        next_player: Some(next_player),
        should_continue,
    })
}

/// Execute a bot move using AITask (no database queries needed)
pub async fn execute_bot_move_from_task(
    task: &AITask,
) -> Result<BotMoveResult, GameError> {
    let bot_cards = task.bot_hand_cards.clone();

    if bot_cards.is_empty() {
        return Err(GameError::Internal(Box::new(std::io::Error::new(
            std::io::ErrorKind::Other,
            "No cards available for bot",
        ))));
    }

    let chosen = compute_strategy(
        &bot_cards,
        &task.played_cards_this_round,
        task.current_winning_card,
    );

    info!(
        "Bot {} selected card index {} from AITask",
        task.player_id, chosen
    );

    Ok(BotMoveResult {
        game_id: task.game_id,
        player_id: task.player_id,
        chosen_card: chosen,
        next_player: task.current_player_turn,
        should_continue: false,
    })
}

/// Create an AITask from current game state
pub async fn create_ai_task_from_game(
    game_id: Uuid,
    player_id: Uuid,
    db: &DatabaseConnection,
) -> Result<AITask, GameError> {
    let player_repo = PlayerRepository::new(db.clone());
    let game_card_repo = GameCardRepository::new(db.clone());
    let game_repo = GameRepository::new(db.clone());

    let game = game_repo
        .find_by_id(game_id)
        .await?
        .ok_or(GameError::GameNotFound)?;

    let rank = game.rank.unwrap_or(0);
    let roll = game.roll;
    let game_status = format!("{:?}", game.status);
    let bet = game.bet;
    let auto_mode = game.auto;
    let current_player_turn = Some(player_id);

    let players = player_repo.list_by_game(game_id).await?;

    let player_infos: Vec<PlayerInfo> = players
        .into_iter()
        .map(|p| PlayerInfo {
            player_id: p.id,
            position: p.position,
            player_type: format!("{:?}", p.player_type),
            credits: p.credits,
            name: p.name,
        })
        .collect();

    let bot_cards = game_card_repo
        .list_by_player(player_id)
        .await?
        .into_iter()
        .filter(|gc| gc.round.is_none())
        .map(|gc| gc.card_index)
        .collect::<Vec<i32>>();

    let round_game_cards = game_card_repo
        .list_by_game_and_round(game_id, roll)
        .await?;

    let round_cards: Vec<i32> = round_game_cards.iter().map(|gc| gc.card_index).collect();

    let current_winning = game.current_winning_card;

    let winning_player_position = if let Some(card_index) = current_winning {
        round_game_cards
            .iter()
            .find(|gc| gc.card_index == card_index)
            .and_then(|gc| gc.player_id)
            .and_then(|player_id| {
                player_infos
                    .iter()
                    .find(|p| p.player_id == player_id)
                    .map(|p| p.position)
            })
    } else {
        None
    };

    Ok(AITask::new(
        game_id,
        player_id,
        None, // correlation_id not available in sync fallback
        rank,
        roll,
        game_status,
        current_player_turn,
        round_cards,
        bot_cards,
        player_infos,
        current_winning,
        winning_player_position,
        bet,
        auto_mode,
    ))
}

/// Check if a player is a bot
pub async fn is_bot_player(
    game_id: Uuid,
    player_id: Uuid,
    db: &DatabaseConnection,
) -> Result<bool, GameError> {
    let player_repo = PlayerRepository::new(db.clone());

    let players = player_repo.list_by_game(game_id).await?;

    let player = players
        .into_iter()
        .find(|p| p.id == player_id)
        .ok_or(GameError::PlayerNotFound)?;

    Ok(matches!(
        player.player_type,
        crate::database::models::PlayerType::Bot
    ))
}
