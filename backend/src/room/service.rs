use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use crate::api::dto::responses::{
    ActiveRunSummary, RoomDetailResponse, RoomGameInfo, RoomListItem, RoomMemberInfo,
};
use crate::config::Config;
use crate::database::repositories::{
    GameCardRepository, GameRepository, GameRunEventRepository, GameRunGameRepository,
    GameRunPlayerRepository, GameRunRepository, PlayerProfileRepository, PlayerRepository,
    RoomMemberRepository, RoomRepository, UserRepository,
};
use crate::i18n::Lang;
use crate::mailer::Mailer;
use crate::messaging::events::RoomEvent;
use crate::messaging::redis::PublishResult;
use crate::messaging::RedisClient;
use crate::room::error::RoomServiceError;
use crate::room::event_publisher::{RedisRoomEventPublisher, RoomEventPublisher};
use crate::room::start_game_lock::{RedisStartGameLock, StartGameLock};
use crate::room::start_next_game::StartNextGameService;
use crate::room::transaction_runner::{SeaOrmTransactionRunner, TransactionRunner};

const EMAIL_CHANNEL_CAPACITY: usize = 256;

pub(crate) struct EmailTask {
    pub email: String,
    pub pseudo: String,
    pub room_name: String,
    pub code: String,
    pub lang: Lang,
}

pub struct RoomService {
    pub(crate) room_repo: RoomRepository,
    pub(crate) member_repo: RoomMemberRepository,
    pub(crate) run_repo: GameRunRepository,
    pub(crate) run_player_repo: GameRunPlayerRepository,
    pub(crate) run_game_repo: GameRunGameRepository,
    pub(crate) run_event_repo: GameRunEventRepository,
    pub(crate) profile_repo: PlayerProfileRepository,
    pub(crate) game_repo: GameRepository,
    pub(crate) player_repo: PlayerRepository,
    pub(crate) user_repo: UserRepository,
    mailer: Arc<dyn Mailer>,
    pub(crate) config: Config,
    redis_client: Option<RedisClient>,
    pub(crate) start_next_game_svc: Arc<StartNextGameService>,
    pub(crate) txn_runner: Arc<dyn TransactionRunner>,
    email_tx: tokio::sync::mpsc::Sender<EmailTask>,
    email_rx: tokio::sync::Mutex<Option<tokio::sync::mpsc::Receiver<EmailTask>>>,
}

impl RoomService {
    pub fn new(
        db: sea_orm::DatabaseConnection,
        mailer: Arc<dyn Mailer>,
        config: Config,
        redis_client: Option<RedisClient>,
    ) -> Self {
        let (email_tx, email_rx) = tokio::sync::mpsc::channel(EMAIL_CHANNEL_CAPACITY);

        let game_repo = GameRepository::new(db.clone());
        let card_repo = GameCardRepository::new(db.clone());
        let run_repo = GameRunRepository::new(db.clone());
        let run_player_repo = GameRunPlayerRepository::new(db.clone());
        let run_game_repo = GameRunGameRepository::new(db.clone());
        let run_event_repo = GameRunEventRepository::new(db.clone());
        let profile_repo = PlayerProfileRepository::new(db.clone());
        let player_repo = PlayerRepository::new(db.clone());
        let user_repo = UserRepository::new(db.clone(), config.default_credit);

        let event_publisher: Arc<dyn RoomEventPublisher> =
            Arc::new(RedisRoomEventPublisher::new(redis_client.clone()));
        let lock_service: Arc<dyn StartGameLock> =
            Arc::new(RedisStartGameLock::new(redis_client.clone()));
        let txn_runner: Arc<dyn TransactionRunner> =
            Arc::new(SeaOrmTransactionRunner::new(db.clone()));

        let start_next_game_svc = Self::build_start_next_game(
            game_repo.clone(),
            card_repo,
            run_repo.clone(),
            run_player_repo.clone(),
            run_game_repo.clone(),
            run_event_repo.clone(),
            profile_repo.clone(),
            player_repo.clone(),
            user_repo.clone(),
            event_publisher,
            lock_service,
            txn_runner.clone(),
        );

        Self {
            room_repo: RoomRepository::new(db.clone()),
            member_repo: RoomMemberRepository::new(db.clone()),
            run_repo,
            run_player_repo,
            run_game_repo,
            run_event_repo,
            profile_repo,
            game_repo,
            player_repo,
            user_repo,
            mailer,
            config,
            redis_client,
            start_next_game_svc,
            txn_runner,
            email_tx,
            email_rx: tokio::sync::Mutex::new(Some(email_rx)),
        }
    }

