use std::sync::Arc;
use uuid::Uuid;

use super::super::{
    error::RoomServiceError,
    event_publisher::RoomEventPublisher,
    start_game_lock::{StartGameLock, StartGameLockGuard},
    start_next_game::StartNextGameService,
    transaction_runner::{SeaOrmTransactionRunner, TransactionRunner},
};
use crate::config::Config;
use crate::database::models::PlayerProfile;
use crate::database::traits::{
    GameCardRepoTrait, GameRepoTrait, GameRunEventRepoTrait, GameRunGameRepoTrait,
    GameRunPlayerRepoTrait, GameRunRepoTrait, PlayerProfileRepoTrait, PlayerRepoTrait,
    UserRepoTrait,
};
use crate::mailer::noop::NoopMailer;
use crate::mailer::MailerConfig;

pub(super) fn make_stub_start_next_game_svc(
    db: sea_orm::DatabaseConnection,
) -> Arc<StartNextGameService> {
    let stub_run_repo: Arc<dyn GameRunRepoTrait> = Arc::new(StubGameRunRepo);
    let stub_run_player_repo: Arc<dyn GameRunPlayerRepoTrait> = Arc::new(StubGameRunPlayerRepo);
    let stub_run_game_repo: Arc<dyn GameRunGameRepoTrait> = Arc::new(StubGameRunGameRepo);
    let stub_game_repo: Arc<dyn GameRepoTrait> = Arc::new(StubGameRepo);
    let stub_player_repo: Arc<dyn PlayerRepoTrait> = Arc::new(StubPlayerRepo);
    let stub_game_card_repo: Arc<dyn GameCardRepoTrait> = Arc::new(StubGameCardRepo);
    let stub_profile_repo: Arc<dyn PlayerProfileRepoTrait> = Arc::new(StubProfileRepo);
    let stub_user_repo: Arc<dyn UserRepoTrait> = Arc::new(StubUserRepo);
    let stub_event_publisher: Arc<dyn RoomEventPublisher> = Arc::new(StubEventPublisher);
    let stub_run_event_logger: Arc<dyn GameRunEventRepoTrait> = Arc::new(StubRunEventRepo);
    let stub_lock: Arc<dyn StartGameLock> = Arc::new(StubLock);
    let stub_txn: Arc<dyn TransactionRunner> = Arc::new(SeaOrmTransactionRunner::new(db.clone()));

    Arc::new(StartNextGameService::new(
        stub_run_repo,
        stub_run_player_repo,
        stub_run_game_repo,
        stub_game_repo,
        stub_player_repo,
        stub_game_card_repo,
        stub_profile_repo,
        stub_user_repo,
        stub_event_publisher,
        stub_run_event_logger,
        stub_lock,
        stub_txn,
    ))
}

pub(super) struct StubGameRunRepo;
#[async_trait::async_trait]
impl GameRunRepoTrait for StubGameRunRepo {
    async fn find_by_id(
        &self,
        _id: Uuid,
    ) -> Result<Option<crate::database::models::GameRun>, sea_orm::DbErr> {
        Ok(None)
    }
    async fn list_by_room(
        &self,
        _room_id: Uuid,
    ) -> Result<Vec<crate::database::models::GameRun>, sea_orm::DbErr> {
        Ok(vec![])
    }
    async fn find_active_by_room(
        &self,
        _room_id: Uuid,
    ) -> Result<Option<crate::database::models::GameRun>, sea_orm::DbErr> {
        Ok(None)
    }
    async fn increment_game_index_in_txn(
        &self,
        _txn: &sea_orm::DatabaseTransaction,
        _run_id: Uuid,
        _new_index: i32,
        _now: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), sea_orm::DbErr> {
        Ok(())
    }
    async fn update_status_in_txn(
        &self,
        _txn: &sea_orm::DatabaseTransaction,
        _run_id: Uuid,
        _status: crate::database::models::RunStatus,
    ) -> Result<(), sea_orm::DbErr> {
        Ok(())
    }
}

