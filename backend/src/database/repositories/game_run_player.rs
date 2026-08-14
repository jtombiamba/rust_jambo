use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, DatabaseTransaction, EntityTrait,
    QueryFilter, Set,
};
use uuid::Uuid;

use crate::database::models::{game_run_player, GameRunPlayer};
use crate::database::traits::GameRunPlayerRepoTrait;

#[derive(Debug, Clone)]
pub struct GameRunPlayerRepository {
    connection: DatabaseConnection,
}

#[allow(dead_code)]
impl GameRunPlayerRepository {
    pub fn new(connection: DatabaseConnection) -> Self {
        Self { connection }
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn create(
        &self,
        run_id: Uuid,
        user_id: Uuid,
        position: i32,
        provisioned_credits: i32,
    ) -> Result<GameRunPlayer, sea_orm::DbErr> {
        let now = chrono::Utc::now();
        let active = game_run_player::ActiveModel {
            id: Set(Uuid::now_v7()),
            game_run_id: Set(run_id),
            user_id: Set(user_id),
            position: Set(position),
            provisioned_credits: Set(provisioned_credits),
            kicked: Set(false),
            joined_at: Set(now),
        };
        let result = game_run_player::Entity::insert(active)
            .exec(&self.connection)
            .await?;
        let player = game_run_player::Entity::find_by_id(result.last_insert_id)
            .one(&self.connection)
            .await?
            .ok_or_else(|| {
                sea_orm::DbErr::Custom("GameRunPlayer not found after insert".to_string())
            })?;
        Ok(player)
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn list_by_run(&self, run_id: Uuid) -> Result<Vec<GameRunPlayer>, sea_orm::DbErr> {
        game_run_player::Entity::find()
            .filter(game_run_player::Column::GameRunId.eq(run_id))
            .filter(game_run_player::Column::Kicked.eq(false))
            .all(&self.connection)
            .await
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn list_all_by_run(
        &self,
        run_id: Uuid,
    ) -> Result<Vec<GameRunPlayer>, sea_orm::DbErr> {
        game_run_player::Entity::find()
            .filter(game_run_player::Column::GameRunId.eq(run_id))
            .all(&self.connection)
            .await
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn find_by_run_and_user(
        &self,
        run_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<GameRunPlayer>, sea_orm::DbErr> {
        game_run_player::Entity::find()
            .filter(game_run_player::Column::GameRunId.eq(run_id))
            .filter(game_run_player::Column::UserId.eq(user_id))
            .one(&self.connection)
            .await
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn deduct_provisioned(&self, id: Uuid, amount: i32) -> Result<i32, sea_orm::DbErr> {
        let model = game_run_player::Entity::find_by_id(id)
            .one(&self.connection)
            .await?;
        if let Some(model) = model {
            let new_credits = (model.provisioned_credits - amount).max(0);
            let mut active: game_run_player::ActiveModel = model.into();
            active.provisioned_credits = Set(new_credits);
            active.update(&self.connection).await?;
            Ok(new_credits)
        } else {
            Err(sea_orm::DbErr::Custom(
                "GameRunPlayer not found".to_string(),
            ))
        }
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn mark_kicked(&self, run_id: Uuid, user_id: Uuid) -> Result<(), sea_orm::DbErr> {
        let model = game_run_player::Entity::find()
            .filter(game_run_player::Column::GameRunId.eq(run_id))
            .filter(game_run_player::Column::UserId.eq(user_id))
            .one(&self.connection)
            .await?;
        if let Some(model) = model {
            let mut active: game_run_player::ActiveModel = model.into();
            active.kicked = Set(true);
            active.update(&self.connection).await?;
        }
        Ok(())
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn remove(&self, run_id: Uuid, user_id: Uuid) -> Result<(), sea_orm::DbErr> {
        game_run_player::Entity::delete_many()
            .filter(game_run_player::Column::GameRunId.eq(run_id))
            .filter(game_run_player::Column::UserId.eq(user_id))
            .exec(&self.connection)
            .await?;
        Ok(())
    }

    #[tracing::instrument(skip(txn), fields(db.statement, db.rows_affected))]
    pub async fn deduct_provisioned_in_txn(
        &self,
        txn: &DatabaseTransaction,
        id: Uuid,
        amount: i32,
    ) -> Result<(), sea_orm::DbErr> {
        use sea_orm::sea_query::ExprTrait;
        game_run_player::Entity::update_many()
            .col_expr(
                game_run_player::Column::ProvisionedCredits,
                sea_orm::sea_query::Expr::col(game_run_player::Column::ProvisionedCredits)
                    .sub(amount),
            )
            .filter(game_run_player::Column::Id.eq(id))
            .filter(game_run_player::Column::ProvisionedCredits.gte(amount))
            .exec(txn)
            .await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl GameRunPlayerRepoTrait for GameRunPlayerRepository {
    async fn find_by_run_and_user(
        &self,
        run_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<GameRunPlayer>, sea_orm::DbErr> {
        self.find_by_run_and_user(run_id, user_id).await
    }

    async fn list_by_run(&self, run_id: Uuid) -> Result<Vec<GameRunPlayer>, sea_orm::DbErr> {
        self.list_by_run(run_id).await
    }

    async fn deduct_provisioned_in_txn(
        &self,
        txn: &DatabaseTransaction,
        id: Uuid,
        amount: i32,
    ) -> Result<(), sea_orm::DbErr> {
        self.deduct_provisioned_in_txn(txn, id, amount).await
    }
}
