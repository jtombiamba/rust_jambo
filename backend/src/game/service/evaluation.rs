use chrono::Utc;
use sea_orm::sea_query;
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, DatabaseTransaction, EntityTrait, QueryFilter,
    QueryOrder,
};
use std::time::Instant;
use tracing::info;
use uuid::Uuid;

use crate::database::models::{game, game_card, player, player_profile, GameStatus};
use crate::game::card_mapping::Card;
use crate::game::constants::CARDS_PER_PLAYER;
use crate::game::payment::calculate_payment;
use crate::game::round_evaluation::{evaluate_round, PlayedCard, RoundContext};
use crate::game::service::types::{GameServiceError, RoundEvalTimer, RoundEvaluationResult};

use super::GameService;

impl GameService {
    /// Check if all players have played a card in the given round.
    /// Uses the provided transaction connection to see uncommitted data.
    pub(crate) async fn is_round_complete_txn(
        &self,
        txn: &DatabaseTransaction,
        game_id: Uuid,
        round: i32,
    ) -> Result<bool, GameServiceError> {
        let players = player::Entity::find()
            .filter(player::Column::GameId.eq(game_id))
            .all(txn)
            .await
            .map_err(|e| {
                GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
            })?;

        for player_model in players {
            let cards = game_card::Entity::find()
                .filter(game_card::Column::PlayerId.eq(player_model.id))
                .all(txn)
                .await
                .map_err(|e| {
                    GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
                })?;
            let played_in_round = cards.iter().any(|c| c.played && c.round == Some(round));
            if !played_in_round {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Evaluate a completed round inside an active transaction.
    /// All DB writes use the transaction connection for atomicity.
    /// Returns RoundEvaluationResult for post-transaction event publishing.
    pub(crate) async fn evaluate_round_in_txn(
        &self,
        txn: &DatabaseTransaction,
        game_id: Uuid,
        round: i32,
    ) -> Result<RoundEvaluationResult, GameServiceError> {
        let _timer = RoundEvalTimer(Instant::now());
        let span = tracing::info_span!(
            "round_eval",
            game_id = %game_id,
            round = round,
        );
        let _guard = span.enter();

        // Fetch played cards for this round (via txn)
        let played_cards = game_card::Entity::find()
            .filter(game_card::Column::GameId.eq(game_id))
            .filter(game_card::Column::Round.eq(round))
            .filter(game_card::Column::Played.eq(true))
            .order_by_asc(game_card::Column::PlayedAt)
            .all(txn)
            .await
            .map_err(|e| {
                GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
            })?;
        info!(
            "evaluate_round_in_txn: found {} played cards for round {}",
            played_cards.len(),
            round
        );
        if played_cards.is_empty() {
            return Err(GameServiceError::RoundNotComplete);
        }

        // Fetch players (via txn)
        let players = player::Entity::find()
            .filter(player::Column::GameId.eq(game_id))
            .order_by_asc(player::Column::Position)
            .all(txn)
            .await
            .map_err(|e| {
                GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
            })?;
        let player_positions: Vec<Uuid> = players.iter().map(|p| p.id).collect();

        // Convert to PlayedCard structures
        let mut plays = Vec::new();
        for card in &played_cards {
            if let Some(player_id) = card.player_id {
                let position = player_positions
                    .iter()
                    .position(|&id| id == player_id)
                    .ok_or_else(|| {
                        GameServiceError::Internal("Player not found in game".to_string())
                    })?;
                let Ok(index) = u8::try_from(card.card_index) else {
                    continue;
                };
                if let Some(card_obj) = Card::new(index) {
                    plays.push(PlayedCard {
                        player_position: position,
                        card: card_obj,
                    });
                }
            }
        }

        let first_play = plays
            .first()
            .ok_or_else(|| GameServiceError::Internal("No plays in round".to_string()))?;
        let leading_card = Some(first_play.card);
        let leading_player_position = Some(first_play.player_position);

        for (index, value) in plays.iter().map(|p| (p.player_position, p.card)) {
            info!(
                " played in round {}: Index: {}, Value: {}",
                round, index, value.index
            );
        }
        if let Some(card) = leading_card {
            info!(
                " leading card for round {}: index {} (suit {})",
                round,
                card.index,
                card.index / 8
            );
        } else {
            info!(" no leading card for round {} (first round)", round);
        }

        let ctx = RoundContext {
            plays,
            leading_card,
            leading_player_position,
        };
        let round_result = evaluate_round(&ctx)
            .ok_or_else(|| GameServiceError::Internal("Round evaluation failed".to_string()))?;
        let winner_pos = round_result.winner_position;
        let winner_id = player_positions[winner_pos];

        let new_roll = round + 1;
        info!(
            "Round {} evaluated, winner is player {}, updating round to {}",
            round, winner_id, new_roll
        );

        let game_model = game::Entity::find_by_id(game_id)
            .one(txn)
            .await
            .map_err(|e| {
                GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
            })?
            .ok_or(GameServiceError::GameNotFound)?;

        let read_version = game_model.updated_at;

        let game_ends = new_roll > CARDS_PER_PLAYER as i32;
        let mut final_status = game_model.status;

        if game_ends {
            if round_result.is_kora {
                final_status = GameStatus::Kora;
            } else {
                final_status = GameStatus::Finished;
            }
        }

        // Build a single UPDATE query with optimistic locking.
        //
        // We use update_many() with col_expr() + an explicit ::game_status CAST for the
        // status column because:
        //
        // 1. Optimistic locking: update_many() lets us add `.filter(UpdatedAt.eq(read_version))`
        //    so that if another concurrent transaction modified this row between our read and
        //    write, the UPDATE affects 0 rows and we can retry. ActiveModel::update() only
        //    filters by primary key, silently overwriting concurrent changes.
        //
        // 2. Performance: a single UPDATE query for all columns is faster than splitting into
        //    two queries (update_many for non-enum cols + ActiveModel for the enum col).
        //
        // 3. The ::game_status CAST is required because col_expr() sends raw sea_orm::Value
        //    types to the database. Without the cast, Value::String(...) is sent as PostgreSQL
        //    `text`, which the `game_status` enum column rejects. The `::game_status` suffix
        //    tells PostgreSQL to interpret the string as the enum type.
        let mut update = game::Entity::update_many()
            .col_expr(
                game::Column::WinnerId,
                sea_query::Expr::value(sea_orm::Value::Uuid(Some(winner_id))),
            )
            .col_expr(
                game::Column::Rank,
                sea_query::Expr::value(sea_orm::Value::Int(Some(winner_pos as i32))),
            )
            .col_expr(
                game::Column::Roll,
                sea_query::Expr::value(sea_orm::Value::Int(Some(new_roll))),
            )
            .col_expr(
                game::Column::CurrentWinningCard,
                sea_query::Expr::value(sea_orm::Value::Int(None)),
            )
            .col_expr(
                game::Column::CurrentWinningPlayerPosition,
                sea_query::Expr::value(sea_orm::Value::Int(None)),
            )
            .col_expr(
                game::Column::UpdatedAt,
                sea_query::Expr::value(sea_orm::Value::ChronoDateTimeUtc(Some(Utc::now()))),
            )
            .filter(game::Column::Id.eq(game_id))
            .filter(game::Column::UpdatedAt.eq(read_version))
            .to_owned();

        if game_ends {
            // Cast the string to the PostgreSQL game_status enum so the type matches.
            // Without ::game_status, Value::String is sent as `text` which PostgreSQL rejects.
            let status_str = final_status.to_string();
            update = update.col_expr(
                game::Column::Status,
                sea_query::Expr::cust_with_values(
                    "$1::game_status",
                    [sea_orm::Value::String(Some(status_str))],
                ),
            );
        }

        let update_result = update.exec(txn).await.map_err(|e| {
            GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
        })?;

        if update_result.rows_affected == 0 {
            return Err(GameServiceError::VersionConflict);
        }

        Ok(RoundEvaluationResult {
            round,
            winner_id,
            winner_position: winner_pos,
            game_ended: game_ends,
            final_status,
            players,
        })
    }

    /// Process payment for a finished game inside an active transaction.
    pub(crate) async fn process_payment_in_txn(
        &self,
        txn: &DatabaseTransaction,
        game_id: Uuid,
        result: &RoundEvaluationResult,
    ) -> Result<(), GameServiceError> {
        let players = &result.players;
        let total_players = players.len();
        let winner_id = result.winner_id;
        let winner_position = players
            .iter()
            .position(|p| p.id == winner_id)
            .ok_or_else(|| GameServiceError::Internal("Winner not in player list".to_string()))?;

        let bet_multiplier = match result.final_status {
            GameStatus::Kora => 2,
            GameStatus::DoubleKora => 4,
            _ => 1,
        };

        let is_kora = matches!(
            result.final_status,
            GameStatus::Kora | GameStatus::DoubleKora
        );

        let game_model = game::Entity::find_by_id(game_id)
            .one(txn)
            .await
            .map_err(|e| {
                GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
            })?
            .ok_or(GameServiceError::GameNotFound)?;
        let bet = game_model.bet * bet_multiplier;

        let credits = calculate_payment(winner_position, total_players, bet);

        for (idx, player) in players.iter().enumerate() {
            let new_credits = player.credits + game_model.bet + credits[idx];
            let mut player_active: player::ActiveModel = player.clone().into();
            player_active.credits = ActiveValue::Set(new_credits);
            player_active.update(txn).await.map_err(|e| {
                GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
            })?;

            if let Some(user_id) = player.user_id {
                let profile = player_profile::Entity::find()
                    .filter(player_profile::Column::UserId.eq(user_id))
                    .one(txn)
                    .await
                    .map_err(|e| {
                        GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
                    })?;

                if let Some(profile_model) = profile {
                    let won = player.id == winner_id;
                    let was_frozen = profile_model.frozen_until.is_some();
                    let mut profile_active: player_profile::ActiveModel = profile_model.into();
                    profile_active.credit = ActiveValue::Set(new_credits);
                    profile_active.game_played =
                        ActiveValue::Set(profile_active.game_played.unwrap() + 1);
                    if won {
                        profile_active.wins = ActiveValue::Set(profile_active.wins.unwrap() + 1);
                        profile_active.winning_streak =
                            ActiveValue::Set(profile_active.winning_streak.unwrap() + 1);
                    } else {
                        profile_active.winning_streak = ActiveValue::Set(0);
                    }
                    if won && is_kora {
                        profile_active.kora_wins =
                            ActiveValue::Set(profile_active.kora_wins.unwrap() + 1);
                    }
                    if new_credits <= 0 {
                        profile_active.frozen_until = ActiveValue::Set(Some(
                            chrono::Utc::now() + chrono::Duration::seconds(3600_i64),
                        ));
                    } else if was_frozen {
                        profile_active.frozen_until = ActiveValue::Set(None);
                    }
                    profile_active.updated_at = ActiveValue::Set(chrono::Utc::now());
                    profile_active.update(txn).await.map_err(|e| {
                        GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
                    })?;
                }
            }
        }

        Ok(())
    }
}
