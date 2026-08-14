use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, DatabaseTransaction, EntityTrait,
    QueryFilter, Set,
};
use uuid::Uuid;

use crate::database::models::{game_run_game, GameRunGame, RunStatus};
use crate::database::traits::GameRunGameRepoTrait;

#[derive(Debug, Clone)]
pub struct GameRunGameRepository {
    connection: DatabaseConnection,
}

#[allow(dead_code)]
impl GameRunGameRepository {
    pub fn new(connection: DatabaseConnection) -> Self {
        Self { connection }
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn create(
        &self,
        run_id: Uuid,
        game_id: Uuid,
        game_index: i32,
    ) -> Result<GameRunGame, sea_orm::DbErr> {
        let now = chrono::Utc::now();
        let active = game_run_game::ActiveModel {
            id: Set(Uuid::now_v7()),
            game_run_id: Set(run_id),
            game_id: Set(game_id),
            game_index: Set(game_index),
            status: Set(RunStatus::Active),
            created_at: Set(now),
        };
        let result = game_run_game::Entity::insert(active)
            .exec(&self.connection)
            .await?;
        let rungame = game_run_game::Entity::find_by_id(result.last_insert_id)
            .one(&self.connection)
            .await?
            .ok_or_else(|| {
                sea_orm::DbErr::Custom("GameRunGame not found after insert".to_string())
            })?;
        Ok(rungame)
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn find_by_run_and_index(
        &self,
        run_id: Uuid,
        game_index: i32,
    ) -> Result<Option<GameRunGame>, sea_orm::DbErr> {
        game_run_game::Entity::find()
            .filter(game_run_game::Column::GameRunId.eq(run_id))
            .filter(game_run_game::Column::GameIndex.eq(game_index))
            .one(&self.connection)
            .await
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn list_by_run(&self, run_id: Uuid) -> Result<Vec<GameRunGame>, sea_orm::DbErr> {
        game_run_game::Entity::find()
            .filter(game_run_game::Column::GameRunId.eq(run_id))
            .all(&self.connection)
            .await
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn list_by_runs(&self, run_ids: &[Uuid]) -> Result<Vec<GameRunGame>, sea_orm::DbErr> {
        if run_ids.is_empty() {
            return Ok(vec![]);
        }
        game_run_game::Entity::find()
            .filter(game_run_game::Column::GameRunId.is_in(run_ids.iter().copied()))
            .all(&self.connection)
            .await
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn update_status(
        &self,
        run_game_id: Uuid,
        status: RunStatus,
    ) -> Result<(), sea_orm::DbErr> {
        let model = game_run_game::Entity::find_by_id(run_game_id)
            .one(&self.connection)
            .await?;
        if let Some(model) = model {
            let mut active: game_run_game::ActiveModel = model.into();
            active.status = Set(status);
            active.update(&self.connection).await?;
        }
        Ok(())
    }

    #[tracing::instrument(skip(txn), fields(db.statement, db.rows_affected))]
    pub async fn create_in_txn(
        &self,
        txn: &DatabaseTransaction,
        run_id: Uuid,
        game_id: Uuid,
        game_index: i32,
        status: RunStatus,
    ) -> Result<(), sea_orm::DbErr> {
        game_run_game::Entity::insert(game_run_game::ActiveModel {
            id: Set(Uuid::now_v7()),
            game_run_id: Set(run_id),
            game_id: Set(game_id),
            game_index: Set(game_index),
            status: Set(status),
            created_at: Set(chrono::Utc::now()),
        })
        .exec(txn)
        .await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl GameRunGameRepoTrait for GameRunGameRepository {
    async fn find_by_run_and_index(
        &self,
        run_id: Uuid,
        game_index: i32,
    ) -> Result<Option<GameRunGame>, sea_orm::DbErr> {
        self.find_by_run_and_index(run_id, game_index).await
    }

    async fn list_by_run(&self, run_id: Uuid) -> Result<Vec<GameRunGame>, sea_orm::DbErr> {
        self.list_by_run(run_id).await
    }

    async fn create_in_txn(
        &self,
        txn: &DatabaseTransaction,
        run_id: Uuid,
        game_id: Uuid,
        game_index: i32,
        status: RunStatus,
    ) -> Result<(), sea_orm::DbErr> {
        self.create_in_txn(txn, run_id, game_id, game_index, status)
            .await
    }
}
