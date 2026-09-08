use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use crate::api::dto::responses::{CurrentGameResponse, StartNextGameResponse};
use crate::database::models::{
    GameRun, GameRunGame, GameRunPlayer, GameStatus, Player, PlayerProfile, RunStatus,
};
use crate::database::traits::{
    GameCardRepoTrait, GameRepoTrait, GameRunEventRepoTrait, GameRunGameRepoTrait,
    GameRunPlayerRepoTrait, GameRunRepoTrait, PlayerProfileRepoTrait, PlayerRepoTrait,
    UserRepoTrait,
};
use crate::messaging::events::RoomEvent;
use crate::observability::metrics;
use crate::room::error::RoomServiceError;
use crate::room::event_publisher::RoomEventPublisher;
use crate::room::start_game_lock::StartGameLock;
use crate::room::transaction_runner::TransactionRunner;

/// Immutable inputs computed during the planning phase, consumed by the
/// transactional write phase.
struct GameCreationPlan {
    game_id: Uuid,
    player_ids: Vec<Uuid>,
    user_map: HashMap<Uuid, String>,
    profile_map: HashMap<Uuid, PlayerProfile>,
    run_player_map: HashMap<Uuid, GameRunPlayer>,
    shuffled_positions: Vec<usize>,
    position_map: HashMap<i32, Uuid>,
    bet: i32,
}

pub struct StartNextGameService {
    run_repo: Arc<dyn GameRunRepoTrait>,
    run_player_repo: Arc<dyn GameRunPlayerRepoTrait>,
    run_game_repo: Arc<dyn GameRunGameRepoTrait>,
    game_repo: Arc<dyn GameRepoTrait>,
    player_repo: Arc<dyn PlayerRepoTrait>,
    game_card_repo: Arc<dyn GameCardRepoTrait>,
    profile_repo: Arc<dyn PlayerProfileRepoTrait>,
    user_repo: Arc<dyn UserRepoTrait>,
    event_publisher: Arc<dyn RoomEventPublisher>,
    run_event_logger: Arc<dyn GameRunEventRepoTrait>,
    lock_service: Arc<dyn StartGameLock>,
    txn_runner: Arc<dyn TransactionRunner>,
}

