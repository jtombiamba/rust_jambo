use rand::Rng;
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
    QueryOrder, Set, TransactionTrait,
};
use serde_json::json;
use uuid::Uuid;

use crate::api::dto::responses::PlayerInfoDto;
use crate::database::models::{self, GameStatus, PlayerType};
use crate::database::repositories::PlayerProfileRepository;
use crate::error::GameError;
use crate::game::distribution::distribute_cards;
use crate::game::service::types::QuickGameOutcome;
use crate::observability::CorrelationId;

use super::GameService;

impl GameService {
    // pub async fn create_quick_game_for_user(
    //     &self,
    //     user_id: Uuid,
    //     _db: &DatabaseConnection,
    // ) -> Result<QuickGameOutcome, GameError> {
    //     self.create_quick_game_for_user_with_step_by_step(user_id, _db, false)
    //         .await
    // }

    pub async fn create_quick_game_for_user_with_step_by_step(
        &self,
        user_id: Uuid,
        _db: &DatabaseConnection,
        step_by_step: bool,
    ) -> Result<QuickGameOutcome, GameError> {
        const SOLO_BET: i32 = 10;
        let profile_repo = PlayerProfileRepository::new(self.db.clone());

        let txn = self.db.begin().await?;

        let profile = profile_repo
            .find_by_user_id(user_id)
            .await?
            .ok_or(GameError::ProfileNotFound)?;

        if let Some(frozen_until) = profile.frozen_until {
            if frozen_until > chrono::Utc::now() {
                txn.rollback().await.ok();
                return Err(GameError::AccountFrozen {
                    until: frozen_until.to_rfc3339(),
                });
            }
        }

        if profile.credit < SOLO_BET {
            txn.rollback().await.ok();
            return Err(GameError::InsufficientCredits {
                required: SOLO_BET,
                current: profile.credit,
            });
        }

        let new_credit = profile.credit - SOLO_BET;
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

        let mut profile_active: crate::database::models::player_profile::ActiveModel =
            profile.into();
        profile_active.credit = ActiveValue::Set(final_credit);
        profile_active.frozen_until = ActiveValue::Set(frozen_until);
        profile_active.updated_at = ActiveValue::Set(chrono::Utc::now());
        profile_active.update(&txn).await?;

        let outcome = self
            .create_quick_game_in_txn(&txn, true, Some(user_id), final_credit, step_by_step)
            .await?;

        txn.commit().await?;

        if was_previously_frozen && frozen_until.is_none() {
            let _ = self.send_unfreeze_email(user_id).await;
        }

        self.schedule_first_bot_if_needed(&outcome).await;

        Ok(outcome)
    }

    async fn schedule_first_bot_if_needed(&self, outcome: &QuickGameOutcome) {
        if outcome.step_by_step {
            return;
        }
        let first_bot = outcome
            .players
            .iter()
            .find(|p| p.position == outcome.current_turn && p.player_type == "bot");
        if let Some(bot) = first_bot {
            let game_id = outcome.game_id;
            let bot_id = bot.id;
            tracing::info!(
                "First player is bot (position {}), scheduling initial move",
                outcome.current_turn
            );
            if let Some(ref bs) = self.bot_scheduler {
                bs.schedule_if_next_bot(game_id, bot_id, None).await;
            } else {
                tracing::warn!(
                    "Bot scheduler unavailable: first player is bot but no scheduler configured, game {} will stall",
                    game_id
                );
            }
        }
    }

