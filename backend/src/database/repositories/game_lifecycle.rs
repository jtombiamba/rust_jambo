use sea_orm::{
    sea_query::Expr, ActiveValue, ColumnTrait, DatabaseTransaction, DbErr, EntityTrait,
    QueryFilter, Set, Value,
};
use serde_json::json;
use uuid::Uuid;

use crate::database::models::{game, Game, GameMode, GameStatus};

use super::game::GameRepository;

impl GameRepository {
    /// Build and insert the solo "quick game" `game` row inside an existing
    /// transaction. The caller owns `game_id`, `now` and `rank` so it can reuse
    /// them to drive the surrounding player/card inserts and the returned
    /// outcome without a round-trip.
    #[tracing::instrument(skip(txn), fields(db.statement, db.rows_affected))]
    #[allow(clippy::too_many_arguments)]
    pub async fn create_quick_game_in_txn(
        &self,
        txn: &DatabaseTransaction,
        game_id: Uuid,
        now: chrono::DateTime<chrono::Utc>,
        rank: i32,
        creator_id: Option<Uuid>,
        step_by_step: bool,
    ) -> Result<(), DbErr> {
        game::Entity::insert(game::ActiveModel {
            id: Set(game_id),
            status: Set(GameStatus::Active),
            bet: Set(10),
            created_at: Set(now),
            updated_at: Set(now),
            finished_at: ActiveValue::NotSet,
            rank: Set(Some(rank)),
            roll: Set(1),
            auto: Set(false),
            winner_id: ActiveValue::NotSet,
            player_positions: Set(json!({})),
            current_winning_card: ActiveValue::NotSet,
            current_winning_player_position: ActiveValue::NotSet,
            creator_id: Set(creator_id),
            game_mode: Set(GameMode::Solo),
            max_players: Set(4),
            invite_expires_at: ActiveValue::NotSet,
            stall_warning_sent_at: ActiveValue::NotSet,
            game_run_id: ActiveValue::NotSet,
            step_by_step: Set(step_by_step),
            kicked_players: Set(json!([])),
            pending_claim_player_id: ActiveValue::NotSet,
        })
        .exec_without_returning(txn)
        .await?;
        Ok(())
    }

    /// List pending multiplayer games whose invite window has already closed.
    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn list_expired_pending_multiplayer(
        &self,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<Game>, DbErr> {
        game::Entity::find()
            .filter(game::Column::Status.eq(GameStatus::Pending))
            .filter(game::Column::GameMode.eq(GameMode::Multiplayer))
            .filter(game::Column::InviteExpiresAt.lte(now))
            .all(&self.connection)
            .await
    }

    /// Transition a `Ready` game to `Active`, recording the initial turn and
    /// optional special-card claim pending state. The status write uses an
    /// explicit `::game_status` CAST because `col_expr` sends a raw
    /// `Value::String` that PostgreSQL otherwise interprets as `text`.
    #[tracing::instrument(skip(txn), fields(db.statement, db.rows_affected))]
    #[allow(clippy::too_many_arguments)]
    pub async fn mark_started_in_txn(
        &self,
        txn: &DatabaseTransaction,
        game_id: Uuid,
        rank: i32,
        roll: i32,
        now: chrono::DateTime<chrono::Utc>,
        pending_claim_player_id: Option<Uuid>,
    ) -> Result<(), DbErr> {
        game::Entity::update_many()
            .col_expr(
                game::Column::Status,
                Expr::cust_with_values(
                    "$1::game_status",
                    [Value::String(Some(GameStatus::Active.to_string()))],
                ),
            )
            .col_expr(game::Column::Rank, Expr::value(Value::Int(Some(rank))))
            .col_expr(game::Column::Roll, Expr::value(Value::Int(Some(roll))))
            .col_expr(
                game::Column::UpdatedAt,
                Expr::value(Value::ChronoDateTimeUtc(Some(now))),
            )
            .col_expr(
                game::Column::PendingClaimPlayerId,
                Expr::value(Value::Uuid(pending_claim_player_id)),
            )
            .filter(game::Column::Id.eq(game_id))
            .exec(txn)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm::{DatabaseBackend, MockDatabase, MockExecResult, TransactionTrait};

    fn now() -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::from_timestamp(1_700_000_000, 0).unwrap()
    }

    fn make_game(id: Uuid) -> Game {
        Game {
            id,
            status: GameStatus::Pending,
            bet: 10,
            created_at: now(),
            updated_at: now(),
            finished_at: None,
            rank: None,
            roll: 1,
            auto: false,
            winner_id: None,
            player_positions: json!({}),
            current_winning_card: None,
            current_winning_player_position: None,
            creator_id: None,
            game_mode: GameMode::Multiplayer,
            max_players: 4,
            invite_expires_at: Some(now() - chrono::Duration::minutes(1)),
            stall_warning_sent_at: None,
            game_run_id: None,
            step_by_step: false,
            kicked_players: json!([]),
            pending_claim_player_id: None,
        }
    }

    #[tokio::test]
    async fn create_quick_game_inserts_active_solo_game() {
        let db = MockDatabase::new(DatabaseBackend::Postgres)
            .append_exec_results(vec![MockExecResult {
                last_insert_id: 0,
                rows_affected: 1,
            }])
            .into_connection();
        let repo = GameRepository::new(db.clone());
        let txn = db.begin().await.unwrap();

        let result = repo
            .create_quick_game_in_txn(&txn, Uuid::now_v7(), now(), 2, Some(Uuid::now_v7()), true)
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn list_expired_pending_multiplayer_returns_expired_games() {
        let expired = make_game(Uuid::now_v7());
        let db = MockDatabase::new(DatabaseBackend::Postgres)
            .append_query_results(vec![vec![expired.clone()]])
            .into_connection();
        let repo = GameRepository::new(db);

        let games = repo.list_expired_pending_multiplayer(now()).await.unwrap();

        assert_eq!(games.len(), 1);
        assert_eq!(games[0].id, expired.id);
    }

    #[tokio::test]
    async fn list_expired_pending_multiplayer_empty_when_no_rows() {
        let db = MockDatabase::new(DatabaseBackend::Postgres)
            .append_query_results(vec![Vec::<Game>::new()])
            .into_connection();
        let repo = GameRepository::new(db);

        let games = repo.list_expired_pending_multiplayer(now()).await.unwrap();

        assert!(games.is_empty());
    }

    #[tokio::test]
    async fn mark_started_updates_game_to_active() {
        let db = MockDatabase::new(DatabaseBackend::Postgres)
            .append_exec_results(vec![MockExecResult {
                last_insert_id: 0,
                rows_affected: 1,
            }])
            .into_connection();
        let repo = GameRepository::new(db.clone());
        let txn = db.begin().await.unwrap();

        let result = repo
            .mark_started_in_txn(&txn, Uuid::now_v7(), 0, 1, now(), None)
            .await;

        assert!(result.is_ok());
    }
}