impl StartNextGameService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        run_repo: Arc<dyn GameRunRepoTrait>,
        run_player_repo: Arc<dyn GameRunPlayerRepoTrait>,
        run_game_repo: Arc<dyn GameRunGameRepoTrait>,
        game_repo: Arc<dyn GameRepoTrait>,
        player_repo: Arc<dyn PlayerRepoTrait>,
        game_card_repo: Arc<dyn GameCardRepoTrait>,
        profile_repo: Arc<dyn PlayerProfileRepoTrait>,
        user_repo: Arc<dyn UserRepoTrait>,
        event_publisher: Arc<dyn RoomEventPublisher>,
        run_event_logger: Arc<dyn GameRunEventRepoTrait>,
        lock_service: Arc<dyn StartGameLock>,
        txn_runner: Arc<dyn TransactionRunner>,
    ) -> Self {
        Self {
            run_repo,
            run_player_repo,
            run_game_repo,
            game_repo,
            player_repo,
            game_card_repo,
            profile_repo,
            user_repo,
            event_publisher,
            run_event_logger,
            lock_service,
            txn_runner,
        }
    }

    pub async fn start_next_game(
        &self,
        run_id: Uuid,
        user_id: Uuid,
    ) -> Result<StartNextGameResponse, RoomServiceError> {
        let mut guard = self.lock_service.acquire(run_id).await?;
        let result = self.start_next_game_inner(run_id, user_id).await;
        guard.release().await;
        result
    }

    async fn start_next_game_inner(
        &self,
        run_id: Uuid,
        user_id: Uuid,
    ) -> Result<StartNextGameResponse, RoomServiceError> {
        // Validate run exists, is active, and still ongoing.
        let run = self.validate_run_active(run_id).await?;
        // Validate the requester is a member of the run.
        self.validate_run_member(run_id, user_id).await?;

        let game_index = run.current_game_index;

        // Idempotent early return when a game already exists at this index.
        if let Some(rg) = self.find_existing_game(run_id, game_index).await? {
            return Ok(existing_response(rg.game_id, game_index, &run));
        }

        // Validate previous game finished and enough players remain.
        self.validate_previous_game_finished(run_id, game_index)
            .await?;
        let run_players = self.load_run_players(run_id).await?;

        // Pure planning phase (no transaction).
        let plan = self
            .build_player_ordering(&run_players, run.bet_per_game)
            .await?;

        // Write phase (single transaction).
        let txn = self.txn_runner.begin().await?;
        let players = self
            .create_game_and_players_in_txn(&txn, run_id, user_id, &plan)
            .await?;
        self.deal_cards_in_txn(&txn, plan.game_id, &players).await?;
        let new_index = self
            .finalize_run_in_txn(&txn, run_id, plan.game_id, game_index)
            .await?;
        self.txn_runner.clone().commit(txn).await?;
        metrics::ACTIVE_GAMES.inc();

        // Side effects + response.
        self.log_and_publish(&run, plan.game_id, game_index, user_id)
            .await?;

        Ok(build_response(plan.game_id, game_index, &run, new_index))
    }

    async fn validate_run_active(&self, run_id: Uuid) -> Result<GameRun, RoomServiceError> {
        let run = self
            .run_repo
            .find_by_id(run_id)
            .await?
            .ok_or(RoomServiceError::RunNotFound)?;

        if run.status != RunStatus::Active {
            return Err(RoomServiceError::RunNotActive { status: run.status });
        }

        if run.current_game_index >= run.num_games {
            return Err(RoomServiceError::RunCompleted);
        }

        Ok(run)
    }

    async fn validate_run_member(
        &self,
        run_id: Uuid,
        user_id: Uuid,
    ) -> Result<(), RoomServiceError> {
        self.run_player_repo
            .find_by_run_and_user(run_id, user_id)
            .await?
            .ok_or(RoomServiceError::NotRunPlayer)?;
        Ok(())
    }

    async fn find_existing_game(
        &self,
        run_id: Uuid,
        game_index: i32,
    ) -> Result<Option<GameRunGame>, RoomServiceError> {
        Ok(self
            .run_game_repo
            .find_by_run_and_index(run_id, game_index)
            .await?)
    }

    async fn validate_previous_game_finished(
        &self,
        run_id: Uuid,
        game_index: i32,
    ) -> Result<(), RoomServiceError> {
        if game_index == 0 {
            return Ok(());
        }

        if let Some(prev_run_game) = self
            .run_game_repo
            .find_by_run_and_index(run_id, game_index - 1)
            .await?
        {
            if let Some(prev_game) = self.game_repo.find_by_id(prev_run_game.game_id).await? {
                let is_finished = matches!(
                    prev_game.status,
                    GameStatus::Finished
                        | GameStatus::Kora
                        | GameStatus::DoubleKora
                        | GameStatus::Cancelled
                );
                if !is_finished {
                    return Err(RoomServiceError::PreviousGameNotFinished);
                }
            }
        }

        Ok(())
    }

    async fn load_run_players(&self, run_id: Uuid) -> Result<Vec<GameRunPlayer>, RoomServiceError> {
        let run_players = self.run_player_repo.list_by_run(run_id).await?;

        if run_players.len() < 2 {
            return Err(RoomServiceError::NotEnoughPlayers);
        }

        Ok(run_players)
    }

    async fn build_player_ordering(
        &self,
        run_players: &[GameRunPlayer],
        bet: i32,
    ) -> Result<GameCreationPlan, RoomServiceError> {
        let player_ids: Vec<Uuid> = run_players.iter().map(|p| p.user_id).collect();

        let users = self.user_repo.find_by_ids(&player_ids).await?;
        let user_map: HashMap<Uuid, String> = users.into_iter().map(|u| (u.id, u.pseudo)).collect();

        let profiles = self.profile_repo.find_by_user_ids(&player_ids).await?;
        let profile_map: HashMap<Uuid, PlayerProfile> =
            profiles.into_iter().map(|p| (p.user_id, p)).collect();

        Ok(assemble_game_creation_plan(
            run_players,
            user_map,
            profile_map,
            bet,
        ))
    }

    async fn create_game_and_players_in_txn(
        &self,
        txn: &sea_orm::DatabaseTransaction,
        run_id: Uuid,
        user_id: Uuid,
        plan: &GameCreationPlan,
    ) -> Result<Vec<Player>, RoomServiceError> {
        let position_json = serde_json::to_value(&plan.position_map)
            .map_err(|e| RoomServiceError::Internal(format!("Failed to serialize: {}", e)))?;

        self.game_repo
            .create_game_for_run_in_txn(
                txn,
                plan.game_id,
                plan.bet,
                Some(user_id),
                position_json,
                plan.player_ids.len() as i16,
                run_id,
            )
            .await?;

        let mut players = Vec::with_capacity(plan.shuffled_positions.len());

        for (new_pos, &orig_idx) in plan.shuffled_positions.iter().enumerate() {
            let loop_user_id = plan.player_ids[orig_idx];
            let player_id = Uuid::now_v7();
            let name = plan
                .user_map
                .get(&loop_user_id)
                .cloned()
                .unwrap_or_else(|| "Unknown".to_string());

            let profile_credit = plan
                .profile_map
                .get(&loop_user_id)
                .ok_or_else(|| RoomServiceError::ProfileNotFound)?
                .credit;

            let player = self
                .player_repo
                .create_player_for_run_in_txn(
                    txn,
                    player_id,
                    plan.game_id,
                    loop_user_id,
                    &name,
                    new_pos as i32,
                    profile_credit,
                )
                .await?;

            if let Some(rp) = plan.run_player_map.get(&loop_user_id) {
                self.run_player_repo
                    .deduct_provisioned_in_txn(txn, rp.id, plan.bet)
                    .await?;
            }

            players.push(player);
        }

        Ok(players)
    }

    async fn deal_cards_in_txn(
        &self,
        txn: &sea_orm::DatabaseTransaction,
        game_id: Uuid,
        players: &[Player],
    ) -> Result<(), RoomServiceError> {
        let (card_models, _cards) = crate::game::cards::build_game_cards(game_id, players);
        self.game_card_repo
            .bulk_insert_in_txn(txn, card_models)
            .await?;
        Ok(())
    }

    async fn finalize_run_in_txn(
        &self,
        txn: &sea_orm::DatabaseTransaction,
        run_id: Uuid,
        game_id: Uuid,
        game_index: i32,
    ) -> Result<i32, RoomServiceError> {
        self.run_game_repo
            .create_in_txn(txn, run_id, game_id, game_index, RunStatus::Active)
            .await?;

        let new_index = game_index + 1;
        let now = chrono::Utc::now();
        self.run_repo
            .increment_game_index_in_txn(txn, run_id, new_index, now)
            .await?;

        Ok(new_index)
    }

    async fn log_and_publish(
        &self,
        run: &GameRun,
        game_id: Uuid,
        game_index: i32,
        user_id: Uuid,
    ) -> Result<(), RoomServiceError> {
        self.run_event_logger
            .log(
                run.id,
                Some(user_id),
                "game_started",
                Some(&format!("game_index={}", game_index)),
            )
            .await?;

        self.event_publisher
            .publish(&RoomEvent::GameStarted {
                room_id: run.room_id,
                run_id: run.id,
                game_id,
                game_index,
                total_games: run.num_games,
            })
            .await;

        Ok(())
    }

    pub async fn get_current_game(
        &self,
        run_id: Uuid,
        user_id: Uuid,
    ) -> Result<CurrentGameResponse, RoomServiceError> {
        let run = self
            .run_repo
            .find_by_id(run_id)
            .await?
            .ok_or(RoomServiceError::RunNotFound)?;

        let _run_player = self
            .run_player_repo
            .find_by_run_and_user(run_id, user_id)
            .await?
            .ok_or(RoomServiceError::NotRunPlayer)?;

        let current = self
            .run_game_repo
            .find_by_run_and_index(run_id, run.current_game_index)
            .await?;

        match current {
            Some(rg) => Ok(CurrentGameResponse {
                run_id,
                game_id: rg.game_id,
                game_index: rg.game_index,
                status: rg.status,
            }),
            None => Err(RoomServiceError::GameNotFound),
        }
    }
}