pub(super) struct StubGameRunPlayerRepo;
#[async_trait::async_trait]
impl GameRunPlayerRepoTrait for StubGameRunPlayerRepo {
    async fn find_by_run_and_user(
        &self,
        _run_id: Uuid,
        _user_id: Uuid,
    ) -> Result<Option<crate::database::models::GameRunPlayer>, sea_orm::DbErr> {
        Ok(None)
    }
    async fn list_by_run(
        &self,
        _run_id: Uuid,
    ) -> Result<Vec<crate::database::models::GameRunPlayer>, sea_orm::DbErr> {
        Ok(vec![])
    }
    async fn deduct_provisioned_in_txn(
        &self,
        _txn: &sea_orm::DatabaseTransaction,
        _id: Uuid,
        _amount: i32,
    ) -> Result<(), sea_orm::DbErr> {
        Ok(())
    }
}

pub(super) struct StubGameRunGameRepo;
#[async_trait::async_trait]
impl GameRunGameRepoTrait for StubGameRunGameRepo {
    async fn find_by_run_and_index(
        &self,
        _run_id: Uuid,
        _game_index: i32,
    ) -> Result<Option<crate::database::models::GameRunGame>, sea_orm::DbErr> {
        Ok(None)
    }
    async fn list_by_run(
        &self,
        _run_id: Uuid,
    ) -> Result<Vec<crate::database::models::GameRunGame>, sea_orm::DbErr> {
        Ok(vec![])
    }
    async fn create_in_txn(
        &self,
        _txn: &sea_orm::DatabaseTransaction,
        _run_id: Uuid,
        _game_id: Uuid,
        _game_index: i32,
        _status: crate::database::models::RunStatus,
    ) -> Result<(), sea_orm::DbErr> {
        Ok(())
    }
}

pub(super) struct StubGameRepo;
#[async_trait::async_trait]
impl GameRepoTrait for StubGameRepo {
    async fn create(
        &self,
        _bet: i32,
        _auto: bool,
        _step_by_step: bool,
    ) -> Result<crate::database::models::Game, sea_orm::DbErr> {
        Err(sea_orm::DbErr::Custom("stub".into()))
    }
    async fn find_by_id(
        &self,
        _id: Uuid,
    ) -> Result<Option<crate::database::models::Game>, sea_orm::DbErr> {
        Ok(None)
    }
    async fn update_rank(
        &self,
        _id: Uuid,
        _rank: Option<i32>,
    ) -> Result<crate::database::models::Game, sea_orm::DbErr> {
        Err(sea_orm::DbErr::Custom("stub".into()))
    }
    async fn update_status(
        &self,
        _id: Uuid,
        _status: crate::database::models::GameStatus,
    ) -> Result<crate::database::models::Game, sea_orm::DbErr> {
        Err(sea_orm::DbErr::Custom("stub".into()))
    }
    async fn update_winner(
        &self,
        _id: Uuid,
        _winner_id: Option<Uuid>,
    ) -> Result<crate::database::models::Game, sea_orm::DbErr> {
        Err(sea_orm::DbErr::Custom("stub".into()))
    }
    async fn list_players(
        &self,
        _game_id: Uuid,
    ) -> Result<Vec<crate::database::models::Player>, sea_orm::DbErr> {
        Ok(vec![])
    }
    async fn create_game_for_run_in_txn(
        &self,
        _txn: &sea_orm::DatabaseTransaction,
        _game_id: Uuid,
        _bet: i32,
        _creator_id: Option<Uuid>,
        _player_positions: serde_json::Value,
        _num_players: i16,
        _run_id: Uuid,
    ) -> Result<(), sea_orm::DbErr> {
        Ok(())
    }
}

pub(super) struct StubPlayerRepo;
#[async_trait::async_trait]
impl PlayerRepoTrait for StubPlayerRepo {
    async fn create(
        &self,
        _game_id: Uuid,
        _player_type: crate::database::models::PlayerType,
        _name: &str,
        _position: i32,
    ) -> Result<crate::database::models::Player, sea_orm::DbErr> {
        Err(sea_orm::DbErr::Custom("stub".into()))
    }
    async fn create_with_user(
        &self,
        _game_id: Uuid,
        _player_type: crate::database::models::PlayerType,
        _name: &str,
        _position: i32,
        _user_id: Uuid,
    ) -> Result<crate::database::models::Player, sea_orm::DbErr> {
        Err(sea_orm::DbErr::Custom("stub".into()))
    }
    async fn list_by_game(
        &self,
        _game_id: Uuid,
    ) -> Result<Vec<crate::database::models::Player>, sea_orm::DbErr> {
        Ok(vec![])
    }
    async fn find_by_game_and_user(
        &self,
        _game_id: Uuid,
        _user_id: Uuid,
    ) -> Result<Option<crate::database::models::Player>, sea_orm::DbErr> {
        Ok(None)
    }
    async fn create_player_for_run_in_txn(
        &self,
        _txn: &sea_orm::DatabaseTransaction,
        _player_id: Uuid,
        _game_id: Uuid,
        _user_id: Uuid,
        _name: &str,
        _position: i32,
        _credits: i32,
    ) -> Result<(), sea_orm::DbErr> {
        Ok(())
    }
    async fn list_by_game_in_txn(
        &self,
        _txn: &sea_orm::DatabaseTransaction,
        _game_id: Uuid,
    ) -> Result<Vec<crate::database::models::Player>, sea_orm::DbErr> {
        Ok(vec![])
    }
}

