use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait,
    QueryFilter, Set,
};
use std::collections::HashMap;
use uuid::Uuid;

use crate::database::models::{
    game_run, game_run_event, game_run_game, game_run_player, room, room_member, GameRun,
    GameRunEvent, GameRunGame, GameRunPlayer, Room, RoomMember,
};

pub struct RoomRepository {
    connection: DatabaseConnection,
}

impl RoomRepository {
    pub fn new(connection: DatabaseConnection) -> Self {
        Self { connection }
    }

    pub async fn create(
        &self,
        creator_id: Uuid,
        name: &str,
        invitation_code: &str,
    ) -> Result<Room, sea_orm::DbErr> {
        let now = chrono::Utc::now();
        let active = room::ActiveModel {
            id: Set(Uuid::new_v4()),
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

    pub async fn find_by_id(&self, id: Uuid) -> Result<Option<Room>, sea_orm::DbErr> {
        room::Entity::find_by_id(id).one(&self.connection).await
    }

    pub async fn find_by_invitation_code(
        &self,
        code: &str,
    ) -> Result<Option<Room>, sea_orm::DbErr> {
        room::Entity::find()
            .filter(room::Column::InvitationCode.eq(code))
            .one(&self.connection)
            .await
    }

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

    pub async fn delete(&self, id: Uuid) -> Result<(), sea_orm::DbErr> {
        room::Entity::delete_by_id(id)
            .exec(&self.connection)
            .await?;
        Ok(())
    }
}

pub struct RoomMemberRepository {
    connection: DatabaseConnection,
}

impl RoomMemberRepository {
    pub fn new(connection: DatabaseConnection) -> Self {
        Self { connection }
    }

    pub async fn create(&self, room_id: Uuid, user_id: Uuid) -> Result<RoomMember, sea_orm::DbErr> {
        let now = chrono::Utc::now();
        let active = room_member::ActiveModel {
            id: Set(Uuid::new_v4()),
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

    pub async fn list_by_room(&self, room_id: Uuid) -> Result<Vec<RoomMember>, sea_orm::DbErr> {
        room_member::Entity::find()
            .filter(room_member::Column::RoomId.eq(room_id))
            .all(&self.connection)
            .await
    }

    pub async fn count_by_room(&self, room_id: Uuid) -> Result<usize, sea_orm::DbErr> {
        room_member::Entity::find()
            .filter(room_member::Column::RoomId.eq(room_id))
            .count(&self.connection)
            .await
            .map(|c| c as usize)
    }

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

    pub async fn remove(&self, room_id: Uuid, user_id: Uuid) -> Result<(), sea_orm::DbErr> {
        room_member::Entity::delete_many()
            .filter(room_member::Column::RoomId.eq(room_id))
            .filter(room_member::Column::UserId.eq(user_id))
            .exec(&self.connection)
            .await?;
        Ok(())
    }
}

pub struct GameRunRepository {
    connection: DatabaseConnection,
}

#[allow(dead_code)]
impl GameRunRepository {
    pub fn new(connection: DatabaseConnection) -> Self {
        Self { connection }
    }

    pub async fn create(
        &self,
        room_id: Uuid,
        created_by: Uuid,
        num_games: i32,
        bet_per_game: i32,
    ) -> Result<GameRun, sea_orm::DbErr> {
        let now = chrono::Utc::now();
        let active = game_run::ActiveModel {
            id: Set(Uuid::new_v4()),
            room_id: Set(room_id),
            num_games: Set(num_games),
            bet_per_game: Set(bet_per_game),
            num_players: Set(0),
            current_game_index: Set(0),
            status: Set("active".to_string()),
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

    pub async fn find_by_id(&self, id: Uuid) -> Result<Option<GameRun>, sea_orm::DbErr> {
        game_run::Entity::find_by_id(id).one(&self.connection).await
    }

    pub async fn find_active_by_room(
        &self,
        room_id: Uuid,
    ) -> Result<Option<GameRun>, sea_orm::DbErr> {
        game_run::Entity::find()
            .filter(game_run::Column::RoomId.eq(room_id))
            .filter(game_run::Column::Status.eq("active"))
            .one(&self.connection)
            .await
    }

    pub async fn list_by_room(&self, room_id: Uuid) -> Result<Vec<GameRun>, sea_orm::DbErr> {
        game_run::Entity::find()
            .filter(game_run::Column::RoomId.eq(room_id))
            .all(&self.connection)
            .await
    }

    pub async fn update_status(&self, id: Uuid, status: &str) -> Result<(), sea_orm::DbErr> {
        let model = game_run::Entity::find_by_id(id)
            .one(&self.connection)
            .await?;
        if let Some(model) = model {
            let mut active: game_run::ActiveModel = model.into();
            active.status = Set(status.to_string());
            active.updated_at = Set(chrono::Utc::now());
            active.update(&self.connection).await?;
        }
        Ok(())
    }

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

    pub async fn set_auto_start(
        &self,
        id: Uuid,
        auto_start_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), sea_orm::DbErr> {
        let model = game_run::Entity::find_by_id(id)
            .one(&self.connection)
            .await?;
        if let Some(model) = model {
            let mut active: game_run::ActiveModel = model.into();
            active.next_game_auto_start_at = Set(Some(auto_start_at));
            active.updated_at = Set(chrono::Utc::now());
            active.update(&self.connection).await?;
        }
        Ok(())
    }
}

pub struct GameRunPlayerRepository {
    connection: DatabaseConnection,
}

#[allow(dead_code)]
impl GameRunPlayerRepository {
    pub fn new(connection: DatabaseConnection) -> Self {
        Self { connection }
    }

    pub async fn create(
        &self,
        run_id: Uuid,
        user_id: Uuid,
        position: i32,
        provisioned_credits: i32,
    ) -> Result<GameRunPlayer, sea_orm::DbErr> {
        let now = chrono::Utc::now();
        let active = game_run_player::ActiveModel {
            id: Set(Uuid::new_v4()),
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

    pub async fn list_by_run(&self, run_id: Uuid) -> Result<Vec<GameRunPlayer>, sea_orm::DbErr> {
        game_run_player::Entity::find()
            .filter(game_run_player::Column::GameRunId.eq(run_id))
            .filter(game_run_player::Column::Kicked.eq(false))
            .all(&self.connection)
            .await
    }

    pub async fn list_all_by_run(
        &self,
        run_id: Uuid,
    ) -> Result<Vec<GameRunPlayer>, sea_orm::DbErr> {
        game_run_player::Entity::find()
            .filter(game_run_player::Column::GameRunId.eq(run_id))
            .all(&self.connection)
            .await
    }

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

    pub async fn remove(&self, run_id: Uuid, user_id: Uuid) -> Result<(), sea_orm::DbErr> {
        game_run_player::Entity::delete_many()
            .filter(game_run_player::Column::GameRunId.eq(run_id))
            .filter(game_run_player::Column::UserId.eq(user_id))
            .exec(&self.connection)
            .await?;
        Ok(())
    }
}

pub struct GameRunGameRepository {
    connection: DatabaseConnection,
}

#[allow(dead_code)]
impl GameRunGameRepository {
    pub fn new(connection: DatabaseConnection) -> Self {
        Self { connection }
    }

    pub async fn create(
        &self,
        run_id: Uuid,
        game_id: Uuid,
        game_index: i32,
    ) -> Result<GameRunGame, sea_orm::DbErr> {
        let now = chrono::Utc::now();
        let active = game_run_game::ActiveModel {
            id: Set(Uuid::new_v4()),
            game_run_id: Set(run_id),
            game_id: Set(game_id),
            game_index: Set(game_index),
            status: Set("active".to_string()),
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

    pub async fn list_by_run(&self, run_id: Uuid) -> Result<Vec<GameRunGame>, sea_orm::DbErr> {
        game_run_game::Entity::find()
            .filter(game_run_game::Column::GameRunId.eq(run_id))
            .all(&self.connection)
            .await
    }

    pub async fn update_status(
        &self,
        run_game_id: Uuid,
        status: &str,
    ) -> Result<(), sea_orm::DbErr> {
        let model = game_run_game::Entity::find_by_id(run_game_id)
            .one(&self.connection)
            .await?;
        if let Some(model) = model {
            let mut active: game_run_game::ActiveModel = model.into();
            active.status = Set(status.to_string());
            active.update(&self.connection).await?;
        }
        Ok(())
    }
}

pub struct GameRunEventRepository {
    connection: DatabaseConnection,
}

impl GameRunEventRepository {
    pub fn new(connection: DatabaseConnection) -> Self {
        Self { connection }
    }

    pub async fn log(
        &self,
        run_id: Uuid,
        user_id: Option<Uuid>,
        event_type: &str,
        data: Option<&str>,
    ) -> Result<GameRunEvent, sea_orm::DbErr> {
        let now = chrono::Utc::now();
        let active = game_run_event::ActiveModel {
            id: Set(Uuid::new_v4()),
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
