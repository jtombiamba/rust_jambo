use uuid::Uuid;

use crate::api::dto::responses::{CurrentGameResponse, StartNextGameResponse};
use crate::room::error::RoomServiceError;
use crate::room::service::RoomService;

impl RoomService {
    pub async fn get_current_game(
        &self,
        run_id: Uuid,
        user_id: Uuid,
    ) -> Result<CurrentGameResponse, RoomServiceError> {
        self.start_next_game_svc
            .get_current_game(run_id, user_id)
            .await
    }

    pub async fn start_next_game(
        &self,
        run_id: Uuid,
        user_id: Uuid,
    ) -> Result<StartNextGameResponse, RoomServiceError> {
        self.start_next_game_svc
            .start_next_game(run_id, user_id)
            .await
    }

    pub async fn list_runs(
        &self,
        room_id: Uuid,
        user_id: Uuid,
    ) -> Result<Vec<serde_json::Value>, RoomServiceError> {
        let member = self.member_repo.find_membership(room_id, user_id).await?;
        if member.is_none() {
            return Err(RoomServiceError::NotMember);
        }

        let runs = self.run_repo.list_by_room(room_id).await?;

        let result = runs
            .iter()
            .map(|r| {
                serde_json::json!({
                    "id": r.id,
                    "num_games": r.num_games,
                    "bet_per_game": r.bet_per_game,
                    "current_game_index": r.current_game_index,
                    "status": r.status.to_string(),
                    "created_at": r.created_at.to_rfc3339(),
                })
            })
            .collect();

        Ok(result)
    }
}
