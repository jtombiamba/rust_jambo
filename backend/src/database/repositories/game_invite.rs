use async_trait::async_trait;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter, Set,
};
use uuid::Uuid;

use crate::database::models::{game, game_invite, Game, InviteStatus};
use crate::database::traits::GameInviteRepoTrait;

pub struct GameInviteRepository {
    connection: DatabaseConnection,
}

#[allow(dead_code)]
impl GameInviteRepository {
    pub fn new(connection: DatabaseConnection) -> Self {
        Self { connection }
    }

    pub async fn create_invite(
        &self,
        game_id: Uuid,
        invited_user_id: Uuid,
    ) -> Result<game_invite::Model, DbErr> {
        let id = Uuid::new_v4();
        let now = chrono::Utc::now();

        let invite_active = game_invite::ActiveModel {
            id: Set(id),
            game_id: Set(game_id),
            invited_user_id: Set(invited_user_id),
            status: Set(InviteStatus::Pending),
            created_at: Set(now),
        };
        let insert_result = game_invite::Entity::insert(invite_active)
            .exec(&self.connection)
            .await?;
        let inserted_id = insert_result.last_insert_id;
        game_invite::Entity::find_by_id(inserted_id)
            .one(&self.connection)
            .await?
            .ok_or_else(|| DbErr::Custom("GameInvite not found after insertion".to_string()))
    }

    pub async fn find_invite(
        &self,
        game_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<game_invite::Model>, DbErr> {
        game_invite::Entity::find()
            .filter(game_invite::Column::GameId.eq(game_id))
            .filter(game_invite::Column::InvitedUserId.eq(user_id))
            .one(&self.connection)
            .await
    }

    pub async fn update_invite_status(
        &self,
        invite_id: Uuid,
        status: InviteStatus,
    ) -> Result<game_invite::Model, DbErr> {
        let model = game_invite::Entity::find_by_id(invite_id)
            .one(&self.connection)
            .await?
            .ok_or_else(|| DbErr::Custom("GameInvite not found".to_string()))?;
        let mut active: game_invite::ActiveModel = model.into();
        active.status = Set(status);
        active.update(&self.connection).await
    }

    pub async fn find_by_id(&self, invite_id: Uuid) -> Result<Option<game_invite::Model>, DbErr> {
        game_invite::Entity::find_by_id(invite_id)
            .one(&self.connection)
            .await
    }

    pub async fn list_pending_invites_for_user(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<(game_invite::Model, Game)>, DbErr> {
        let invites = game_invite::Entity::find()
            .filter(game_invite::Column::InvitedUserId.eq(user_id))
            .filter(game_invite::Column::Status.eq(InviteStatus::Pending))
            .all(&self.connection)
            .await?;

        let mut results = Vec::new();
        for invite in invites {
            if let Some(game) = game::Entity::find_by_id(invite.game_id)
                .one(&self.connection)
                .await?
            {
                results.push((invite, game));
            }
        }
        Ok(results)
    }
}

#[async_trait]
impl GameInviteRepoTrait for GameInviteRepository {
    async fn create_invite(
        &self,
        game_id: Uuid,
        invited_user_id: Uuid,
    ) -> Result<game_invite::Model, DbErr> {
        self.create_invite(game_id, invited_user_id).await
    }

    async fn find_invite(
        &self,
        game_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<game_invite::Model>, DbErr> {
        self.find_invite(game_id, user_id).await
    }

    async fn update_invite_status(
        &self,
        invite_id: Uuid,
        status: InviteStatus,
    ) -> Result<game_invite::Model, DbErr> {
        self.update_invite_status(invite_id, status).await
    }

    async fn list_pending_invites_for_user(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<(game_invite::Model, Game)>, DbErr> {
        self.list_pending_invites_for_user(user_id).await
    }
}
