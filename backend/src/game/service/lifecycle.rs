use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, QueryFilter, QueryOrder,
    TransactionTrait,
};
use tracing::{error, info};
use uuid::Uuid;

use crate::database::models::{
    game, game_card, game_invite, player, player_profile, GameMode, GameStatus, InviteStatus,
    PlayerType,
};
use crate::error::GameError;
use crate::game::constants::CARDS_PER_PLAYER;
use crate::game::service::types::GameCreationTimer;
use crate::game::special_cards::{compute_special_cards, select_askee};
use crate::messaging::events::{GameEvent, UserEvent};
use crate::messaging::redis::PublishResult;
use crate::observability::metrics;

use super::GameService;

impl GameService {
    pub async fn cancel_game(&self, game_id: Uuid) -> Result<(), GameError> {
        let txn = self.db.begin().await?;

        let game_model = game::Entity::find_by_id(game_id)
            .one(&txn)
            .await?
            .ok_or(GameError::GameNotFound)?;

        if game_model.status != GameStatus::Pending {
            txn.rollback().await.ok();
            return Ok(());
        }

        let players = player::Entity::find()
            .filter(player::Column::GameId.eq(game_id))
            .all(&txn)
            .await?;

        for p in &players {
            if let Some(uid) = p.user_id {
                let profile = player_profile::Entity::find()
                    .filter(player_profile::Column::UserId.eq(uid))
                    .one(&txn)
                    .await?;
                if let Some(profile_model) = profile {
                    let mut profile_active: player_profile::ActiveModel = profile_model.into();
                    profile_active.credit =
                        ActiveValue::Set(profile_active.credit.unwrap() + game_model.bet);
                    profile_active.updated_at = ActiveValue::Set(chrono::Utc::now());
                    profile_active.update(&txn).await?;
                }
            }
        }

        let mut game_active: game::ActiveModel = game_model.into();
        game_active.status = ActiveValue::Set(GameStatus::Cancelled);
        game_active.updated_at = ActiveValue::Set(chrono::Utc::now());
        game_active.finished_at = ActiveValue::Set(Some(chrono::Utc::now()));
        game_active.update(&txn).await?;

        let pending_invites = game_invite::Entity::find()
            .filter(game_invite::Column::GameId.eq(game_id))
            .filter(game_invite::Column::Status.eq(InviteStatus::Pending))
            .all(&txn)
            .await?;
        let mut invited_user_ids: Vec<Uuid> = Vec::new();
        for inv in pending_invites {
            invited_user_ids.push(inv.invited_user_id);
            let mut inv_active: game_invite::ActiveModel = inv.into();
            inv_active.status = ActiveValue::Set(InviteStatus::Declined);
            inv_active.update(&txn).await?;
        }

        txn.commit().await?;

        info!("Game cancelled: game_id={}", game_id);

        if let Some(ref redis) = self.redis_client {
            let event = GameEvent::GameCancelled {
                game_id,
                reason: "Not enough players joined before timeout".to_string(),
            };
            if let PublishResult::RetryExhausted(e) =
                redis.clone().publish_game_event_with_retry(&event).await
            {
                error!(
                    "CRITICAL: Failed to publish GameCancelled event after retries: {}",
                    e
                );
            }

            for user_id in &invited_user_ids {
                let user_event = UserEvent::InviteRemoved {
                    user_id: *user_id,
                    game_id,
                };
                if let PublishResult::RetryExhausted(e) = redis
                    .clone()
                    .publish_user_event_with_retry(&user_event)
                    .await
                {
                    error!(
                        "Failed to publish InviteRemoved event for user {} after retries: {}",
                        user_id, e
                    );
                }
            }
        }

        let user_ids: Vec<Uuid> = players.iter().filter_map(|p| p.user_id).collect();
        if !user_ids.is_empty() {
            self.invalidate_dashboard_caches(&user_ids).await;
        }

        Ok(())
    }

