use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use tracing::error;
use uuid::Uuid;

use crate::database::models::{game, game_card, game_run, player, GameStatus, PlayerType};
use crate::game::constants::CARDS_PER_PLAYER;
use crate::game::service::types::RoundEvaluationResult;
use crate::game::special_cards::{compute_special_cards, SpecialCards};
use crate::messaging::events::GameEvent;
use crate::messaging::events::RoomEvent;
use crate::messaging::redis::PublishResult;
use crate::observability::metrics::RUN_COMPLETION_ERRORS_TOTAL;

use super::GameService;

impl GameService {
    pub(crate) async fn publish_card_played(
        &self,
        game_id: Uuid,
        player_id: Uuid,
        card_index: i32,
        next_turn: Option<Uuid>,
        correlation_id: Option<Uuid>,
    ) {
        if let Some(mut redis_client) = self.redis_client.clone() {
            let event = GameEvent::CardPlayed {
                game_id,
                player_id,
                card_index,
                next_turn,
                correlation_id,
            };
            match redis_client.publish_game_event_with_retry(&event).await {
                PublishResult::Published => {}
                PublishResult::RetryExhausted(e) => {
                    error!(
                        "CRITICAL: Failed to publish CardPlayed event for game {} after retries: {}",
                        game_id, e
                    );
                }
            }
        }
    }

    pub(crate) async fn publish_turn_changed(
        &self,
        game_id: Uuid,
        current_turn: Uuid,
        correlation_id: Option<Uuid>,
    ) {
        if let Some(mut redis_client) = self.redis_client.clone() {
            let event = GameEvent::TurnChanged {
                game_id,
                current_turn,
                correlation_id,
            };
            match redis_client.publish_game_event_with_retry(&event).await {
                PublishResult::Published => {}
                PublishResult::RetryExhausted(e) => {
                    error!(
                        "CRITICAL: Failed to publish TurnChanged event for game {}: {}",
                        game_id, e
                    );
                }
            }
        }
    }

    pub(crate) async fn publish_round_completed(
        &self,
        game_id: Uuid,
        result: &RoundEvaluationResult,
        players: &[player::Model],
        correlation_id: Option<Uuid>,
    ) {
        if let Some(mut redis_client) = self.redis_client.clone() {
            let win_type = if result.game_ended {
                match result.final_status {
                    GameStatus::Kora => Some("kora".to_string()),
                    GameStatus::DoubleKora => Some("doubleKora".to_string()),
                    _ => Some("normal".to_string()),
                }
            } else {
                Some("normal".to_string())
            };

            let num_players = players.len();
            let mut deck_slots: Vec<Option<i32>> = vec![None; num_players];

            if let Ok(played_cards) = game_card::Entity::find()
                .filter(game_card::Column::GameId.eq(game_id))
                .filter(game_card::Column::Round.eq(result.round))
                .filter(game_card::Column::Played.eq(true))
                .all(&self.db)
                .await
            {
                for card in &played_cards {
                    if let Some(pid) = card.player_id {
                        if let Some(pos) = players.iter().position(|p| p.id == pid) {
                            deck_slots[pos] = Some(card.card_index);
                        }
                    }
                }
            }

            let event = GameEvent::RoundCompleted {
                game_id,
                round_number: result.round,
                winner_id: result.winner_id,
                winner_position: result.winner_position as i32,
                win_type,
                deck_slots,
                correlation_id,
            };

            match redis_client.publish_game_event_with_retry(&event).await {
                PublishResult::Published => {}
                PublishResult::RetryExhausted(e) => {
                    error!(
                        "CRITICAL: Failed to publish RoundCompleted event for game {}: {}",
                        game_id, e
                    );
                }
            }
        }
    }

    pub(crate) async fn publish_game_finished(
        &self,
        game_id: Uuid,
        result: &RoundEvaluationResult,
        correlation_id: Option<Uuid>,
    ) {
        if let Some(mut redis_client) = self.redis_client.clone() {
            let winner_name = result
                .players
                .iter()
                .find(|p| p.id == result.winner_id)
                .map(|p| p.name.clone());

            let event = GameEvent::GameFinished {
                game_id,
                winner_id: Some(result.winner_id),
                winner_name,
                winner_position: Some(result.winner_position as i32),
                status: match result.final_status {
                    GameStatus::Kora => "kora".to_string(),
                    GameStatus::DoubleKora => "doubleKora".to_string(),
                    _ => "finished".to_string(),
                },
                final_score: None,
                rounds_played: result.round,
                correlation_id,
            };

            match redis_client.publish_game_event_with_retry(&event).await {
                PublishResult::Published => {}
                PublishResult::RetryExhausted(e) => {
                    error!(
                        "CRITICAL: Failed to publish GameFinished event for game {}: {}",
                        game_id, e
                    );
                }
            }

            let game_model = match game::Entity::find_by_id(game_id).one(&self.db).await {
                Ok(Some(g)) => g,
                _ => return,
            };

            if let Some(run_id) = game_model.game_run_id {
                self.finalize_run_on_game_completion(run_id, &game_model)
                    .await;
            }
        }
    }

    pub(crate) async fn publish_claim_resolved(&self, game_id: Uuid) {
        if let Some(mut redis_client) = self.redis_client.clone() {
            let event = GameEvent::ClaimResolved { game_id };
            match redis_client.publish_game_event_with_retry(&event).await {
                PublishResult::Published => {}
                PublishResult::RetryExhausted(e) => {
                    error!(
                        "CRITICAL: Failed to publish ClaimResolved event for game {}: {}",
                        game_id, e
                    );
                }
            }
        }
    }

