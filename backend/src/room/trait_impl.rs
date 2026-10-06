use uuid::Uuid;

use crate::api::dto::responses::{
    ActiveRunResponse, CreateRunResponse, CurrentGameResponse, JoinRunResponse, RoomDetailResponse,
    RoomListItem, RunListItem, StartNextGameResponse,
};
use crate::room::error::RoomServiceError;
use crate::room::service::RoomService;
use crate::room::service_trait::RoomServiceTrait;

#[async_trait::async_trait]
impl RoomServiceTrait for RoomService {
    async fn create_room(
        &self,
        user_id: Uuid,
        name: &str,
    ) -> Result<crate::database::models::room::Model, RoomServiceError> {
        RoomService::create_room(self, user_id, name).await
    }

    async fn list_user_rooms(&self, user_id: Uuid) -> Result<Vec<RoomListItem>, RoomServiceError> {
        RoomService::list_user_rooms(self, user_id).await
    }

    async fn get_room_detail(
        &self,
        room_id: Uuid,
        user_id: Uuid,
    ) -> Result<RoomDetailResponse, RoomServiceError> {
        RoomService::get_room_detail(self, room_id, user_id).await
    }

    async fn join_room(
        &self,
        user_id: Uuid,
        invitation_code: &str,
    ) -> Result<crate::database::models::room::Model, RoomServiceError> {
        RoomService::join_room(self, user_id, invitation_code).await
    }

    async fn invite_to_room(
        &self,
        room_id: Uuid,
        user_id: Uuid,
        email: &str,
    ) -> Result<(), RoomServiceError> {
        RoomService::invite_to_room(self, room_id, user_id, email).await
    }

    async fn leave_room(&self, room_id: Uuid, user_id: Uuid) -> Result<(), RoomServiceError> {
        RoomService::leave_room(self, room_id, user_id).await
    }

    async fn create_run(
        &self,
        room_id: Uuid,
        user_id: Uuid,
        num_games: i32,
        bet: i32,
        player_ids: &[Uuid],
    ) -> Result<CreateRunResponse, RoomServiceError> {
        RoomService::create_run(self, room_id, user_id, num_games, bet, player_ids).await
    }

    async fn join_run(
        &self,
        run_id: Uuid,
        user_id: Uuid,
    ) -> Result<JoinRunResponse, RoomServiceError> {
        RoomService::join_run(self, run_id, user_id).await
    }

    async fn leave_run(&self, run_id: Uuid, user_id: Uuid) -> Result<(), RoomServiceError> {
        RoomService::leave_run(self, run_id, user_id).await
    }

    async fn get_active_run(
        &self,
        room_id: Uuid,
        user_id: Uuid,
    ) -> Result<ActiveRunResponse, RoomServiceError> {
        RoomService::get_active_run(self, room_id, user_id).await
    }

    async fn start_next_game(
        &self,
        run_id: Uuid,
        user_id: Uuid,
    ) -> Result<StartNextGameResponse, RoomServiceError> {
        RoomService::start_next_game(self, run_id, user_id).await
    }

    async fn get_current_game(
        &self,
        run_id: Uuid,
        user_id: Uuid,
    ) -> Result<CurrentGameResponse, RoomServiceError> {
        RoomService::get_current_game(self, run_id, user_id).await
    }

    async fn list_runs(
        &self,
        room_id: Uuid,
        user_id: Uuid,
    ) -> Result<Vec<RunListItem>, RoomServiceError> {
        RoomService::list_runs(self, room_id, user_id).await
    }
}
