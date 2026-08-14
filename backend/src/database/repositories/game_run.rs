use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, DatabaseConnection, DatabaseTransaction,
    EntityTrait, QueryFilter, Set,
};
use uuid::Uuid;

use crate::database::models::{game_run, GameRun, RunStatus};
use crate::database::traits::GameRunRepoTrait;

#[derive(Debug, Clone)]
pub struct GameRunRepository {
    connection: DatabaseConnection,
}

#[allow(dead_code)]
impl GameRunRepository {
    pub fn new(connection: DatabaseConnection) -> Self {
        Self { connection }
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn create(
        &self,
        room_id: Uuid,
        created_by: Uuid,
        num_games: i32,
        bet_per_game: i32,
    ) -> Result<GameRun, sea_orm::DbErr> {
        let now = chrono::Utc::now();
        let active = game_run::ActiveModel {
            id: Set(Uuid::now_v7()),
            room_id: Set(room_id),
            num_games: Set(num_games),
            bet_per_game: Set(bet_per_game),
            num_players: Set(0),
            current_game_index: Set(0),
            status: Set(RunStatus::Active),
            created_by: Set(created_by),
            next_game_auto_start_at: ActiveValue::NotSet,
            stall_warning_sent_at: ActiveValue::NotSet,
            stall_cancelled_at: ActiveValue::NotSet,
            created_at: Set(now),
            updated_at: Set(now),
        };
        let result = game_run::Entity::insert(active)
            .exec(&self.connection)
            .await?;
        let run = game_run::Entity::find_by_id(result.last_insert_id)
            .one(&self.connection)
            .await?
            .ok_or_else(|| sea_orm::DbErr::Custom("GameRun not found after insert".to_string()))?;
        Ok(run)
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn find_by_id(&self, id: Uuid) -> Result<Option<GameRun>, sea_orm::DbErr> {
        game_run::Entity::find_by_id(id).one(&self.connection).await
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn find_active_by_room(
        &self,
        room_id: Uuid,
    ) -> Result<Option<GameRun>, sea_orm::DbErr> {
        game_run::Entity::find()
            .filter(game_run::Column::RoomId.eq(room_id))
            .filter(game_run::Column::Status.eq(RunStatus::Active))
            .one(&self.connection)
            .await
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn list_by_room(&self, room_id: Uuid) -> Result<Vec<GameRun>, sea_orm::DbErr> {
        game_run::Entity::find()
            .filter(game_run::Column::RoomId.eq(room_id))
            .all(&self.connection)
            .await
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn update_status(&self, id: Uuid, status: RunStatus) -> Result<(), sea_orm::DbErr> {
        let model = game_run::Entity::find_by_id(id)
            .one(&self.connection)
            .await?;
        if let Some(model) = model {
            let mut active: game_run::ActiveModel = model.into();
            active.status = Set(status);
            active.updated_at = Set(chrono::Utc::now());
            active.update(&self.connection).await?;
        }
        Ok(())
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn update_game_index(&self, id: Uuid, index: i32) -> Result<(), sea_orm::DbErr> {
        let model = game_run::Entity::find_by_id(id)
            .one(&self.connection)
            .await?;
        if let Some(model) = model {
            let mut active: game_run::ActiveModel = model.into();
            active.current_game_index = Set(index);
            active.updated_at = Set(chrono::Utc::now());
            active.update(&self.connection).await?;
        }
        Ok(())
    }

    #[tracing::instrument(skip(txn), fields(db.statement, db.rows_affected))]
    pub async fn increment_game_index_in_txn(
        &self,
        txn: &DatabaseTransaction,
        run_id: Uuid,
        new_index: i32,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), sea_orm::DbErr> {
        use sea_orm::sea_query::Expr;
        game_run::Entity::update_many()
            .col_expr(game_run::Column::CurrentGameIndex, Expr::value(new_index))
            .col_expr(game_run::Column::UpdatedAt, Expr::value(now))
            .filter(game_run::Column::Id.eq(run_id))
            .exec(txn)
            .await?;
        Ok(())
    }

    #[tracing::instrument(skip(txn), fields(db.statement, db.rows_affected))]
    pub async fn update_status_in_txn(
        &self,
        txn: &DatabaseTransaction,
        run_id: Uuid,
        status: RunStatus,
    ) -> Result<(), sea_orm::DbErr> {
        use sea_orm::sea_query::Expr;
        game_run::Entity::update_many()
            .col_expr(game_run::Column::Status, Expr::value(status))
            .col_expr(game_run::Column::UpdatedAt, Expr::value(chrono::Utc::now()))
            .filter(game_run::Column::Id.eq(run_id))
            .exec(txn)
            .await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl GameRunRepoTrait for GameRunRepository {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<GameRun>, sea_orm::DbErr> {
        self.find_by_id(id).await
    }

    async fn list_by_room(&self, room_id: Uuid) -> Result<Vec<GameRun>, sea_orm::DbErr> {
        self.list_by_room(room_id).await
    }

    async fn find_active_by_room(&self, room_id: Uuid) -> Result<Option<GameRun>, sea_orm::DbErr> {
        self.find_active_by_room(room_id).await
    }

    async fn increment_game_index_in_txn(
        &self,
        txn: &DatabaseTransaction,
        run_id: Uuid,
        new_index: i32,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), sea_orm::DbErr> {
        self.increment_game_index_in_txn(txn, run_id, new_index, now)
            .await
    }

    async fn update_status_in_txn(
        &self,
        txn: &DatabaseTransaction,
        run_id: Uuid,
        status: RunStatus,
    ) -> Result<(), sea_orm::DbErr> {
        self.update_status_in_txn(txn, run_id, status).await
    }
}
