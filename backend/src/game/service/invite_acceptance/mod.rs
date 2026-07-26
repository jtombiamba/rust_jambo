mod credit;
mod events;
mod validation;

use std::collections::HashMap;

use sea_orm::{ActiveModelTrait, ActiveValue, EntityTrait, TransactionTrait};
use uuid::Uuid;

use crate::database::models::{
    game, game_invite, player, player_profile, GameStatus, InviteStatus, PlayerType,
};
use crate::error::GameError;
use crate::messaging::RedisClient;

use credit::CreditCalculator;
use events::{publish_game_ready_if_needed, publish_player_joined};
use validation::{
    load_and_validate_profile, validate_game_not_full, validate_game_pending,
    validate_not_already_in_game, validate_not_creator, validate_pending_invite_exists,
    validate_sufficient_credit,
};

use super::is_unique_violation;

pub(crate) struct AcceptInviteOrchestrator {
    db: sea_orm::DatabaseConnection,
    credit_calculator: CreditCalculator,
    redis_client: Option<RedisClient>,
}

impl AcceptInviteOrchestrator {
    pub(crate) fn new(
        db: sea_orm::DatabaseConnection,
        freeze_duration_secs: u64,
        unfreeze_credit_no_payment: i32,
        redis_client: Option<RedisClient>,
    ) -> Self {
        Self {
            db,
            credit_calculator: CreditCalculator::new(
                freeze_duration_secs,
                unfreeze_credit_no_payment,
            ),
            redis_client,
        }
    }

    pub(crate) async fn execute(
        &self,
        game_id: Uuid,
        user_id: Uuid,
        user_pseudo: &str,
    ) -> Result<player::Model, GameError> {
        let txn = self.db.begin().await?;

        let game_model = game::Entity::find_by_id(game_id)
            .one(&txn)
            .await?
            .ok_or(GameError::GameNotFound)?;

        validate_game_pending(&game_model)?;
        validate_not_creator(&game_model, user_id)?;
        validate_not_already_in_game(&txn, game_id, user_id).await?;

        let invite = validate_pending_invite_exists(&txn, game_id, user_id).await?;

        let max_players_val = game_model.max_players;
        let player_count = validate_game_not_full(&txn, game_id, max_players_val).await?;
        let next_position = player_count as i32;

        let bet = game_model.bet;
        let profile = load_and_validate_profile(&txn, user_id).await?;
        validate_sufficient_credit(&profile, bet)?;

        let now = chrono::Utc::now();
        let credit_result = self
            .credit_calculator
            .compute_joining_credit(&profile, bet, now);

        let mut profile_active: player_profile::ActiveModel = profile.into();
        profile_active.credit = ActiveValue::Set(credit_result.final_credit);
        profile_active.frozen_until = ActiveValue::Set(credit_result.frozen_until);
        profile_active.updated_at = ActiveValue::Set(now);
        profile_active.update(&txn).await?;

        let new_player_id = Uuid::now_v7();
        let player_active = player::ActiveModel {
            id: ActiveValue::Set(new_player_id),
            game_id: ActiveValue::Set(game_id),
            player_type: ActiveValue::Set(PlayerType::Human),
            name: ActiveValue::Set(user_pseudo.to_string()),
            position: ActiveValue::Set(next_position),
            credits: ActiveValue::Set(credit_result.final_credit),
            created_at: ActiveValue::Set(now),
            user_id: ActiveValue::Set(Some(user_id)),
            kicked: ActiveValue::Set(false),
            kicked_at: ActiveValue::NotSet,
        };
        if let Err(e) = player::Entity::insert(player_active).exec(&txn).await {
            txn.rollback().await.ok();
            if is_unique_violation(&e) {
                return Err(GameError::AlreadyJoined);
            }
            return Err(GameError::Database(e));
        }

        let mut invite_active: game_invite::ActiveModel = invite.into();
        invite_active.status = ActiveValue::Set(InviteStatus::Accepted);
        invite_active.update(&txn).await?;

        let current_positions: HashMap<i32, Uuid> = if game_model.player_positions.is_null() {
            HashMap::new()
        } else {
            serde_json::from_value(game_model.player_positions.clone()).map_err(|e| {
                GameError::internal(format!("Failed to parse player_positions: {}", e))
            })?
        };
        let mut updated_positions = current_positions;
        updated_positions.insert(next_position, user_id);

        let new_status = if (player_count + 1) >= max_players_val as u64 {
            GameStatus::Ready
        } else {
            GameStatus::Pending
        };

        let mut game_active: game::ActiveModel = game_model.into();
        game_active.player_positions =
            ActiveValue::Set(serde_json::to_value(&updated_positions).map_err(|e| {
                GameError::internal(format!("Failed to serialize player_positions: {}", e))
            })?);
        game_active.status = ActiveValue::Set(new_status);
        game_active.updated_at = ActiveValue::Set(now);
        game_active.update(&txn).await?;

        txn.commit().await?;

        publish_player_joined(
            &self.redis_client,
            game_id,
            new_player_id,
            user_id,
            user_pseudo,
            next_position,
            (player_count + 1) as i32,
            max_players_val as i32,
        )
        .await;

        publish_game_ready_if_needed(&self.redis_client, game_id, new_status).await;

        Ok(player::Model {
            id: new_player_id,
            game_id,
            player_type: PlayerType::Human,
            name: user_pseudo.to_string(),
            position: next_position,
            credits: credit_result.final_credit,
            created_at: now,
            user_id: Some(user_id),
            kicked: false,
            kicked_at: None,
        })
    }
}