    pub async fn kick_player_from_game(
        &self,
        game_id: Uuid,
        kicked_player_id: Uuid,
        all_players: &[player::Model],
    ) -> Result<(), GameError> {
        let txn = self.db.begin().await?;

        let game_model = game::Entity::find_by_id(game_id)
            .one(&txn)
            .await?
            .ok_or(GameError::GameNotFound)?;

        if game_model.status != GameStatus::Active {
            txn.rollback().await.ok();
            return Err(GameError::GameFinished);
        }

        let kicked_player = all_players
            .iter()
            .find(|p| p.id == kicked_player_id)
            .ok_or(GameError::PlayerNotFound)?;

        // Mark player as kicked
        let mut kicked_active: player::ActiveModel = kicked_player.clone().into();
        kicked_active.kicked = ActiveValue::Set(true);
        kicked_active.kicked_at = ActiveValue::Set(Some(chrono::Utc::now()));
        kicked_active.update(&txn).await?;

        let old_rank = game_model.rank.unwrap_or(0) as usize;

        // Build list of players that remain after this kick.
        // IMPORTANT: filter by ID, NOT by `p.kicked`, because `all_players` contains
        // the original models where `kicked=false` for the player being kicked now.
        let active_players: Vec<&player::Model> = all_players
            .iter()
            .filter(|p| p.id != kicked_player_id)
            .collect();

        let has_human_remaining = active_players
            .iter()
            .any(|p| matches!(p.player_type, PlayerType::Human));
        let is_solo_game = matches!(game_model.game_mode, GameMode::Solo);

        // ── Solo game: only bots remain → kill the game, no credit to refund ──
        if is_solo_game && !has_human_remaining {
            info!(
                "Solo game {} has only bots remaining after kick, finishing game without credit processing",
                game_id
            );

            let mut game_active: game::ActiveModel = game_model.clone().into();
            game_active.status = ActiveValue::Set(GameStatus::Finished);
            game_active.winner_id = ActiveValue::Set(None);
            game_active.finished_at = ActiveValue::Set(Some(chrono::Utc::now()));
            game_active.updated_at = ActiveValue::Set(chrono::Utc::now());
            game_active.player_positions = ActiveValue::Set(serde_json::json!({}));
            game_active.stall_warning_sent_at = ActiveValue::Set(None);
            game_active.update(&txn).await?;

            txn.commit().await?;
            metrics::ACTIVE_GAMES.dec();

            if let Some(ref redis) = self.redis_client {
                let pk_event = GameEvent::PlayerKicked {
                    game_id,
                    player_id: kicked_player.id,
                    player_name: kicked_player.name.clone(),
                };
                let _ = redis.clone().publish_game_event_with_retry(&pk_event).await;

                let gf_event = GameEvent::GameFinished {
                    game_id,
                    winner_id: None,
                    winner_name: None,
                    winner_position: None,
                    status: "finished".to_string(),
                    final_score: None,
                    rounds_played: game_model.roll,
                    correlation_id: None,
                };
                let _ = redis.clone().publish_game_event_with_retry(&gf_event).await;
            }

            self.invalidate_game_state_cache(game_id).await;
            return Ok(());
        }

        // ── 0 or 1 player remains: game ends, remaining player wins ──
        if active_players.len() <= 1 {
            let winner = active_players
                .first()
                .ok_or_else(|| GameError::internal("No remaining players"))?;

            let mut game_active: game::ActiveModel = game_model.clone().into();
            game_active.status = ActiveValue::Set(GameStatus::Finished);
            game_active.winner_id = ActiveValue::Set(Some(winner.id));
            game_active.finished_at = ActiveValue::Set(Some(chrono::Utc::now()));
            game_active.updated_at = ActiveValue::Set(chrono::Utc::now());
            game_active.player_positions = ActiveValue::Set(
                serde_json::to_value(std::collections::HashMap::<i32, Uuid>::from([(
                    0,
                    winner.user_id.unwrap_or(Uuid::nil()),
                )]))
                .map_err(|e| {
                    GameError::internal(format!("Failed to serialize positions: {}", e))
                })?,
            );
            game_active.stall_warning_sent_at = ActiveValue::Set(None);
            game_active.update(&txn).await?;

            // Process payment only if the winner has a user_id (authenticated player)
            if winner.user_id.is_some() {
                let bet = game_model.bet;
                let mut winner_active: player::ActiveModel = (**winner).clone().into();
                winner_active.credits = ActiveValue::Set(winner.credits + bet);
                winner_active.update(&txn).await?;
            }

            txn.commit().await?;
            metrics::ACTIVE_GAMES.dec();

            info!(
                "Player {} kicked from game {}, {} remaining -> {} wins by forfeit",
                kicked_player.id,
                game_id,
                active_players.len(),
                winner.id
            );

            // Fire-and-forget the kick email so it doesn't block the caller
            let mailer = self.mailer.clone();
            let db = self.db.clone();
            let kicked_user_id = kicked_player.user_id;
            let bet = game_model.bet;
            tokio::spawn(async move {
                Self::send_kicked_email_impl(mailer, db, kicked_user_id, game_id, bet).await;
            });

            if let Some(ref redis) = self.redis_client {
                let pk_event = GameEvent::PlayerKicked {
                    game_id,
                    player_id: kicked_player.id,
                    player_name: kicked_player.name.clone(),
                };
                let _ = redis.clone().publish_game_event_with_retry(&pk_event).await;

                let fw_event = GameEvent::PlayerForfeitWin {
                    game_id,
                    winner_id: winner.id,
                    winner_name: winner.name.clone(),
                };
                let _ = redis.clone().publish_game_event_with_retry(&fw_event).await;

                let final_score = if winner.user_id.is_some() {
                    Some(winner.credits + game_model.bet)
                } else {
                    None
                };

                let gf_event = GameEvent::GameFinished {
                    game_id,
                    winner_id: Some(winner.id),
                    winner_name: Some(winner.name.clone()),
                    winner_position: Some(winner.position),
                    status: "finished".to_string(),
                    final_score,
                    rounds_played: game_model.roll,
                    correlation_id: None,
                };
                let _ = redis.clone().publish_game_event_with_retry(&gf_event).await;
            }

            self.invalidate_game_state_cache(game_id).await;

            let user_ids: Vec<Uuid> = all_players.iter().filter_map(|p| p.user_id).collect();
            if !user_ids.is_empty() {
                self.invalidate_dashboard_caches(&user_ids).await;
            }

            return Ok(());
        }

        // ── 2+ players remain: reseat positions ──
        let new_positions: std::collections::HashMap<i32, Uuid> = active_players
            .iter()
            .enumerate()
            .filter_map(|(i, p)| p.user_id.map(|uid| (i as i32, uid)))
            .collect();

        let old_position_to_new: std::collections::HashMap<usize, usize> = active_players
            .iter()
            .enumerate()
            .map(|(new_pos, p)| {
                let old_pos = p.position as usize;
                (old_pos, new_pos)
            })
            .collect();

        // Update positions of remaining players
        for (new_pos, &active) in active_players.iter().enumerate() {
            let mut p_active: player::ActiveModel = (*active).clone().into();
            p_active.position = ActiveValue::Set(new_pos as i32);
            p_active.update(&txn).await?;
        }

        // Advance turn if the kicked player was the current player
        let new_rank = if old_rank == kicked_player.position as usize {
            let current_player_new_pos = old_position_to_new.get(&old_rank).copied().unwrap_or(0);
            current_player_new_pos as i32
        } else {
            let old_kicked_pos = kicked_player.position as usize;
            old_position_to_new
                .get(&old_rank)
                .copied()
                .unwrap_or(if old_rank > old_kicked_pos {
                    old_rank - 1
                } else {
                    old_rank
                }) as i32
        };

        let mut game_active: game::ActiveModel = game_model.into();
        game_active.player_positions =
            ActiveValue::Set(serde_json::to_value(&new_positions).map_err(|e| {
                GameError::internal(format!("Failed to serialize positions: {}", e))
            })?);
        game_active.rank = ActiveValue::Set(Some(new_rank));
        game_active.updated_at = ActiveValue::Set(chrono::Utc::now());
        game_active.stall_warning_sent_at = ActiveValue::Set(None);
        game_active.update(&txn).await?;

        txn.commit().await?;

        info!(
            "Player {} kicked from game {}, {} players remaining, new rank {}",
            kicked_player.id,
            game_id,
            active_players.len(),
            new_rank
        );

        if let Some(ref redis) = self.redis_client {
            let pk_event = GameEvent::PlayerKicked {
                game_id,
                player_id: kicked_player.id,
                player_name: kicked_player.name.clone(),
            };
            let _ = redis.clone().publish_game_event_with_retry(&pk_event).await;

            let rs_event = GameEvent::GameReshuffled {
                game_id,
                remaining_players: active_players.len() as u32,
            };
            let _ = redis.clone().publish_game_event_with_retry(&rs_event).await;
        }

        self.invalidate_game_state_cache(game_id).await;

        // Schedule bot move if new current player is a bot
        let new_rank_usize = new_rank as usize;
        if new_rank_usize < active_players.len() {
            let next = active_players[new_rank_usize];
            if matches!(next.player_type, PlayerType::Bot) {
                let db = self.db.clone();
                let redis = self.redis_client.clone();
                let gid = game_id;
                let nid = next.id;
                let fds = self.freeze_duration_secs;
                let ucnp = self.unfreeze_credit_no_payment;
                tokio::spawn(async move {
                    crate::game::bot_scheduler::BotScheduler::run_sync_chain(
                        db, redis, gid, nid, fds, ucnp, None,
                    )
                    .await;
                });
            }
        }

        Ok(())
    }