fn assemble_game_creation_plan(
    run_players: &[GameRunPlayer],
    user_map: HashMap<Uuid, String>,
    profile_map: HashMap<Uuid, PlayerProfile>,
    bet: i32,
) -> GameCreationPlan {
    let player_ids: Vec<Uuid> = run_players.iter().map(|p| p.user_id).collect();

    let run_player_map: HashMap<Uuid, GameRunPlayer> = run_players
        .iter()
        .map(|rp| (rp.user_id, rp.clone()))
        .collect();

    use rand::rng;
    use rand::seq::SliceRandom;

    let mut shuffled_positions: Vec<usize> = (0..player_ids.len()).collect();
    shuffled_positions.shuffle(&mut rng());

    let game_id = Uuid::now_v7();

    let position_map: HashMap<i32, Uuid> = shuffled_positions
        .iter()
        .enumerate()
        .map(|(new_pos, &orig_idx)| (new_pos as i32, player_ids[orig_idx]))
        .collect();

    GameCreationPlan {
        game_id,
        player_ids,
        user_map,
        profile_map,
        run_player_map,
        shuffled_positions,
        position_map,
        bet,
    }
}

fn existing_response(game_id: Uuid, game_index: i32, run: &GameRun) -> StartNextGameResponse {
    StartNextGameResponse {
        game_id,
        game_index,
        total_games: run.num_games,
        current_game_index: game_index,
        all_games_created: None,
        status: Some("existing".to_string()),
    }
}