    pub(crate) async fn publish_special_claim(
        &self,
        game_id: Uuid,
        player_id: Uuid,
        cards: Vec<i32>,
        winner_position: i32,
    ) {
        if let Some(mut redis_client) = self.redis_client.clone() {
            let event = GameEvent::SpecialClaim {
                game_id,
                player_id,
                cards,
                winner_position,
            };
            match redis_client.publish_game_event_with_retry(&event).await {
                PublishResult::Published => {}
                PublishResult::RetryExhausted(e) => {
                    error!(
                        "CRITICAL: Failed to publish SpecialClaim event for game {} after retries: {}",
                        game_id, e
                    );
                }
            }
        }
    }

    /// Publish the deal + start events for a freshly started game, including the
    /// per-player `CardsDealt`, the `GameStarted` broadcast and, when a player
    /// holds a claimable special combination, the `ClaimPending`/`ClaimOffered`
    /// events.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn publish_game_start_events(
        &self,
        game_id: Uuid,
        players: &[player::Model],
        player_ids: &[Uuid],
        cards: &[i32],
        askee: Option<Uuid>,
        askee_specials: Option<SpecialCards>,
        game_mode: String,
        first_player_id: Uuid,
    ) {
        let Some(mut redis) = self.redis_client.clone() else {
            return;
        };

        for &pid in player_ids {
            let player_cards: Vec<i32> = {
                let offset =
                    players.iter().position(|p| p.id == pid).unwrap_or(0) * CARDS_PER_PLAYER;
                cards[offset..offset + CARDS_PER_PLAYER].to_vec()
            };
            let event = GameEvent::CardsDealt {
                game_id,
                player_id: pid,
                cards: player_cards.clone(),
                special_cards: compute_special_cards(&player_cards),
            };
            if let PublishResult::RetryExhausted(e) =
                redis.publish_game_event_with_retry(&event).await
            {
                error!("Failed to publish CardsDealt event: {}", e);
            }
        }

        let game_started_players: Vec<crate::messaging::events::GameStartedPlayer> = players
            .iter()
            .map(|p| {
                let player_type_str = match p.player_type {
                    PlayerType::Human => "human",
                    PlayerType::Bot => "bot",
                };
                crate::messaging::events::GameStartedPlayer {
                    id: p.id,
                    name: p.name.clone(),
                    position: p.position,
                    display_position: p.position,
                    cards_count: CARDS_PER_PLAYER as i32,
                    player_type: player_type_str.to_string(),
                }
            })
            .collect();

        let event = GameEvent::GameStarted {
            game_id,
            players: game_started_players,
            current_turn: first_player_id,
            game_mode,
            correlation_id: None,
        };
        match redis.publish_game_event_with_retry(&event).await {
            PublishResult::Published => {}
            PublishResult::RetryExhausted(e) => {
                error!("Failed to publish GameStarted event: {}", e);
            }
        }

        if let (Some(askee_id), Some(specials)) = (askee, askee_specials) {
            let pending_event = GameEvent::ClaimPending { game_id };
            let _ = redis.publish_game_event_with_retry(&pending_event).await;

            let offered_event = GameEvent::ClaimOffered {
                game_id,
                player_id: askee_id,
                special_cards: specials,
            };
            if let PublishResult::RetryExhausted(e) =
                redis.publish_game_event_with_retry(&offered_event).await
            {
                error!("Failed to publish ClaimOffered event: {}", e);
            }
        }
    }

    async fn finalize_run_on_game_completion(&self, run_id: Uuid, _game_model: &game::Model) {
        let run = {
            let mut attempts = 0u32;
            loop {
                match game_run::Entity::find_by_id(run_id).one(&self.db).await {
                    Ok(Some(r)) => break r,
                    Ok(None) => {
                        tracing::warn!("Run {} not found for completion finalization", run_id);
                        RUN_COMPLETION_ERRORS_TOTAL.inc();
                        return;
                    }
                    Err(e) => {
                        attempts += 1;
                        if attempts >= 3 {
                            tracing::error!(
                                "Failed to finalize run {} after {} attempts: {}",
                                run_id,
                                attempts,
                                e
                            );
                            RUN_COMPLETION_ERRORS_TOTAL.inc();
                            return;
                        }
                        tracing::warn!("Retry {}/3 looking up run {}: {}", attempts, run_id, e);
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    }
                }
            }
        };

        if run.current_game_index >= run.num_games {
            match game_run::Entity::update_many()
                .col_expr(
                    game_run::Column::Status,
                    sea_orm::sea_query::Expr::value(sea_orm::Value::String(Some(
                        "completed".to_string(),
                    ))),
                )
                .filter(game_run::Column::Id.eq(run_id))
                .filter(game_run::Column::Status.eq("active"))
                .exec(&self.db)
                .await
            {
                Ok(result) => {
                    if result.rows_affected > 0 {
                        tracing::info!("Run {} completed after last game finished", run_id);

                        if let Some(mut redis_client) = self.redis_client.clone() {
                            let room_event = RoomEvent::RunCompleted {
                                room_id: run.room_id,
                                run_id,
                            };
                            match redis_client
                                .publish_room_event_with_retry(&room_event)
                                .await
                            {
                                PublishResult::Published => {}
                                PublishResult::RetryExhausted(e) => {
                                    tracing::error!(
                                        "CRITICAL: Failed to publish RunCompleted event for run {} after retries: {}",
                                        run_id, e
                                    );
                                }
                            }
                        }
                    } else {
                        tracing::debug!("Run {} already completed (0 rows affected)", run_id);
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to update run {} status to completed: {}", run_id, e);
                }
            }
        }
    }
}
