use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use tracing::{error, info};

use crate::database::models::{game, GameStatus, PlayerType};
use crate::database::repositories::PlayerRepository;
use crate::messaging::RedisClient;

use super::GameService;

impl GameService {
    pub async fn detect_and_recover_stalled_games(
        db: sea_orm::DatabaseConnection,
        redis_client: Option<RedisClient>,
        staleness_threshold: chrono::Duration,
    ) -> u64 {
        let now = chrono::Utc::now();
        let cutoff = now - staleness_threshold;

        let stalled_games = match game::Entity::find()
            .filter(game::Column::Status.eq(GameStatus::Active))
            .filter(game::Column::UpdatedAt.lt(cutoff))
            .all(&db)
            .await
        {
            Ok(games) => games,
            Err(e) => {
                error!("Failed to query stalled games: {}", e);
                return 0;
            }
        };

        let mut recovered = 0u64;
        for g in stalled_games {
            let player_repo = PlayerRepository::new(db.clone());
            let players = match player_repo.list_by_game(g.id).await {
                Ok(players) => players,
                Err(e) => {
                    error!("Failed to fetch players for game {}: {}", g.id, e);
                    continue;
                }
            };

            let current_rank = g.rank.unwrap_or(0) as usize;
            let current_player = match players.get(current_rank) {
                Some(player) if matches!(player.player_type, PlayerType::Bot) => player,
                _ => continue,
            };

            info!(
                "Detected stalled game {}: current bot player {}, last updated {:?} seconds ago",
                g.id,
                current_player.id,
                (now - g.updated_at).num_seconds()
            );

            crate::observability::metrics::GAMES_STALLED_TOTAL.inc();

            let db_clone = db.clone();
            let redis_clone = redis_client.clone();
            let game_id = g.id;
            let player_id = current_player.id;
            tokio::spawn(async move {
                crate::game::bot_scheduler::BotScheduler::run_sync_chain(
                    db_clone,
                    redis_clone,
                    game_id,
                    player_id,
                )
                .await;
            });

            recovered += 1;
        }

        if recovered > 0 {
            info!("Recovered {} stalled games", recovered);
        }
        recovered
    }
}
