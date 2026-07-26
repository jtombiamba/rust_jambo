use uuid::Uuid;

use crate::database::models::player;
use crate::game::service::types::RoundEvaluationResult;
use crate::observability::metrics;

use super::super::GameService;

pub(crate) struct PostCommitContext {
    pub(crate) game_id: Uuid,
    pub(crate) player_id: Uuid,
    pub(crate) card_index: i32,
    pub(crate) next_player_id: Uuid,
    pub(crate) players: Vec<player::Model>,
    pub(crate) game_ended: bool,
    pub(crate) round_result: Option<RoundEvaluationResult>,
    pub(crate) correlation_id: Option<Uuid>,
}

impl PostCommitContext {
    pub(crate) async fn handle(self, service: &GameService) {
        let PostCommitContext {
            game_id,
            player_id,
            card_index,
            next_player_id,
            players,
            game_ended,
            round_result,
            correlation_id,
        } = self;

        service
            .publish_card_played(
                game_id,
                player_id,
                card_index,
                Some(next_player_id),
                correlation_id,
            )
            .await;

        if !game_ended {
            service
                .publish_turn_changed(game_id, next_player_id, correlation_id)
                .await;
        }

        if let Some(ref result) = round_result {
            service
                .publish_round_completed(game_id, result, &players, correlation_id)
                .await;

            if result.game_ended {
                metrics::GAMES_FINISHED_TOTAL
                    .with_label_values(&[&result.final_status.to_string()])
                    .inc();
                service
                    .publish_game_finished(game_id, result, correlation_id)
                    .await;
                service.invalidate_game_state_cache(game_id).await;

                let user_ids: Vec<Uuid> = result.players.iter().filter_map(|p| p.user_id).collect();
                if !user_ids.is_empty() {
                    service.invalidate_dashboard_caches(&user_ids).await;
                }
            }
        }

        if !game_ended {
            service.cache_game_state(game_id).await;
        }
    }
}