pub(super) struct StubGameCardRepo;
#[async_trait::async_trait]
impl GameCardRepoTrait for StubGameCardRepo {
    async fn create(
        &self,
        _game_id: Uuid,
        _player_id: Option<Uuid>,
        _card_index: i32,
        _round: Option<i32>,
    ) -> Result<crate::database::models::GameCard, sea_orm::DbErr> {
        Err(sea_orm::DbErr::Custom("stub".into()))
    }
    async fn bulk_insert(
        &self,
        _cards: Vec<(Uuid, Option<Uuid>, i32)>,
    ) -> Result<(), sea_orm::DbErr> {
        Ok(())
    }
    async fn list_by_player(
        &self,
        _player_id: Uuid,
    ) -> Result<Vec<crate::database::models::GameCard>, sea_orm::DbErr> {
        Ok(vec![])
    }
    async fn list_by_game_and_round(
        &self,
        _game_id: Uuid,
        _round: i32,
    ) -> Result<Vec<crate::database::models::GameCard>, sea_orm::DbErr> {
        Ok(vec![])
    }
    async fn list_by_game(
        &self,
        _game_id: Uuid,
    ) -> Result<Vec<crate::database::models::GameCard>, sea_orm::DbErr> {
        Ok(vec![])
    }
    async fn bulk_insert_in_txn(
        &self,
        _txn: &sea_orm::DatabaseTransaction,
        _cards: Vec<crate::database::models::game_card::ActiveModel>,
    ) -> Result<(), sea_orm::DbErr> {
        Ok(())
    }
}

pub(super) struct StubProfileRepo;
#[async_trait::async_trait]
impl PlayerProfileRepoTrait for StubProfileRepo {
    async fn find_by_user_id(
        &self,
        _user_id: Uuid,
    ) -> Result<Option<PlayerProfile>, sea_orm::DbErr> {
        Ok(None)
    }
    async fn find_by_user_ids(
        &self,
        _user_ids: &[Uuid],
    ) -> Result<Vec<PlayerProfile>, sea_orm::DbErr> {
        Ok(vec![])
    }
    async fn update_stats(
        &self,
        _user_id: Uuid,
        _wins_delta: i32,
        _kora_wins_delta: i32,
    ) -> Result<PlayerProfile, sea_orm::DbErr> {
        Err(sea_orm::DbErr::Custom("stub".into()))
    }
    async fn apply_game_settlement_in_txn(
        &self,
        _txn: &sea_orm::DatabaseTransaction,
        _user_id: Uuid,
        _delta: i32,
        _won: bool,
        _is_kora: bool,
        _freeze_duration_secs: u64,
    ) -> Result<Option<i32>, sea_orm::DbErr> {
        Ok(None)
    }
}