    #[allow(dead_code)]
    pub fn new_with_start_next_game(
        db: sea_orm::DatabaseConnection,
        mailer: Arc<dyn Mailer>,
        config: Config,
        redis_client: Option<RedisClient>,
        start_next_game_svc: Arc<StartNextGameService>,
    ) -> Self {
        let (email_tx, email_rx) = tokio::sync::mpsc::channel(EMAIL_CHANNEL_CAPACITY);
        Self {
            room_repo: RoomRepository::new(db.clone()),
            member_repo: RoomMemberRepository::new(db.clone()),
            run_repo: GameRunRepository::new(db.clone()),
            run_player_repo: GameRunPlayerRepository::new(db.clone()),
            run_game_repo: GameRunGameRepository::new(db.clone()),
            run_event_repo: GameRunEventRepository::new(db.clone()),
            profile_repo: PlayerProfileRepository::new(db.clone()),
            game_repo: GameRepository::new(db.clone()),
            player_repo: PlayerRepository::new(db.clone()),
            user_repo: UserRepository::new(db.clone(), config.default_credit),
            mailer,
            config,
            redis_client,
            start_next_game_svc,
            txn_runner: Arc::new(SeaOrmTransactionRunner::new(db.clone())),
            email_tx,
            email_rx: tokio::sync::Mutex::new(Some(email_rx)),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn build_start_next_game(
        game_repo: GameRepository,
        card_repo: GameCardRepository,
        run_repo: GameRunRepository,
        run_player_repo: GameRunPlayerRepository,
        run_game_repo: GameRunGameRepository,
        run_event_repo: GameRunEventRepository,
        profile_repo: PlayerProfileRepository,
        player_repo: PlayerRepository,
        user_repo: UserRepository,
        event_publisher: Arc<dyn RoomEventPublisher>,
        lock_service: Arc<dyn StartGameLock>,
        txn_runner: Arc<dyn TransactionRunner>,
    ) -> Arc<StartNextGameService> {
        use crate::database::traits::{
            GameCardRepoTrait, GameRepoTrait, GameRunEventRepoTrait, GameRunGameRepoTrait,
            GameRunPlayerRepoTrait, GameRunRepoTrait, PlayerProfileRepoTrait, PlayerRepoTrait,
            UserRepoTrait,
        };

        Arc::new(StartNextGameService::new(
            Arc::new(run_repo) as Arc<dyn GameRunRepoTrait>,
            Arc::new(run_player_repo) as Arc<dyn GameRunPlayerRepoTrait>,
            Arc::new(run_game_repo) as Arc<dyn GameRunGameRepoTrait>,
            Arc::new(game_repo) as Arc<dyn GameRepoTrait>,
            Arc::new(player_repo) as Arc<dyn PlayerRepoTrait>,
            Arc::new(card_repo) as Arc<dyn GameCardRepoTrait>,
            Arc::new(profile_repo) as Arc<dyn PlayerProfileRepoTrait>,
            Arc::new(user_repo) as Arc<dyn UserRepoTrait>,
            event_publisher,
            Arc::new(run_event_repo) as Arc<dyn GameRunEventRepoTrait>,
            lock_service,
            txn_runner,
        ))
    }

    pub async fn start_email_consumer(self: &Arc<Self>) {
        let Some(mut rx) = self.email_rx.lock().await.take() else {
            tracing::warn!("Email consumer already started");
            return;
        };
        let mailer = self.mailer.clone();
        tokio::spawn(async move {
            while let Some(task) = rx.recv().await {
                if let Err(e) = mailer
                    .send_room_invitation(
                        &task.email,
                        &task.pseudo,
                        &task.room_name,
                        &task.code,
                        task.lang,
                    )
                    .await
                {
                    tracing::error!("Failed to send room invitation to {}: {}", task.email, e);
                }
            }
            tracing::info!("Email consumer shutting down");
        });
    }

    pub(crate) async fn publish_event(&self, event: &RoomEvent) {
        if let Some(mut redis) = self.redis_client.clone() {
            match redis.publish_room_event_with_retry(event).await {
                PublishResult::Published => {}
                PublishResult::RetryExhausted(e) => {
                    tracing::error!("Failed to publish room event after retries: {}", e);
                }
            }
        }
    }

    fn generate_invitation_code() -> String {
        use rand::Rng;
        let chars: Vec<char> = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789".chars().collect();
        let mut rng = rand::thread_rng();
        (0..8)
            .map(|_| chars[rng.gen_range(0..chars.len())])
            .collect()
    }

    pub async fn create_room(
        &self,
        user_id: Uuid,
        name: &str,
    ) -> Result<crate::database::models::room::Model, RoomServiceError> {
        if name.trim().is_empty() {
            return Err(RoomServiceError::NameRequired);
        }

        let code = Self::generate_invitation_code();
        let room = self.room_repo.create(user_id, name.trim(), &code).await?;

        self.member_repo.create(room.id, user_id).await?;

        Ok(room)
    }

    pub async fn list_user_rooms(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<RoomListItem>, RoomServiceError> {
        let rooms = self.room_repo.list_by_user(user_id).await?;

        if rooms.is_empty() {
            return Ok(vec![]);
        }

        let room_ids: Vec<Uuid> = rooms.iter().map(|r| r.id).collect();
        let counts = self.member_repo.count_by_rooms(&room_ids).await?;

        let result = rooms
            .iter()
            .map(|room| {
                let member_count = counts.get(&room.id).copied().unwrap_or(0);
                RoomListItem {
                    id: room.id,
                    name: room.name.clone(),
                    creator_id: room.creator_id,
                    invitation_code: room.invitation_code.clone(),
                    created_at: room.created_at.to_rfc3339(),
                    member_count,
                }
            })
            .collect();

        Ok(result)
    }

    pub async fn get_room_detail(
        &self,
        room_id: Uuid,
        user_id: Uuid,
    ) -> Result<RoomDetailResponse, RoomServiceError> {
        let room = self
            .room_repo
            .find_by_id(room_id)
            .await?
            .ok_or(RoomServiceError::RoomNotFound)?;

        let member = self.member_repo.find_membership(room_id, user_id).await?;
        if member.is_none() {
            return Err(RoomServiceError::NotMember);
        }

        let members = self.member_repo.list_by_room(room_id).await?;

        let member_user_ids: Vec<Uuid> = members.iter().map(|m| m.user_id).collect();
        let users = if member_user_ids.is_empty() {
            Vec::new()
        } else {
            self.user_repo
                .find_by_ids(&member_user_ids)
                .await
                .map_err(|_| RoomServiceError::UserNotFound)?
        };
        let user_map: HashMap<Uuid, String> = users.into_iter().map(|u| (u.id, u.pseudo)).collect();

        let member_infos: Vec<RoomMemberInfo> = members
            .iter()
            .map(|m| RoomMemberInfo {
                user_id: m.user_id,
                pseudo: user_map.get(&m.user_id).cloned().unwrap_or_default(),
                joined_at: m.joined_at.to_rfc3339(),
            })
            .collect();

        let member_count = members.len();

        let active_run = self.run_repo.find_active_by_room(room_id).await?;

        let active_run_summary = match active_run {
            Some(r) => {
                let current_game = if r.current_game_index > 0 {
                    self.run_game_repo
                        .find_by_run_and_index(r.id, r.current_game_index - 1)
                        .await?
                        .map(|rg| RoomGameInfo {
                            game_id: rg.game_id,
                            game_index: rg.game_index,
                            status: rg.status.to_string(),
                        })
                } else {
                    None
                };

                Some(ActiveRunSummary {
                    id: r.id,
                    num_games: r.num_games,
                    bet_per_game: r.bet_per_game,
                    current_game_index: r.current_game_index,
                    status: r.status.to_string(),
                    all_games_created: r.current_game_index >= r.num_games,
                    current_game,
                })
            }
            None => None,
        };

        Ok(RoomDetailResponse {
            id: room.id,
            name: room.name,
            creator_id: room.creator_id,
            invitation_code: room.invitation_code,
            created_at: room.created_at.to_rfc3339(),
            members: member_infos,
            member_count,
            active_run: active_run_summary,
        })
    }

    pub async fn join_room(
        &self,
        user_id: Uuid,
        invitation_code: &str,
    ) -> Result<crate::database::models::room::Model, RoomServiceError> {
        let room = self
            .room_repo
            .find_by_invitation_code(invitation_code)
            .await?
            .ok_or(RoomServiceError::InvalidCode)?;

        let existing = self.member_repo.find_membership(room.id, user_id).await?;
        if existing.is_some() {
            return Err(RoomServiceError::AlreadyMember);
        }

        let member_count = self.member_repo.count_by_room(room.id).await?;
        if member_count >= self.config.room_max_players as usize {
            return Err(RoomServiceError::RoomFull {
                max: self.config.room_max_players,
            });
        }

        self.member_repo.create(room.id, user_id).await?;

        if let Ok(Some(u)) = self.user_repo.find_by_id(user_id).await {
            self.publish_event(&RoomEvent::MemberJoined {
                room_id: room.id,
                user_id,
                pseudo: u.pseudo,
            })
            .await;
        }

        Ok(room)
    }

    pub async fn invite_to_room(
        &self,
        room_id: Uuid,
        user_id: Uuid,
        email: &str,
    ) -> Result<(), RoomServiceError> {
        let room = self
            .room_repo
            .find_by_id(room_id)
            .await?
            .ok_or(RoomServiceError::RoomNotFound)?;

        let member = self.member_repo.find_membership(room_id, user_id).await?;
        if member.is_none() {
            return Err(RoomServiceError::NotMember);
        }

        let user = self
            .user_repo
            .find_by_id(user_id)
            .await
            .map_err(|_| RoomServiceError::UserNotFound)?
            .ok_or(RoomServiceError::UserNotFound)?;

        let lang = Lang::parse(&user.language).unwrap_or_default();

        let task = EmailTask {
            email: email.to_string(),
            pseudo: user.pseudo.clone(),
            room_name: room.name.clone(),
            code: room.invitation_code.clone(),
            lang,
        };

        match tokio::time::timeout(std::time::Duration::from_secs(5), self.email_tx.send(task))
            .await
        {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                tracing::error!(
                    "Email consumer channel closed, cannot send invitation to {}: {}",
                    email,
                    e
                );
            }
            Err(_) => {
                tracing::warn!(
                    "Email channel full (timed out after 5s), dropping invitation for {}",
                    email
                );
            }
        }

        Ok(())
    }

    pub async fn leave_room(&self, room_id: Uuid, user_id: Uuid) -> Result<(), RoomServiceError> {
        let room = self
            .room_repo
            .find_by_id(room_id)
            .await?
            .ok_or(RoomServiceError::RoomNotFound)?;

        let member = self.member_repo.find_membership(room_id, user_id).await?;
        if member.is_none() {
            return Err(RoomServiceError::NotMember);
        }

        let active_run = self.run_repo.find_active_by_room(room_id).await?;
        if let Some(ref run) = active_run {
            let run_player = self
                .run_player_repo
                .find_by_run_and_user(run.id, user_id)
                .await?;
            if run_player.is_some() {
                return Err(RoomServiceError::LeaveBlockedByRun);
            }
        }

        self.member_repo.remove(room_id, user_id).await?;

        let remaining = self.member_repo.list_by_room(room_id).await?;

        if remaining.is_empty() {
            self.room_repo.delete(room_id).await?;
        } else if user_id == room.creator_id {
            let oldest = remaining.iter().min_by_key(|m| m.joined_at);
            if let Some(new_creator) = oldest {
                self.room_repo
                    .update_creator(room_id, new_creator.user_id)
                    .await?;
            }
        }

        if let Ok(Some(u)) = self.user_repo.find_by_id(user_id).await {
            self.publish_event(&RoomEvent::MemberLeft {
                room_id,
                user_id,
                pseudo: u.pseudo,
            })
            .await;
        }

        Ok(())
    }
}
