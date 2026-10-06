use uuid::Uuid;

use crate::api::dto::responses::{
    ActiveRunResponse, CreateRunResponse, CurrentGameResponse, JoinRunResponse, RoomDetailResponse,
    RoomListItem, RunListItem, StartNextGameResponse,
};
use crate::room::error::RoomServiceError;

/// Trait seam for the room service, enabling handler-level testing.
#[async_trait::async_trait]
pub trait RoomServiceTrait: Send + Sync {
    async fn create_room(
        &self,
        user_id: Uuid,
        name: &str,
    ) -> Result<crate::database::models::room::Model, RoomServiceError>;

    async fn list_user_rooms(&self, user_id: Uuid) -> Result<Vec<RoomListItem>, RoomServiceError>;

    async fn get_room_detail(
        &self,
        room_id: Uuid,
        user_id: Uuid,
    ) -> Result<RoomDetailResponse, RoomServiceError>;

    async fn join_room(
        &self,
        user_id: Uuid,
        invitation_code: &str,
    ) -> Result<crate::database::models::room::Model, RoomServiceError>;

    async fn invite_to_room(
        &self,
        room_id: Uuid,
        user_id: Uuid,
        email: &str,
    ) -> Result<(), RoomServiceError>;

    async fn leave_room(&self, room_id: Uuid, user_id: Uuid) -> Result<(), RoomServiceError>;

    async fn create_run(
        &self,
        room_id: Uuid,
        user_id: Uuid,
        num_games: i32,
        bet: i32,
        player_ids: &[Uuid],
    ) -> Result<CreateRunResponse, RoomServiceError>;

    async fn join_run(
        &self,
        run_id: Uuid,
        user_id: Uuid,
    ) -> Result<JoinRunResponse, RoomServiceError>;

    async fn leave_run(&self, run_id: Uuid, user_id: Uuid) -> Result<(), RoomServiceError>;

    async fn get_active_run(
        &self,
        room_id: Uuid,
        user_id: Uuid,
    ) -> Result<ActiveRunResponse, RoomServiceError>;

    async fn start_next_game(
        &self,
        run_id: Uuid,
        user_id: Uuid,
    ) -> Result<StartNextGameResponse, RoomServiceError>;

    async fn get_current_game(
        &self,
        run_id: Uuid,
        user_id: Uuid,
    ) -> Result<CurrentGameResponse, RoomServiceError>;

    async fn list_runs(
        &self,
        room_id: Uuid,
        user_id: Uuid,
    ) -> Result<Vec<RunListItem>, RoomServiceError>;
}