    async fn create_quick_game_in_txn(
        &self,
        txn: &sea_orm::DatabaseTransaction,
        with_human: bool,
        human_user_id: Option<Uuid>,
        human_credits: i32,
        step_by_step: bool,
    ) -> Result<QuickGameOutcome, GameError> {
        let game_id = Uuid::now_v7();
        let now = chrono::Utc::now();
        let initial_rank = rand::thread_rng().gen_range(0..4) as i32;

        let game_active = models::game::ActiveModel {
            id: Set(game_id),
            status: Set(GameStatus::Active),
            bet: Set(10),
            created_at: Set(now),
            updated_at: Set(now),
            finished_at: ActiveValue::NotSet,
            rank: Set(Some(initial_rank)),
            roll: Set(1),
            auto: Set(false),
            winner_id: ActiveValue::NotSet,
            player_positions: Set(json!({})),
            current_winning_card: ActiveValue::NotSet,
            current_winning_player_position: ActiveValue::NotSet,
            creator_id: Set(human_user_id),
            game_mode: Set(models::GameMode::Solo),
            max_players: Set(4),
            invite_expires_at: ActiveValue::NotSet,
            stall_warning_sent_at: ActiveValue::NotSet,
            game_run_id: ActiveValue::NotSet,
            step_by_step: Set(step_by_step),
            kicked_players: Set(json!([])),
        };
        models::game::Entity::insert(game_active).exec(txn).await?;

        let bot_names = ["Bot East", "Bot North", "Bot West"];
        let num_bots = 3;
        let total_players = if human_user_id.is_some() || with_human {
            1 + num_bots
        } else {
            4
        };
        let all_names: Vec<&str> = if human_user_id.is_some() || with_human {
            let mut names = vec!["You"];
            names.extend_from_slice(&bot_names);
            names
        } else {
            vec!["Bot South", "Bot East", "Bot North", "Bot West"]
        };

        let mut player_rows = Vec::with_capacity(total_players);
        for (i, name) in all_names.iter().enumerate() {
            let position = i as i32;
            let player_type = if i == 0 && (human_user_id.is_some() || with_human) {
                PlayerType::Human
            } else {
                PlayerType::Bot
            };
            let credits = if player_type == PlayerType::Human {
                human_credits
            } else {
                0
            };
            let user_id = if player_type == PlayerType::Human {
                human_user_id
            } else {
                None
            };
            player_rows.push(models::player::ActiveModel {
                id: Set(Uuid::now_v7()),
                game_id: Set(game_id),
                player_type: Set(player_type),
                name: Set(name.to_string()),
                position: Set(position),
                credits: Set(credits),
                created_at: Set(now),
                user_id: Set(user_id),
                kicked: Set(false),
                kicked_at: ActiveValue::NotSet,
            });
        }
        models::player::Entity::insert_many(player_rows)
            .exec(txn)
            .await?;

        let players = models::player::Entity::find()
            .filter(models::player::Column::GameId.eq(game_id))
            .order_by_asc(models::player::Column::Position)
            .all(txn)
            .await?;
        let player_ids: Vec<Uuid> = players.iter().map(|p| p.id).collect();

        let card_assignments = distribute_cards(&player_ids);
        let card_models: Vec<models::game_card::ActiveModel> = card_assignments
            .iter()
            .map(|&(player_id, card_index)| models::game_card::ActiveModel {
                id: Set(Uuid::now_v7()),
                game_id: Set(game_id),
                player_id: Set(Some(player_id)),
                card_index: Set(card_index),
                played: Set(false),
                played_at: ActiveValue::NotSet,
                round: Set(None),
                created_at: Set(now),
            })
            .collect();
        if !card_models.is_empty() {
            models::game_card::Entity::insert_many(card_models)
                .exec(txn)
                .await?;
        }

        let human_player = players
            .iter()
            .find(|p| p.user_id.is_some() || p.player_type.eq(&PlayerType::Human));
        let human_cards: Vec<i32> = if let Some(hp) = human_player {
            card_assignments
                .iter()
                .filter(|(pid, _)| *pid == hp.id)
                .map(|(_, card)| *card)
                .collect()
        } else {
            Vec::new()
        };

        let players_json: Vec<PlayerInfoDto> = players
            .iter()
            .map(|p| {
                let is_human = matches!(p.player_type, PlayerType::Human);
                PlayerInfoDto {
                    id: p.id,
                    player_type: if is_human {
                        "human".to_string()
                    } else {
                        "bot".to_string()
                    },
                    name: p.name.clone(),
                    position: p.position,
                    display_position: p.position,
                    cards: if is_human {
                        human_cards.clone()
                    } else {
                        Vec::new()
                    },
                    cards_count: 5,
                    is_current_user: is_human,
                }
            })
            .collect();

        Ok(QuickGameOutcome {
            game_id,
            players: players_json,
            status: "active".to_string(),
            current_turn: initial_rank,
            bet: 10,
            max_players: 4,
            invite_expires_at: None,
            deck_slots: None,
            ws_token: None,
            step_by_step,
        })
    }

    #[tracing::instrument(level = "info", skip(self), fields(correlation_id = %correlation_id.map(|c| c.to_string()).unwrap_or_default()))]
    pub async fn create_quick_game(
        &self,
        correlation_id: Option<CorrelationId>,
        step_by_step: bool,
    ) -> Result<QuickGameOutcome, GameError> {
        let txn = self.db.begin().await?;
        let outcome = self
            .create_quick_game_in_txn(&txn, true, None, 0, step_by_step)
            .await?;
        txn.commit().await?;

        self.schedule_first_bot_if_needed(&outcome).await;
        Ok(outcome)
    }

    #[allow(dead_code)]
    #[tracing::instrument(level = "info", skip(self))]
    pub async fn create_bot_only_game(&self) -> Result<QuickGameOutcome, GameError> {
        let txn = self.db.begin().await?;
        let outcome = self
            .create_quick_game_in_txn(&txn, false, None, 0, false)
            .await?;
        txn.commit().await?;

        self.schedule_first_bot_if_needed(&outcome).await;
        Ok(outcome)
    }
}
