use super::*;
use crate::api::dto::dashboard::GameFilter;
use crate::database::models::{Game, GameCard, GameStatus, Player, PlayerProfile};
use crate::database::traits::{GameCardRepoTrait, GameRepoTrait};
use async_trait::async_trait;
use sea_orm::{DatabaseTransaction, DbErr};
use std::sync::Mutex;

struct MockGameRepo;

#[async_trait]
impl GameRepoTrait for MockGameRepo {
    async fn create(&self, _bet: i32, _auto: bool, _step_by_step: bool) -> Result<Game, DbErr> {
        unimplemented!()
    }
    async fn find_by_id(&self, _id: Uuid) -> Result<Option<Game>, DbErr> {
        Ok(None)
    }
    async fn update_rank(&self, _id: Uuid, _rank: Option<i32>) -> Result<Game, DbErr> {
        unimplemented!()
    }
    async fn update_status(&self, _id: Uuid, _status: GameStatus) -> Result<Game, DbErr> {
        unimplemented!()
    }
    async fn update_winner(&self, _id: Uuid, _winner_id: Option<Uuid>) -> Result<Game, DbErr> {
        unimplemented!()
    }
    async fn list_players(&self, _game_id: Uuid) -> Result<Vec<Player>, DbErr> {
        Ok(vec![])
    }
    async fn create_game_for_run_in_txn(
        &self,
        _txn: &DatabaseTransaction,
        _game_id: Uuid,
        _bet: i32,
        _creator_id: Option<Uuid>,
        _player_positions: serde_json::Value,
        _num_players: i16,
        _run_id: Uuid,
    ) -> Result<(), DbErr> {
        unimplemented!()
    }
}

struct MockCardRepo;

#[async_trait]
impl GameCardRepoTrait for MockCardRepo {
    async fn create(
        &self,
        _game_id: Uuid,
        _player_id: Option<Uuid>,
        _card_index: i32,
        _round: Option<i32>,
    ) -> Result<GameCard, DbErr> {
        unimplemented!()
    }
    async fn bulk_insert(&self, _cards: Vec<(Uuid, Option<Uuid>, i32)>) -> Result<(), DbErr> {
        Ok(())
    }
    async fn list_by_player(&self, _player_id: Uuid) -> Result<Vec<GameCard>, DbErr> {
        Ok(vec![])
    }
    async fn list_by_game_and_round(
        &self,
        _game_id: Uuid,
        _round: i32,
    ) -> Result<Vec<GameCard>, DbErr> {
        Ok(vec![])
    }
    async fn list_by_game(&self, _game_id: Uuid) -> Result<Vec<GameCard>, DbErr> {
        Ok(vec![])
    }
    async fn bulk_insert_in_txn(
        &self,
        _txn: &DatabaseTransaction,
        _cards: Vec<crate::database::models::game_card::ActiveModel>,
    ) -> Result<(), DbErr> {
        Ok(())
    }
}

struct MockDashboardRepo {
    profile: Mutex<Option<PlayerProfile>>,
    players: Mutex<Vec<Player>>,
    games: Mutex<HashMap<Uuid, Game>>,
}

