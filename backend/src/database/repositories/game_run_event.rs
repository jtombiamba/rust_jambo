use sea_orm::{DatabaseConnection, EntityTrait, Set};
use uuid::Uuid;

use crate::database::models::{game_run_event, GameRunEvent};
use crate::database::traits::GameRunEventRepoTrait;

#[derive(Debug, Clone)]
pub struct GameRunEventRepository {
    connection: DatabaseConnection,
}

impl GameRunEventRepository {
    pub fn new(connection: DatabaseConnection) -> Self {
        Self { connection }
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn log(
        &self,
        run_id: Uuid,
        user_id: Option<Uuid>,
        event_type: &str,
        data: Option<&str>,
    ) -> Result<GameRunEvent, sea_orm::DbErr> {
        let now = chrono::Utc::now();
        let active = game_run_event::ActiveModel {
            id: Set(Uuid::now_v7()),
            game_run_id: Set(run_id),
            user_id: Set(user_id),
            event_type: Set(event_type.to_string()),
            data: Set(data.map(|s| s.to_string())),
            created_at: Set(now),
        };
        let result = game_run_event::Entity::insert(active)
            .exec(&self.connection)
            .await?;
        let event = game_run_event::Entity::find_by_id(result.last_insert_id)
            .one(&self.connection)
            .await?
            .ok_or_else(|| {
                sea_orm::DbErr::Custom("GameRunEvent not found after insert".to_string())
            })?;
        Ok(event)
    }
}

#[async_trait::async_trait]
impl GameRunEventRepoTrait for GameRunEventRepository {
    async fn log(
        &self,
        run_id: Uuid,
        user_id: Option<Uuid>,
        event_type: &str,
        data: Option<&str>,
    ) -> Result<GameRunEvent, sea_orm::DbErr> {
        self.log(run_id, user_id, event_type, data).await
    }
}