fn build_response(
    game_id: Uuid,
    game_index: i32,
    run: &GameRun,
    new_index: i32,
) -> StartNextGameResponse {
    StartNextGameResponse {
        game_id,
        game_index,
        total_games: run.num_games,
        current_game_index: new_index,
        all_games_created: Some(new_index >= run.num_games),
        status: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::models::PlayerType;

    fn make_run_player(user_id: Uuid) -> GameRunPlayer {
        GameRunPlayer {
            id: Uuid::now_v7(),
            game_run_id: Uuid::nil(),
            user_id,
            position: 0,
            provisioned_credits: 0,
            kicked: false,
            joined_at: chrono::Utc::now(),
        }
    }

    fn make_profile(user_id: Uuid) -> PlayerProfile {
        PlayerProfile {
            id: Uuid::now_v7(),
            user_id,
            player_type: PlayerType::Human,
            credit: 1000,
            game_played: 0,
            wins: 0,
            kora_wins: 0,
            winning_streak: 0,
            latitude: None,
            longitude: None,
            country_code: None,
            city: None,
            frozen_until: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn assemble_plan_keeps_all_players_and_orders_positions() {
        let players: Vec<GameRunPlayer> = (0..4).map(|_| make_run_player(Uuid::now_v7())).collect();
        let user_ids: Vec<Uuid> = players.iter().map(|p| p.user_id).collect();

        let user_map: HashMap<Uuid, String> = user_ids
            .iter()
            .enumerate()
            .map(|(i, id)| (*id, format!("user{}", i)))
            .collect();
        let profile_map: HashMap<Uuid, PlayerProfile> =
            user_ids.iter().map(|id| (*id, make_profile(*id))).collect();

        let plan = assemble_game_creation_plan(&players, user_map, profile_map, 25);

        assert_eq!(plan.player_ids, user_ids);
        assert_eq!(plan.bet, 25);
        assert_eq!(plan.shuffled_positions.len(), 4);
        assert_eq!(plan.position_map.len(), 4);

        // shuffled_positions is a permutation of 0..3
        let mut sorted_positions = plan.shuffled_positions.clone();
        sorted_positions.sort_unstable();
        assert_eq!(sorted_positions, vec![0, 1, 2, 3]);

        // position_map covers every position with a distinct player
        let mut assigned: Vec<Uuid> = (0..4).map(|i| plan.position_map[&i]).collect();
        assigned.sort();
        let mut expected = user_ids.clone();
        expected.sort();
        assert_eq!(assigned, expected);

        // every player is represented in the run_player_map
        for id in &user_ids {
            assert!(plan.run_player_map.contains_key(id));
        }
    }
}
