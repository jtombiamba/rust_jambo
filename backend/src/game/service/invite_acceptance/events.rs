use tracing::error;
use uuid::Uuid;

use crate::database::models::GameStatus;
use crate::messaging::events::GameEvent;
use crate::messaging::redis::PublishResult;
use crate::messaging::RedisClient;

#[allow(clippy::too_many_arguments)]
pub(crate) async fn publish_player_joined(
    redis: &Option<RedisClient>,
    game_id: Uuid,
    player_id: Uuid,
    user_id: Uuid,
    pseudo: &str,
    position: i32,
    player_count: i32,
    max_players: i32,
) {
    let redis = match redis {
        Some(r) => r,
        None => return,
    };
    let event = GameEvent::PlayerJoined {
        game_id,
        player_id,
        user_id,
        pseudo: pseudo.to_string(),
        position,
        player_count,
        max_players,
    };
    match redis.clone().publish_game_event_with_retry(&event).await {
        PublishResult::Published => {}
        PublishResult::RetryExhausted(e) => {
            error!("Failed to publish PlayerJoined event: {}", e);
        }
    }
}

pub(crate) async fn publish_game_ready_if_needed(
    redis: &Option<RedisClient>,
    game_id: Uuid,
    new_status: GameStatus,
) {
    if new_status != GameStatus::Ready {
        return;
    }
    let redis = match redis {
        Some(r) => r,
        None => return,
    };
    let event = GameEvent::GameReady {
        game_id,
        correlation_id: None,
    };
    match redis.clone().publish_game_event_with_retry(&event).await {
        PublishResult::Published => {}
        PublishResult::RetryExhausted(e) => {
            error!("Failed to publish GameReady event: {}", e);
        }
    }
}
