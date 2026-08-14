use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use sea_orm::Set;

use crate::api::dto::responses::{CurrentGameResponse, StartNextGameResponse};
use crate::database::models::{GameStatus, RunStatus};
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
        metrics::ACTIVE_GAMES.inc();
        result
    }

    async fn start_next_game_inner(
        &self,
        run_id: Uuid,
        user_id: Uuid,
    ) -> Result<StartNextGameResponse, RoomServiceError> {
        let run = self
            .run_repo
            .find_by_id(run_id)
            .await?
            .ok_or(RoomServiceError::RunNotFound)?;

        if run.status != RunStatus::Active {
            return Err(RoomServiceError::RunNotActive { status: run.status });
        }

        let _run_player = self
            .run_player_repo
            .find_by_run_and_user(run_id, user_id)
            .await?
            .ok_or(RoomServiceError::NotRunPlayer)?;

        let game_index = run.current_game_index;
        if game_index >= run.num_games {
            return Err(RoomServiceError::RunCompleted);
        }

        let existing = self
            .run_game_repo
            .find_by_run_and_index(run_id, game_index)
            .await?;
        if let Some(rg) = existing {
            return Ok(StartNextGameResponse {
                game_id: rg.game_id,
                game_index,
                total_games: run.num_games,
                current_game_index: game_index,
                all_games_created: None,
                status: Some("existing".to_string()),
            });
        }

        if game_index > 0 {
            if let Some(prev_run_game) = self
                .run_game_repo
                .find_by_run_and_index(run_id, game_index - 1)
                .await?
            {
                let prev_game = self.game_repo.find_by_id(prev_run_game.game_id).await?;
                if let Some(pg) = prev_game {
                    let is_finished = matches!(
                        pg.status,
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
        }

        let run_players = self.run_player_repo.list_by_run(run_id).await?;

        if run_players.len() < 2 {
            return Err(RoomServiceError::NotEnoughPlayers);
        }

        let player_ids: Vec<Uuid> = run_players.iter().map(|p| p.user_id).collect();

        let users = self.user_repo.find_by_ids(&player_ids).await?;
        let user_map: HashMap<Uuid, String> = users.into_iter().map(|u| (u.id, u.pseudo)).collect();

        let bet = run.bet_per_game;
        use rand::seq::SliceRandom;
        use rand::thread_rng;

        let mut shuffled_positions: Vec<usize> = (0..player_ids.len()).collect();
        shuffled_positions.shuffle(&mut thread_rng());

        let game_id = Uuid::now_v7();

        let position_map: std::collections::HashMap<i32, Uuid> = shuffled_positions
            .iter()
            .enumerate()
            .map(|(new_pos, &orig_idx)| (new_pos as i32, player_ids[orig_idx]))
            .collect();

        let txn = self.txn_runner.begin().await?;

        let position_json = serde_json::to_value(&position_map)
            .map_err(|e| RoomServiceError::Internal(format!("Failed to serialize: {}", e)))?;

        self.game_repo
            .create_game_for_run_in_txn(
                &txn,
                game_id,
                bet,
                Some(user_id),
                position_json,
                player_ids.len() as i16,
                run_id,
            )
            .await?;

        let profiles = self.profile_repo.find_by_user_ids(&player_ids).await?;
        let profile_map: HashMap<Uuid, crate::database::models::PlayerProfile> =
            profiles.into_iter().map(|p| (p.user_id, p)).collect();

        let run_player_map: HashMap<Uuid, crate::database::models::GameRunPlayer> = run_players
            .iter()
            .map(|rp| (rp.user_id, rp.clone()))
            .collect();

        for (new_pos, &orig_idx) in shuffled_positions.iter().enumerate() {
            let loop_user_id = player_ids[orig_idx];
            let player_id = Uuid::now_v7();
            let name = user_map
                .get(&loop_user_id)
                .cloned()
                .unwrap_or_else(|| "Unknown".to_string());

            let profile_credit = profile_map
                .get(&loop_user_id)
                .ok_or_else(|| RoomServiceError::ProfileNotFound)?
                .credit;

            self.player_repo
                .create_player_for_run_in_txn(
                    &txn,
                    player_id,
                    game_id,
                    loop_user_id,
                    &name,
                    new_pos as i32,
                    profile_credit,
                )
                .await?;

            if let Some(rp) = run_player_map.get(&loop_user_id) {
                self.run_player_repo
                    .deduct_provisioned_in_txn(&txn, rp.id, bet)
                    .await?;
            }
        }

        let cards: Vec<i32> = {
            let mut cards: Vec<i32> = (0..crate::game::constants::TOTAL_CARDS as i32).collect();
            cards.shuffle(&mut thread_rng());
            cards
        };

        let created_players = self.player_repo.list_by_game_in_txn(&txn, game_id).await?;

        let card_models: Vec<crate::database::models::game_card::ActiveModel> = created_players
            .iter()
            .enumerate()
            .flat_map(|(i, p)| {
                let start = i * crate::game::constants::CARDS_PER_PLAYER;
                let end = start + crate::game::constants::CARDS_PER_PLAYER;
                cards[start..end].iter().map(move |&ci| {
                    crate::database::models::game_card::ActiveModel {
                        id: Set(Uuid::now_v7()),
                        game_id: Set(game_id),
                        player_id: Set(Some(p.id)),
                        card_index: Set(ci),
                        played: Set(false),
                        played_at: sea_orm::ActiveValue::NotSet,
                        round: sea_orm::ActiveValue::NotSet,
                        created_at: Set(chrono::Utc::now()),
                    }
                })
            })
            .collect();

        self.game_card_repo
            .bulk_insert_in_txn(&txn, card_models)
            .await?;

        self.run_game_repo
            .create_in_txn(&txn, run_id, game_id, game_index, RunStatus::Active)
            .await?;

        let new_index = game_index + 1;
        let now = chrono::Utc::now();
        self.run_repo
            .increment_game_index_in_txn(&txn, run_id, new_index, now)
            .await?;

        self.txn_runner.clone().commit(txn).await?;

        let all_games_created = new_index >= run.num_games;

        self.run_event_logger
            .log(
                run_id,
                Some(user_id),
                "game_started",
                Some(&format!("game_index={}", game_index)),
            )
            .await?;

        self.event_publisher
            .publish(&RoomEvent::GameStarted {
                room_id: run.room_id,
                run_id,
                game_id,
                game_index,
                total_games: run.num_games,
            })
            .await;

        Ok(StartNextGameResponse {
            game_id,
            game_index,
            total_games: run.num_games,
            current_game_index: new_index,
            all_games_created: Some(all_games_created),
            status: None,
        })
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