    #[allow(dead_code)]
    pub async fn cancel_expired_games(&self) -> Result<u64, GameError> {
        let now = chrono::Utc::now();

        // TODO: should be in a repository
        let expired_games = game::Entity::find()
            .filter(game::Column::Status.eq(GameStatus::Pending))
            .filter(game::Column::GameMode.eq(GameMode::Multiplayer))
            .filter(game::Column::InviteExpiresAt.lte(now))
            .all(&self.db)
            .await?;

        let mut cancelled = 0u64;
        for g in expired_games {
            if let Err(e) = self.cancel_game(g.id).await {
                error!("Failed to cancel expired game {}: {}", g.id, e);
            } else {
                cancelled += 1;
            }
        }

        if cancelled > 0 {
            info!("Cancelled {} expired games", cancelled);
        }
        Ok(cancelled)
    }

    pub async fn start_game(&self, game_id: Uuid, user_id: Uuid) -> Result<(), GameError> {
        let _timer = GameCreationTimer::new("quick");

        let txn = self.db.begin().await?;

        // TODO: should be in a repository
        let game_model = game::Entity::find_by_id(game_id)
            .one(&txn)
            .await?
            .ok_or(GameError::GameNotFound)?;

        if game_model.status != GameStatus::Ready {
            txn.rollback().await.ok();
            return Err(GameError::GameNotReady);
        }
        if game_model.creator_id != Some(user_id) {
            txn.rollback().await.ok();
            return Err(GameError::NotCreator);
        }

        let players = player::Entity::find()
            .filter(player::Column::GameId.eq(game_id))
            .order_by_asc(player::Column::Position)
            .all(&txn)
            .await?;

        let num_players = players.len();
        if num_players < 2 {
            txn.rollback().await.ok();
            return Err(GameError::internal(
                "Not enough players to start".to_string(),
            ));
        }
        if num_players > game_model.max_players as usize {
            txn.rollback().await.ok();
            return Err(GameError::internal(format!(
                "Player count {} exceeds max_players {} for game {}",
                num_players, game_model.max_players, game_id
            )));
        }

        let player_ids: Vec<Uuid> = players.iter().map(|p| p.id).collect();

        let (card_models, cards) = crate::game::cards::build_game_cards(game_id, &players);
        game_card::Entity::insert_many(card_models)
            .exec(&txn)
            .await?;

        let special_hands: Vec<(Uuid, crate::game::special_cards::SpecialCards)> = players
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let offset = i * CARDS_PER_PLAYER;
                let hand = &cards[offset..offset + CARDS_PER_PLAYER];
                (p.id, compute_special_cards(hand))
            })
            .collect();

        let all_human = players
            .iter()
            .all(|p| matches!(p.player_type, PlayerType::Human));
        let askee = if all_human {
            select_askee(&special_hands)
        } else {
            None
        };

        let initial_rank = 0i32;
        let first_player_id = player_ids[0];

        // TODO: should be a function in a repository
        let now = chrono::Utc::now();
        let game_mode = game_model.game_mode.to_string();
        let mut game_active: game::ActiveModel = game_model.into();
        game_active.status = ActiveValue::Set(GameStatus::Active);
        game_active.rank = ActiveValue::Set(Some(initial_rank));
        game_active.roll = ActiveValue::Set(1);
        game_active.updated_at = ActiveValue::Set(now);
        game_active.pending_claim_player_id = ActiveValue::Set(askee);
        game_active.update(&txn).await?;

        txn.commit().await?;

        info!(
            "Game started: game_id={}, players={}, first_turn={}",
            game_id, num_players, first_player_id
        );

        let askee_specials = askee.and_then(|id| {
            special_hands
                .iter()
                .find(|(pid, _)| *pid == id)
                .map(|(_, s)| *s)
        });

        self.publish_game_start_events(
            game_id,
            &players,
            &player_ids,
            &cards,
            askee,
            askee_specials,
            game_mode,
            first_player_id,
        )
        .await;

        self.cache_game_state(game_id).await;

        Ok(())
    }
}
