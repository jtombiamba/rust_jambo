use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, QueryFilter, TransactionTrait,
};
use serde_json::json;
use std::collections::HashMap;
use uuid::Uuid;

use crate::database::models::{game, player, player_profile, GameMode, GameStatus};
use crate::game::constants::KORA_CREDIT_MULTIPLIER;
use crate::game::service::types::{GameServiceError, MultiplayerGameOutcome};

use super::GameService;

impl GameService {
    pub async fn create_multiplayer_game(
        &self,
        creator_user_id: Uuid,
        creator_pseudo: &str,
        bet: i32,
        max_players: i16,
    ) -> Result<MultiplayerGameOutcome, GameServiceError> {
        const INVITE_TIMEOUT_MINUTES: i64 = 6;

        let txn = self.db.begin().await.map_err(|e| {
            GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
        })?;

        let profile = player_profile::Entity::find()
            .filter(player_profile::Column::UserId.eq(creator_user_id))
            .one(&txn)
            .await
            .map_err(|e| {
                GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
            })?
            .ok_or_else(|| GameServiceError::Internal("Player profile not found".to_string()))?;

        if let Some(frozen_until) = profile.frozen_until {
            if frozen_until > chrono::Utc::now() {
                txn.rollback().await.ok();
                return Err(GameServiceError::AccountFrozen {
                    until: frozen_until.to_rfc3339(),
                });
            }
        }

        let required_credit = bet * KORA_CREDIT_MULTIPLIER;
        if profile.credit < required_credit {
            txn.rollback().await.ok();
            return Err(GameServiceError::InsufficientCredits {
                required: required_credit,
                current: profile.credit,
            });
        }

        let creator_credit_before = profile.credit;
        let new_credit = creator_credit_before - bet;
        let freeze_duration = chrono::Duration::seconds(self.freeze_duration_secs as i64);
        let was_previously_frozen = profile.frozen_until.is_some();

        let (final_credit, frozen_until) = if new_credit <= 0 {
            (new_credit, Some(chrono::Utc::now() + freeze_duration))
        } else if was_previously_frozen {
            let auto_unfreeze_credit = if new_credit < self.unfreeze_credit_no_payment {
                self.unfreeze_credit_no_payment
            } else {
                new_credit
            };
            (auto_unfreeze_credit, None)
        } else {
            (new_credit, profile.frozen_until)
        };
        let mut profile_active: player_profile::ActiveModel = profile.into();
        profile_active.credit = ActiveValue::Set(final_credit);
        profile_active.frozen_until = ActiveValue::Set(frozen_until);
        profile_active.updated_at = ActiveValue::Set(chrono::Utc::now());
        profile_active.update(&txn).await.map_err(|e| {
            GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
        })?;

        let game_id = Uuid::new_v4();
        let now = chrono::Utc::now();
        let expires_at = now + chrono::Duration::minutes(INVITE_TIMEOUT_MINUTES);

        let game_active = game::ActiveModel {
            id: ActiveValue::Set(game_id),
            status: ActiveValue::Set(GameStatus::Pending),
            bet: ActiveValue::Set(bet),
            created_at: ActiveValue::Set(now),
            updated_at: ActiveValue::Set(now),
            finished_at: ActiveValue::NotSet,
            rank: ActiveValue::NotSet,
            roll: ActiveValue::Set(1),
            auto: ActiveValue::Set(false),
            winner_id: ActiveValue::NotSet,
            player_positions: ActiveValue::Set(json!({})),
            current_winning_card: ActiveValue::NotSet,
            current_winning_player_position: ActiveValue::NotSet,
            creator_id: ActiveValue::Set(Some(creator_user_id)),
            game_mode: ActiveValue::Set(GameMode::Multiplayer),
            max_players: ActiveValue::Set(max_players),
            invite_expires_at: ActiveValue::Set(Some(expires_at)),
        };
        let insert_result = game::Entity::insert(game_active)
            .exec(&txn)
            .await
            .map_err(|e| {
                GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
            })?;
        let inserted_game_id = insert_result.last_insert_id;

        let player_id = Uuid::new_v4();
        let player_active = player::ActiveModel {
            id: ActiveValue::Set(player_id),
            game_id: ActiveValue::Set(inserted_game_id),
            player_type: ActiveValue::Set(crate::database::models::PlayerType::Human),
            name: ActiveValue::Set(creator_pseudo.to_string()),
            position: ActiveValue::Set(0),
            credits: ActiveValue::Set(final_credit),
            created_at: ActiveValue::Set(now),
            user_id: ActiveValue::Set(Some(creator_user_id)),
        };
        player::Entity::insert(player_active)
            .exec(&txn)
            .await
            .map_err(|e| {
                GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
            })?;

        let player_positions: HashMap<i32, Uuid> = HashMap::from([(0, creator_user_id)]);
        let game_model = game::Entity::find_by_id(inserted_game_id)
            .one(&txn)
            .await
            .map_err(|e| {
                GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
            })?
            .ok_or_else(|| GameServiceError::Internal("Game not found after insert".to_string()))?;
        let mut game_active: game::ActiveModel = game_model.into();
        game_active.player_positions =
            ActiveValue::Set(serde_json::to_value(player_positions).map_err(|e| {
                GameServiceError::Internal(format!("Failed to serialize player_positions: {}", e))
            })?);
        game_active.update(&txn).await.map_err(|e| {
            GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
        })?;

        txn.commit().await.map_err(|e| {
            GameServiceError::Database(Box::new(e) as Box<dyn std::error::Error + Send>)
        })?;

        Ok(MultiplayerGameOutcome {
            game_id: inserted_game_id,
            player_id,
            status: GameStatus::Pending,
            bet,
            max_players,
            invite_expires_at: expires_at,
        })
    }
}
