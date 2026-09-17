use rand::RngExt;
use sea_orm::{ActiveModelTrait, ActiveValue, DatabaseConnection, TransactionTrait};
use uuid::Uuid;

use crate::api::dto::responses::PlayerInfoDto;
use crate::database::models::PlayerType;
use crate::database::repositories::{
    GameCardRepository, GameRepository, PlayerProfileRepository, PlayerRepository,
    QuickGamePlayerRow,
};
use crate::error::GameError;
use crate::game::distribution::distribute_cards;
use crate::game::service::types::QuickGameOutcome;
use crate::observability::{metrics, CorrelationId};

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
        let initial_rank = rand::rng().random_range(0..4) as i32;

        let game_repo = GameRepository::new(self.db.clone());
        let player_repo = PlayerRepository::new(self.db.clone());
        let card_repo = GameCardRepository::new(self.db.clone());

        game_repo
            .create_quick_game_in_txn(txn, game_id, now, initial_rank, human_user_id, step_by_step)
            .await?;

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
            player_rows.push(QuickGamePlayerRow {
                name: name.to_string(),
                position,
                player_type,
                credits,
                user_id,
            });
        }
        player_repo
            .create_quick_game_players_in_txn(txn, game_id, player_rows)
            .await?;

        let players = player_repo.list_by_game_in_txn(txn, game_id).await?;
        let player_ids: Vec<Uuid> = players.iter().map(|p| p.id).collect();

        let card_assignments = distribute_cards(&player_ids);
        card_repo
            .create_quick_game_cards_in_txn(txn, game_id, card_assignments.clone())
            .await?;

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
        metrics::ACTIVE_GAMES.inc();

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
