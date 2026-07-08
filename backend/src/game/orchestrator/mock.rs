use async_trait::async_trait;
use sea_orm::DatabaseConnection;
use std::sync::Mutex;
use uuid::Uuid;

use crate::error::GameError;
use crate::observability::CorrelationId;

use super::*;

#[allow(dead_code)]
pub struct MockGameOrchestrator {
    play_card_result: Mutex<Option<Result<PlayCardOutcome, GameError>>>,
    create_quick_game_result: Mutex<Option<Result<QuickGameOutcome, GameError>>>,
    create_multiplayer_game_result: Mutex<Option<Result<MultiplayerCreationOutcome, GameError>>>,
    send_invites_result: Mutex<Option<Result<(), GameError>>>,
    accept_invite_result: Mutex<Option<Result<AcceptInviteOutcome, GameError>>>,
    cancel_game_result: Mutex<Option<Result<(), GameError>>>,
    start_game_result: Mutex<Option<Result<(), GameError>>>,
    create_benchmark_game_result: Mutex<Option<Result<BenchmarkGameOutcome, GameError>>>,
    cleanup_benchmark_result: Mutex<Option<Result<BenchmarkCleanupCounts, GameError>>>,
}

impl MockGameOrchestrator {
    pub fn new(
        play_card_result: Result<PlayCardOutcome, GameError>,
        create_quick_game_result: Result<QuickGameOutcome, GameError>,
    ) -> Self {
        Self {
            play_card_result: Mutex::new(Some(play_card_result)),
            create_quick_game_result: Mutex::new(Some(create_quick_game_result)),
            create_multiplayer_game_result: Mutex::new(None),
            send_invites_result: Mutex::new(None),
            accept_invite_result: Mutex::new(None),
            cancel_game_result: Mutex::new(None),
            start_game_result: Mutex::new(None),
            create_benchmark_game_result: Mutex::new(None),
            cleanup_benchmark_result: Mutex::new(None),
        }
    }

    pub fn ok() -> Self {
        let play_outcome = PlayCardOutcome {
            card_id: Uuid::new_v4(),
            next_turn: Some(Uuid::new_v4()),
            game_ended: false,
            round_completed: false,
            current_round: 1,
        };
        let quick_outcome = QuickGameOutcome {
            game_id: Uuid::new_v4(),
            players: vec![],
            status: "active".to_string(),
            current_turn: 0,
            bet: 10,
            max_players: 4,
            invite_expires_at: None,
            deck_slots: None,
            ws_token: None,
            step_by_step: false,
        };
        Self::new(Ok(play_outcome), Ok(quick_outcome))
    }

    pub fn set_accept_invite_result(&self, result: Result<AcceptInviteOutcome, GameError>) {
        *self.accept_invite_result.lock().unwrap() = Some(result);
    }
}

#[async_trait]
impl GameOrchestratorTrait for MockGameOrchestrator {
    async fn play_card(
        &self,
        _game_id: Uuid,
        _player_id: Uuid,
        _card_index: i32,
        _correlation_id: Option<CorrelationId>,
        _idempotency_key: Option<String>,
    ) -> Result<PlayCardOutcome, GameError> {
        self.play_card_result
            .lock()
            .unwrap()
            .take()
            .expect("mock orchestrator play_card called more than once")
    }

    async fn create_quick_game(
        &self,
        _correlation_id: Option<CorrelationId>,
        _step_by_step: bool,
    ) -> Result<QuickGameOutcome, GameError> {
        self.create_quick_game_result
            .lock()
            .unwrap()
            .take()
            .expect("mock orchestrator create_quick_game called more than once")
    }

    async fn create_bot_only_game(&self) -> Result<QuickGameOutcome, GameError> {
        self.create_quick_game_result
            .lock()
            .unwrap()
            .take()
            .expect("mock orchestrator create_bot_only_game called more than once")
    }

    async fn create_quick_game_for_user(
        &self,
        _user_id: Uuid,
        _db: &DatabaseConnection,
    ) -> Result<QuickGameOutcome, GameError> {
        self.create_quick_game_result
            .lock()
            .unwrap()
            .take()
            .expect("mock orchestrator create_quick_game_for_user called more than once")
    }

    async fn create_quick_game_for_user_with_step_by_step(
        &self,
        _user_id: Uuid,
        _db: &DatabaseConnection,
        _step_by_step: bool,
    ) -> Result<QuickGameOutcome, GameError> {
        self.create_quick_game_result.lock().unwrap().take().expect(
            "mock orchestrator create_quick_game_for_user_with_step_by_step called more than once",
        )
    }

    async fn create_multiplayer_game(
        &self,
        _user_id: Uuid,
        _pseudo: &str,
        _bet: i32,
        _max_players: i16,
    ) -> Result<MultiplayerCreationOutcome, GameError> {
        self.create_multiplayer_game_result
            .lock()
            .unwrap()
            .take()
            .expect("mock orchestrator create_multiplayer_game called more than once")
    }

    async fn create_benchmark_multiplayer_game(
        &self,
        _user_ids: Vec<Uuid>,
        _bet: i32,
    ) -> Result<BenchmarkGameOutcome, GameError> {
        self.create_benchmark_game_result
            .lock()
            .unwrap()
            .take()
            .expect("mock orchestrator create_benchmark_multiplayer_game called more than once")
    }

    async fn cleanup_benchmark_data(&self) -> Result<BenchmarkCleanupCounts, GameError> {
        self.cleanup_benchmark_result
            .lock()
            .unwrap()
            .take()
            .expect("mock orchestrator cleanup_benchmark_data called more than once")
    }

    async fn start_game(&self, _game_id: Uuid, _user_id: Uuid) -> Result<(), GameError> {
        self.start_game_result
            .lock()
            .unwrap()
            .take()
            .expect("mock orchestrator start_game called more than once")
    }

    async fn send_invites(
        &self,
        _game_id: Uuid,
        _creator_user_id: Uuid,
        _invited_user_ids: Vec<Uuid>,
    ) -> Result<(), GameError> {
        self.send_invites_result
            .lock()
            .unwrap()
            .take()
            .expect("mock orchestrator send_invites called more than once")
    }

    async fn accept_invite(
        &self,
        _game_id: Uuid,
        _user_id: Uuid,
        _pseudo: &str,
    ) -> Result<AcceptInviteOutcome, GameError> {
        self.accept_invite_result
            .lock()
            .unwrap()
            .take()
            .expect("mock orchestrator accept_invite called more than once")
    }

    async fn decline_invite(&self, _game_id: Uuid, _user_id: Uuid) -> Result<(), GameError> {
        Ok(())
    }

    async fn cancel_game(&self, _game_id: Uuid) -> Result<(), GameError> {
        self.cancel_game_result
            .lock()
            .unwrap()
            .take()
            .expect("mock orchestrator cancel_game called more than once")
    }

    async fn advance_bot(
        &self,
        _game_id: Uuid,
        _human_player_id: Uuid,
    ) -> Result<AdvanceBotOutcome, GameError> {
        Ok(AdvanceBotOutcome {
            card_played: 0,
            next_player_id: Uuid::new_v4(),
            next_is_bot: false,
            round_complete: false,
            game_ended: false,
        })
    }

    async fn evaluate_round(
        &self,
        _game_id: Uuid,
        _human_player_id: Uuid,
    ) -> Result<EvaluateRoundOutcome, GameError> {
        Ok(EvaluateRoundOutcome {
            round_number: 1,
            winner_id: Uuid::new_v4(),
            winner_position: 0,
            game_ended: false,
        })
    }
}