impl MockDashboardRepo {
    fn new() -> Self {
        Self {
            profile: Mutex::new(None),
            players: Mutex::new(Vec::new()),
            games: Mutex::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl DashboardRepoTrait for MockDashboardRepo {
    async fn find_profile_by_user_id(
        &self,
        _user_id: Uuid,
    ) -> Result<Option<PlayerProfile>, DbErr> {
        Ok(self.profile.lock().unwrap().clone())
    }

    async fn list_players_for_user(&self, _user_id: Uuid) -> Result<Vec<Player>, DbErr> {
        Ok(self.players.lock().unwrap().clone())
    }

    async fn find_player_by_game_and_user(
        &self,
        game_id: Uuid,
        _user_id: Uuid,
    ) -> Result<Option<Player>, DbErr> {
        Ok(self
            .players
            .lock()
            .unwrap()
            .iter()
            .find(|p| p.game_id == game_id)
            .cloned())
    }

    async fn find_game_by_id(&self, game_id: Uuid) -> Result<Option<Game>, DbErr> {
        Ok(self.games.lock().unwrap().get(&game_id).cloned())
    }

    async fn list_players_by_game_ordered(&self, game_id: Uuid) -> Result<Vec<Player>, DbErr> {
        Ok(self
            .players
            .lock()
            .unwrap()
            .iter()
            .filter(|p| p.game_id == game_id)
            .cloned()
            .collect())
    }

    async fn list_players_for_user_filtered(
        &self,
        _user_id: Uuid,
        _filter: GameFilter,
        _page: u64,
        _per_page: u64,
    ) -> Result<(Vec<(Player, Game)>, u64), DbErr> {
        let players = self.players.lock().unwrap().clone();
        let games = self.games.lock().unwrap().clone();
        let total = players.len() as u64;
        let mut pairs = Vec::new();
        for p in &players {
            if let Some(g) = games.get(&p.game_id) {
                pairs.push((p.clone(), g.clone()));
            }
        }
        Ok((pairs, total))
    }

    async fn find_user_by_id(
        &self,
        _id: Uuid,
    ) -> Result<Option<crate::database::models::User>, DbErr> {
        Ok(None)
    }

    async fn find_user_by_pseudo(
        &self,
        _pseudo: &str,
    ) -> Result<Option<crate::database::models::User>, DbErr> {
        Ok(None)
    }

    async fn find_users_by_pseudo_prefix(
        &self,
        _prefix: &str,
        _limit: u64,
    ) -> Result<Vec<crate::database::models::User>, DbErr> {
        Ok(vec![])
    }

    async fn list_pending_invites_for_user(
        &self,
        _user_id: Uuid,
    ) -> Result<Vec<(crate::database::models::game_invite::Model, Game)>, DbErr> {
        Ok(vec![])
    }
}

fn make_service(repo: Arc<MockDashboardRepo>) -> DashboardService<MockDashboardRepo> {
    let cache = Arc::new(UserCache::new());
    let game_repo = Arc::new(MockGameRepo);
    let card_repo = Arc::new(MockCardRepo);
    DashboardService::new(repo, game_repo, card_repo, cache, 500)
}

#[tokio::test]
async fn test_get_profile_not_found_returns_defaults() {
    let repo = Arc::new(MockDashboardRepo::new());
    let service = make_service(repo);
    let resp = service.get_profile(Uuid::new_v4()).await.unwrap();
    assert_eq!(resp.credit, 500);
    assert_eq!(resp.game_played, 0);
}

#[tokio::test]
async fn test_list_games_empty() {
    let repo = Arc::new(MockDashboardRepo::new());
    let service = make_service(repo);
    let resp = service
        .list_games(
            Uuid::new_v4(),
            PaginationParams {
                page: None,
                per_page: None,
                status: None,
                order_by: None,
                bet_min: None,
                bet_max: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(resp.games.len(), 0);
    assert_eq!(resp.total, 0);
}

#[tokio::test]
async fn test_get_profile_with_frozen_until() {
    use crate::database::models::PlayerType as ModelPlayerType;

    let repo = Arc::new(MockDashboardRepo::new());
    let frozen_time = chrono::Utc::now() + chrono::Duration::hours(1);

    let profile = PlayerProfile {
        id: Uuid::new_v4(),
        user_id: Uuid::new_v4(),
        player_type: ModelPlayerType::Human,
        credit: 0,
        game_played: 5,
        wins: 2,
        kora_wins: 0,
        winning_streak: 0,
        latitude: None,
        longitude: None,
        country_code: None,
        city: None,
        frozen_until: Some(frozen_time),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    *repo.profile.lock().unwrap() = Some(profile.clone());

    let service = make_service(repo);
    let resp = service.get_profile(profile.user_id).await.unwrap();
    assert_eq!(resp.credit, 0);
    assert!(resp.frozen_until.is_some());
}

#[tokio::test]
async fn test_get_profile_not_frozen() {
    let repo = Arc::new(MockDashboardRepo::new());
    let service = make_service(repo);
    let resp = service.get_profile(Uuid::new_v4()).await.unwrap();
    assert_eq!(resp.credit, 500);
    assert!(resp.frozen_until.is_none());
}
