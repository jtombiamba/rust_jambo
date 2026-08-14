use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter,
    Set,
};
use std::collections::HashMap;
use uuid::Uuid;

use crate::database::models::{room, room_member, Room, RoomMember};

#[derive(Debug, Clone)]
pub struct RoomRepository {
    connection: DatabaseConnection,
}

impl RoomRepository {
    pub fn new(connection: DatabaseConnection) -> Self {
        Self { connection }
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn create(
        &self,
        creator_id: Uuid,
        name: &str,
        invitation_code: &str,
    ) -> Result<Room, sea_orm::DbErr> {
        let now = chrono::Utc::now();
        let active = room::ActiveModel {
            id: Set(Uuid::now_v7()),
            name: Set(name.to_string()),
            creator_id: Set(creator_id),
            invitation_code: Set(invitation_code.to_string()),
            created_at: Set(now),
            updated_at: Set(now),
        };
        let result = room::Entity::insert(active).exec(&self.connection).await?;
        let room = room::Entity::find_by_id(result.last_insert_id)
            .one(&self.connection)
            .await?
            .ok_or_else(|| sea_orm::DbErr::Custom("Room not found after insert".to_string()))?;
        Ok(room)
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn find_by_id(&self, id: Uuid) -> Result<Option<Room>, sea_orm::DbErr> {
        room::Entity::find_by_id(id).one(&self.connection).await
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn find_by_invitation_code(
        &self,
        code: &str,
    ) -> Result<Option<Room>, sea_orm::DbErr> {
        room::Entity::find()
            .filter(room::Column::InvitationCode.eq(code))
            .one(&self.connection)
            .await
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn list_by_user(&self, user_id: Uuid) -> Result<Vec<Room>, sea_orm::DbErr> {
        let member_room_ids: Vec<Uuid> = room_member::Entity::find()
            .filter(room_member::Column::UserId.eq(user_id))
            .all(&self.connection)
            .await?
            .iter()
            .map(|m| m.room_id)
            .collect();

        if member_room_ids.is_empty() {
            return Ok(vec![]);
        }

        room::Entity::find()
            .filter(room::Column::Id.is_in(member_room_ids))
            .all(&self.connection)
            .await
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn update_creator(
        &self,
        id: Uuid,
        new_creator_id: Uuid,
    ) -> Result<(), sea_orm::DbErr> {
        let model = room::Entity::find_by_id(id).one(&self.connection).await?;
        if let Some(model) = model {
            let mut active: room::ActiveModel = model.into();
            active.creator_id = Set(new_creator_id);
            active.updated_at = Set(chrono::Utc::now());
            active.update(&self.connection).await?;
        }
        Ok(())
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn delete(&self, id: Uuid) -> Result<(), sea_orm::DbErr> {
        room::Entity::delete_by_id(id)
            .exec(&self.connection)
            .await?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct RoomMemberRepository {
    connection: DatabaseConnection,
}

impl RoomMemberRepository {
    pub fn new(connection: DatabaseConnection) -> Self {
        Self { connection }
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn create(&self, room_id: Uuid, user_id: Uuid) -> Result<RoomMember, sea_orm::DbErr> {
        let now = chrono::Utc::now();
        let active = room_member::ActiveModel {
            id: Set(Uuid::now_v7()),
            room_id: Set(room_id),
            user_id: Set(user_id),
            joined_at: Set(now),
        };
        let result = room_member::Entity::insert(active)
            .exec(&self.connection)
            .await?;
        let member = room_member::Entity::find_by_id(result.last_insert_id)
            .one(&self.connection)
            .await?
            .ok_or_else(|| sea_orm::DbErr::Custom("Member not found after insert".to_string()))?;
        Ok(member)
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn find_membership(
        &self,
        room_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<RoomMember>, sea_orm::DbErr> {
        room_member::Entity::find()
            .filter(room_member::Column::RoomId.eq(room_id))
            .filter(room_member::Column::UserId.eq(user_id))
            .one(&self.connection)
            .await
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn list_by_room(&self, room_id: Uuid) -> Result<Vec<RoomMember>, sea_orm::DbErr> {
        room_member::Entity::find()
            .filter(room_member::Column::RoomId.eq(room_id))
            .all(&self.connection)
            .await
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn count_by_room(&self, room_id: Uuid) -> Result<usize, sea_orm::DbErr> {
        room_member::Entity::find()
            .filter(room_member::Column::RoomId.eq(room_id))
            .count(&self.connection)
            .await
            .map(|c| c as usize)
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn count_by_rooms(
        &self,
        room_ids: &[Uuid],
    ) -> Result<HashMap<Uuid, usize>, sea_orm::DbErr> {
        let mut map = HashMap::new();
        if room_ids.is_empty() {
            return Ok(map);
        }
        let members = room_member::Entity::find()
            .filter(room_member::Column::RoomId.is_in(room_ids.iter().copied()))
            .all(&self.connection)
            .await?;
        for member in &members {
            *map.entry(member.room_id).or_insert(0) += 1;
        }
        for &rid in room_ids {
            map.entry(rid).or_insert(0);
        }
        Ok(map)
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn remove(&self, room_id: Uuid, user_id: Uuid) -> Result<(), sea_orm::DbErr> {
        room_member::Entity::delete_many()
            .filter(room_member::Column::RoomId.eq(room_id))
            .filter(room_member::Column::UserId.eq(user_id))
            .exec(&self.connection)
            .await?;
        Ok(())
    }
}
