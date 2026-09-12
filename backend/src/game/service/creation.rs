use futures::future::BoxFuture;
use sea_orm::{DatabaseTransaction, TransactionTrait};
use std::collections::HashMap;
use uuid::Uuid;

use crate::database::models::GameStatus;
use crate::database::repositories::{GameRepository, PlayerProfileRepository, PlayerRepository};
use crate::error::GameError;
use crate::game::service::types::MultiplayerGameOutcome;

use super::credit::{credit_update_result, CreditCalculator};
use super::validation::{load_and_validate_profile, validate_sufficient_credit};
use super::GameService;

impl GameService {
    async fn with_txn<T>(
        &self,
        f: impl for<'a> FnOnce(&'a DatabaseTransaction) -> BoxFuture<'a, Result<T, GameError>>,
    ) -> Result<T, GameError> {
        let txn = self.db.begin().await?;
        match f(&txn).await {
            Ok(value) => {
                txn.commit().await?;
                Ok(value)
            }
            Err(err) => {
                txn.rollback().await.ok();
                Err(err)
            }
        }
    }

    #[tracing::instrument(
        skip(self),
        fields(user_id = ?creator_user_id, bet = bet, max_players = max_players)
    )]
    pub async fn create_multiplayer_game(
        &self,
        creator_user_id: Uuid,
        creator_pseudo: &str,
        bet: i32,
        max_players: i16,
    ) -> Result<MultiplayerGameOutcome, GameError> {
        const INVITE_TIMEOUT_MINUTES: i64 = 6;

        let profile = load_and_validate_profile(&self.db, creator_user_id).await?;
        validate_sufficient_credit(&profile, bet)?;

        let calculator =
            CreditCalculator::new(self.freeze_duration_secs, self.unfreeze_credit_no_payment);
        let now = chrono::Utc::now();
        let credit_result = calculator.compute_joining_credit(&profile, bet, now);
        let read_credit = profile.credit;

        let game_id = Uuid::now_v7();
        let player_id = Uuid::now_v7();
        let expires_at = now + chrono::Duration::minutes(INVITE_TIMEOUT_MINUTES);

        let player_positions: HashMap<i32, Uuid> = HashMap::from([(0, creator_user_id)]);
        let player_positions_json = serde_json::to_value(player_positions).map_err(|e| {
            GameError::internal(format!("Failed to serialize player_positions: {}", e))
        })?;

        let db = self.db.clone();
        let creator_pseudo = creator_pseudo.to_string();

        self.with_txn(|txn| {
            let game_repo = GameRepository::new(db.clone());
            let player_repo = PlayerRepository::new(db.clone());
            let profile_repo = PlayerProfileRepository::new(db);
            let creator_pseudo = creator_pseudo.clone();
            Box::pin(async move {
                let rows = profile_repo
                    .update_credit_optimistic_in_txn(
                        txn,
                        creator_user_id,
                        credit_result.final_credit,
                        credit_result.frozen_until,
                        read_credit,
                        now,
                    )
                    .await?;
                credit_update_result(rows)?;

                game_repo
                    .create_multiplayer_in_txn(
                        txn,
                        game_id,
                        bet,
                        creator_user_id,
                        max_players,
                        expires_at,
                        player_positions_json,
                    )
                    .await?;

                if let Err(e) = player_repo
                    .create_with_user_in_txn(
                        txn,
                        player_id,
                        game_id,
                        creator_user_id,
                        &creator_pseudo,
                        0,
                        credit_result.final_credit,
                    )
                    .await
                {
                    if super::is_unique_violation(&e) {
                        return Err(GameError::AlreadyJoined);
                    }
                    return Err(GameError::Database(e));
                }

                Ok(())
            })
        })
        .await?;

        Ok(MultiplayerGameOutcome {
            game_id,
            player_id,
            status: GameStatus::Pending,
            bet,
            max_players,
            invite_expires_at: expires_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::models::player_profile;
    use chrono::Utc;
    use sea_orm::{DatabaseBackend, MockDatabase, MockExecResult};

    fn make_profile(
        user_id: Uuid,
        credit: i32,
        frozen_until: Option<chrono::DateTime<Utc>>,
    ) -> player_profile::Model {
        player_profile::Model {
            id: Uuid::now_v7(),
            user_id,
            player_type: crate::database::models::PlayerType::Human,
            credit,
            game_played: 0,
            wins: 0,
            kora_wins: 0,
            winning_streak: 0,
            latitude: None,
            longitude: None,
            country_code: None,
            city: None,
            frozen_until,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn make_service(
        profile: Option<player_profile::Model>,
        exec_results: Vec<MockExecResult>,
    ) -> GameService {
        let mut db = MockDatabase::new(DatabaseBackend::Postgres);
        db = match profile {
            Some(p) => db.append_query_results(vec![vec![p]]),
            None => db.append_query_results(vec![Vec::<player_profile::Model>::new()]),
        };
        if !exec_results.is_empty() {
            db = db.append_exec_results(exec_results);
        }
        GameService::new(db.into_connection())
    }

    fn success_exec_results() -> Vec<MockExecResult> {
        vec![
            MockExecResult {
                last_insert_id: 1,
                rows_affected: 1,
            },
            MockExecResult {
                last_insert_id: 2,
                rows_affected: 1,
            },
            MockExecResult {
                last_insert_id: 3,
                rows_affected: 1,
            },
        ]
    }

    #[tokio::test]
    async fn test_create_multiplayer_game_success() {
        let user_id = Uuid::now_v7();
        let service = make_service(
            Some(make_profile(user_id, 500, None)),
            success_exec_results(),
        );

        let outcome = service
            .create_multiplayer_game(user_id, "alice", 100, 4)
            .await
            .unwrap();

        assert_eq!(outcome.status, GameStatus::Pending);
        assert_eq!(outcome.bet, 100);
        assert_eq!(outcome.max_players, 4);
        assert_eq!(outcome.player_id, outcome.player_id);
        assert!(outcome.invite_expires_at > Utc::now());
    }

    #[tokio::test]
    async fn test_create_multiplayer_game_conflict() {
        let user_id = Uuid::now_v7();
        let service = make_service(
            Some(make_profile(user_id, 500, None)),
            vec![MockExecResult {
                last_insert_id: 1,
                rows_affected: 0,
            }],
        );

        let result = service
            .create_multiplayer_game(user_id, "alice", 100, 4)
            .await;
        assert!(matches!(result, Err(GameError::VersionConflict)));
    }

    #[tokio::test]
    async fn test_create_multiplayer_game_insufficient_credit() {
        let user_id = Uuid::now_v7();
        let service = make_service(Some(make_profile(user_id, 100, None)), vec![]);

        let result = service
            .create_multiplayer_game(user_id, "alice", 100, 4)
            .await;
        assert!(matches!(result, Err(GameError::InsufficientCredits { .. })));
    }

    #[tokio::test]
    async fn test_create_multiplayer_game_account_frozen() {
        let user_id = Uuid::now_v7();
        let frozen_until = Utc::now() + chrono::Duration::hours(1);
        let service = make_service(Some(make_profile(user_id, 500, Some(frozen_until))), vec![]);

        let result = service
            .create_multiplayer_game(user_id, "alice", 100, 4)
            .await;
        assert!(matches!(result, Err(GameError::AccountFrozen { .. })));
    }

    #[tokio::test]
    async fn test_create_multiplayer_game_profile_not_found() {
        let service = make_service(None, vec![]);

        let result = service
            .create_multiplayer_game(Uuid::now_v7(), "alice", 100, 4)
            .await;
        assert!(matches!(result, Err(GameError::ProfileNotFound)));
    }
}