pub(super) struct StubUserRepo;
#[async_trait::async_trait]
impl UserRepoTrait for StubUserRepo {
    async fn find_by_email(
        &self,
        _email: &str,
    ) -> Result<Option<crate::database::models::User>, sea_orm::DbErr> {
        Ok(None)
    }
    async fn find_by_id(
        &self,
        _id: Uuid,
    ) -> Result<Option<crate::database::models::User>, sea_orm::DbErr> {
        Ok(None)
    }
    async fn find_by_ids(
        &self,
        _ids: &[Uuid],
    ) -> Result<Vec<crate::database::models::User>, sea_orm::DbErr> {
        Ok(vec![])
    }
    async fn find_by_pseudo(
        &self,
        _pseudo: &str,
    ) -> Result<Option<crate::database::models::User>, sea_orm::DbErr> {
        Ok(None)
    }
    async fn find_by_pseudo_prefix(
        &self,
        _prefix: &str,
        _limit: u64,
    ) -> Result<Vec<crate::database::models::User>, sea_orm::DbErr> {
        Ok(vec![])
    }
    async fn create_user_with_profile(
        &self,
        _pseudo: &str,
        _email: &str,
        _password_hash: &str,
        _ip_hash: Option<&str>,
    ) -> Result<(crate::database::models::User, PlayerProfile), sea_orm::DbErr> {
        Err(sea_orm::DbErr::Custom("stub".into()))
    }
    async fn update_password_hash(
        &self,
        _id: Uuid,
        _hash: &str,
    ) -> Result<crate::database::models::User, sea_orm::DbErr> {
        Err(sea_orm::DbErr::Custom("stub".into()))
    }
    async fn update_last_ip_hash(
        &self,
        _id: Uuid,
        _hash: &str,
    ) -> Result<crate::database::models::User, sea_orm::DbErr> {
        Err(sea_orm::DbErr::Custom("stub".into()))
    }
    async fn update_language(
        &self,
        _id: Uuid,
        _language: &str,
    ) -> Result<crate::database::models::User, sea_orm::DbErr> {
        Err(sea_orm::DbErr::Custom("stub".into()))
    }
}

pub(super) struct StubEventPublisher;
#[async_trait::async_trait]
impl RoomEventPublisher for StubEventPublisher {
    async fn publish(&self, _event: &crate::messaging::events::RoomEvent) {}
}

pub(super) struct StubRunEventRepo;
#[async_trait::async_trait]
impl GameRunEventRepoTrait for StubRunEventRepo {
    async fn log(
        &self,
        _run_id: Uuid,
        _user_id: Option<Uuid>,
        _event_type: &str,
        _data: Option<&str>,
    ) -> Result<crate::database::models::GameRunEvent, sea_orm::DbErr> {
        Err(sea_orm::DbErr::Custom("stub".into()))
    }
}

pub(super) struct StubLock;
#[async_trait::async_trait]
impl StartGameLock for StubLock {
    async fn acquire(&self, _run_id: Uuid) -> Result<StartGameLockGuard, RoomServiceError> {
        Ok(StartGameLockGuard {
            redis: None,
            key: String::new(),
            token: String::new(),
            released: false,
        })
    }
}

pub(super) fn make_mailer() -> Arc<dyn crate::mailer::Mailer> {
    let config = MailerConfig {
        mailer_mode: "console".to_string(),
        smtp_host: "".to_string(),
        smtp_port: 0,
        smtp_username: "".to_string(),
        smtp_password: "".to_string(),
        smtp_tls: false,
        smtp_from_email: "test@test.com".to_string(),
        smtp_from_name: "Test".to_string(),
        frontend_url: "http://localhost:3000".to_string(),
        contact_to_email: "support@test.com".to_string(),
    };
    let mailer = NoopMailer::new(config).unwrap();
    Arc::new(mailer)
}

pub(super) fn make_config() -> Config {
    Config::default()
}

pub(super) fn make_room_model(
    id: Uuid,
    creator_id: Uuid,
    name: &str,
    code: &str,
) -> crate::database::models::room::Model {
    use chrono::DateTime;
    crate::database::models::room::Model {
        id,
        creator_id,
        name: name.to_string(),
        invitation_code: code.to_string(),
        created_at: DateTime::from_timestamp(0, 0).unwrap(),
        updated_at: DateTime::from_timestamp(0, 0).unwrap(),
    }
}

pub(super) fn make_room_member(
    id: Uuid,
    room_id: Uuid,
    user_id: Uuid,
) -> crate::database::models::room_member::Model {
    use chrono::DateTime;
    crate::database::models::room_member::Model {
        id,
        room_id,
        user_id,
        joined_at: DateTime::from_timestamp(0, 0).unwrap(),
    }
}

pub(super) fn make_user_model(id: Uuid, pseudo: &str) -> crate::database::models::user::Model {
    use chrono::DateTime;
    crate::database::models::user::Model {
        id,
        pseudo: pseudo.to_string(),
        email: format!("{}@test.com", pseudo),
        password_hash: "hash".to_string(),
        last_ip_hash: None,
        language: "en".to_string(),
        created_at: DateTime::from_timestamp(0, 0).unwrap(),
        updated_at: DateTime::from_timestamp(0, 0).unwrap(),
    }
}
