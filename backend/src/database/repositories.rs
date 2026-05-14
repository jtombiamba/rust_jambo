use async_trait::async_trait;
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, DatabaseConnection, DbErr, EntityTrait, JoinType,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, RelationTrait, Set, TransactionTrait,
};
use serde_json::json;
use uuid::Uuid;

use crate::api::dto::dashboard::GameFilter;
use crate::database::models::{
    game, game_invite, player_profile, user, Game, GameCard, GameMode, GameStatus, InviteStatus,
    Player, PlayerProfile, PlayerType, User,
};
use crate::database::traits::{
    DashboardRepoTrait, GameCardRepoTrait, GameInviteRepoTrait, GameRepoTrait,
    PlayerProfileRepoTrait, PlayerRepoTrait, UserRepoTrait,
};

pub struct UserRepository {
    connection: DatabaseConnection,
}

impl UserRepository {
    pub fn new(connection: DatabaseConnection) -> Self {
        Self { connection }
    }

    pub async fn find_by_email(&self, email: &str) -> Result<Option<User>, DbErr> {
        user::Entity::find()
            .filter(user::Column::Email.eq(email))
            .one(&self.connection)
            .await
    }

    pub async fn find_by_id(&self, id: Uuid) -> Result<Option<User>, DbErr> {
        user::Entity::find_by_id(id).one(&self.connection).await
    }

    pub async fn find_by_pseudo(&self, pseudo: &str) -> Result<Option<User>, DbErr> {
        user::Entity::find()
            .filter(user::Column::Pseudo.eq(pseudo))
            .one(&self.connection)
            .await
    }

    pub async fn find_by_pseudo_prefix(
        &self,
        prefix: &str,
        limit: u64,
    ) -> Result<Vec<User>, DbErr> {
        user::Entity::find()
            .filter(user::Column::Pseudo.starts_with(prefix))
            .limit(limit)
            .all(&self.connection)
            .await
    }

    pub async fn create_user_with_profile(
        &self,
        pseudo: &str,
        email: &str,
        password_hash: &str,
        ip_hash: Option<&str>,
    ) -> Result<(User, PlayerProfile), DbErr> {
        let now = chrono::Utc::now();
        let user_id = Uuid::new_v4();
        let profile_id = Uuid::new_v4();
        let pseudo = pseudo.to_string();
        let email = email.to_string();
        let password_hash = password_hash.to_string();
        let ip_hash = ip_hash.map(|s| s.to_string());

        self.connection
            .transaction(|txn| {
                Box::pin(async move {
                    let user_active = user::ActiveModel {
                        id: Set(user_id),
                        pseudo: Set(pseudo),
                        email: Set(email),
                        password_hash: Set(password_hash),
                        last_ip_hash: Set(ip_hash),
                        created_at: Set(now),
                        updated_at: Set(now),
                    };
                    let user = user_active.insert(txn).await?;

                    let profile_active = player_profile::ActiveModel {
                        id: Set(profile_id),
                        user_id: Set(user_id),
                        player_type: Set(PlayerType::Human),
                        credit: Set(500),
                        game_played: Set(0),
                        wins: Set(0),
                        kora_wins: Set(0),
                        winning_streak: Set(0),
                        latitude: ActiveValue::NotSet,
                        longitude: ActiveValue::NotSet,
                        country_code: ActiveValue::NotSet,
                        city: ActiveValue::NotSet,
                        created_at: Set(now),
                        updated_at: Set(now),
                    };
                    let profile = profile_active.insert(txn).await?;

                    Ok::<_, DbErr>((user, profile))
                })
            })
            .await
            .map_err(|e: sea_orm::TransactionError<DbErr>| {
                DbErr::Custom(format!("Transaction failed: {}", e))
            })
    }

    pub async fn update_password_hash(&self, id: Uuid, hash: &str) -> Result<User, DbErr> {
        let user_model = user::Entity::find_by_id(id)
            .one(&self.connection)
            .await?
            .ok_or_else(|| DbErr::Custom("User not found".to_string()))?;
        let mut active: user::ActiveModel = user_model.into();
        active.password_hash = Set(hash.to_string());
        active.updated_at = Set(chrono::Utc::now());
        active.update(&self.connection).await
    }

    pub async fn update_last_ip_hash(&self, id: Uuid, hash: &str) -> Result<User, DbErr> {
        let user_model = user::Entity::find_by_id(id)
            .one(&self.connection)
            .await?
            .ok_or_else(|| DbErr::Custom("User not found".to_string()))?;
        let mut active: user::ActiveModel = user_model.into();
        active.last_ip_hash = Set(Some(hash.to_string()));
        active.updated_at = Set(chrono::Utc::now());
        active.update(&self.connection).await
    }
}

#[async_trait]
impl UserRepoTrait for UserRepository {
    async fn find_by_email(&self, email: &str) -> Result<Option<User>, DbErr> {
        self.find_by_email(email).await
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<User>, DbErr> {
        self.find_by_id(id).await
    }

    async fn find_by_pseudo(&self, pseudo: &str) -> Result<Option<User>, DbErr> {
        self.find_by_pseudo(pseudo).await
    }

    async fn find_by_pseudo_prefix(&self, prefix: &str, limit: u64) -> Result<Vec<User>, DbErr> {
        self.find_by_pseudo_prefix(prefix, limit).await
    }

    async fn create_user_with_profile(
        &self,
        pseudo: &str,
        email: &str,
        password_hash: &str,
        ip_hash: Option<&str>,
    ) -> Result<(User, PlayerProfile), DbErr> {
        self.create_user_with_profile(pseudo, email, password_hash, ip_hash)
            .await
    }

    async fn update_password_hash(&self, id: Uuid, hash: &str) -> Result<User, DbErr> {
        self.update_password_hash(id, hash).await
    }

    async fn update_last_ip_hash(&self, id: Uuid, hash: &str) -> Result<User, DbErr> {
        self.update_last_ip_hash(id, hash).await
    }
}

#[allow(dead_code)]
pub struct PlayerProfileRepository {
    connection: DatabaseConnection,
}

impl PlayerProfileRepository {
    #[allow(dead_code)]
    pub fn new(connection: DatabaseConnection) -> Self {
        Self { connection }
    }

    pub async fn list_all(&self) -> Result<Vec<PlayerProfile>, DbErr> {
        player_profile::Entity::find().all(&self.connection).await
    }

    pub async fn find_by_user_id(&self, user_id: Uuid) -> Result<Option<PlayerProfile>, DbErr> {
        player_profile::Entity::find()
            .filter(player_profile::Column::UserId.eq(user_id))
            .one(&self.connection)
            .await
    }

    pub async fn update_stats(
        &self,
        user_id: Uuid,
        wins_delta: i32,
        kora_wins_delta: i32,
    ) -> Result<PlayerProfile, DbErr> {
        let profile = player_profile::Entity::find()
            .filter(player_profile::Column::UserId.eq(user_id))
            .one(&self.connection)
            .await?
            .ok_or_else(|| DbErr::Custom("PlayerProfile not found".to_string()))?;

        let mut active: player_profile::ActiveModel = profile.into();
        active.game_played = Set(active.game_played.unwrap() + 1);
        active.wins = Set(active.wins.unwrap() + wins_delta);
        active.kora_wins = Set(active.kora_wins.unwrap() + kora_wins_delta);
        active.updated_at = Set(chrono::Utc::now());
        active.update(&self.connection).await
    }

    pub async fn update_credit(&self, user_id: Uuid, credit: i32) -> Result<PlayerProfile, DbErr> {
        let profile = player_profile::Entity::find()
            .filter(player_profile::Column::UserId.eq(user_id))
            .one(&self.connection)
            .await?
            .ok_or_else(|| DbErr::Custom("PlayerProfile not found".to_string()))?;
        let mut active: player_profile::ActiveModel = profile.into();
        active.credit = Set(credit);
        active.updated_at = Set(chrono::Utc::now());
        active.update(&self.connection).await
    }
}

#[async_trait]
#[allow(dead_code)]
impl PlayerProfileRepoTrait for PlayerProfileRepository {
    async fn find_by_user_id(&self, user_id: Uuid) -> Result<Option<PlayerProfile>, DbErr> {
        self.find_by_user_id(user_id).await
    }

    async fn update_stats(
        &self,
        user_id: Uuid,
        wins_delta: i32,
        kora_wins_delta: i32,
    ) -> Result<PlayerProfile, DbErr> {
        self.update_stats(user_id, wins_delta, kora_wins_delta)
            .await
    }
}

pub struct GameRepository {
    connection: DatabaseConnection,
}

impl GameRepository {
    pub fn new(connection: DatabaseConnection) -> Self {
        Self { connection }
    }

    pub async fn create(&self, bet: i32, auto: bool) -> Result<Game, DbErr> {
        let id = Uuid::new_v4();
        let now = chrono::Utc::now();

        let game_active = game::ActiveModel {
            id: Set(id),
            status: Set(GameStatus::Pending),
            bet: Set(bet),
            created_at: Set(now),
            updated_at: Set(now),
            finished_at: ActiveValue::NotSet,
            rank: ActiveValue::NotSet,
            roll: Set(1),
            auto: Set(auto),
            winner_id: ActiveValue::NotSet,
            player_positions: Set(json!({})),
            current_winning_card: ActiveValue::NotSet,
            current_winning_player_position: ActiveValue::NotSet,
            creator_id: ActiveValue::NotSet,
            game_mode: Set(GameMode::Solo),
            max_players: Set(4),
            invite_expires_at: ActiveValue::NotSet,
        };
        let insert_result = game::Entity::insert(game_active)
            .exec(&self.connection)
            .await?;
        let inserted_id = insert_result.last_insert_id;
        let game = game::Entity::find_by_id(inserted_id)
            .one(&self.connection)
            .await?
            .ok_or_else(|| DbErr::Custom("Game not found after insertion".to_string()))?;
        Ok(game)
    }

    pub async fn find_by_id(&self, id: Uuid) -> Result<Option<Game>, DbErr> {
        game::Entity::find_by_id(id).one(&self.connection).await
    }

    pub async fn update_rank(&self, id: Uuid, rank: Option<i32>) -> Result<Game, DbErr> {
        let mut active: game::ActiveModel = game::Entity::find_by_id(id)
            .one(&self.connection)
            .await?
            .ok_or_else(|| DbErr::Custom("Game not found".to_string()))?
            .into();
        active.rank = Set(rank);
        active.updated_at = Set(chrono::Utc::now());
        active.update(&self.connection).await
    }

    pub async fn update_status(&self, id: Uuid, status: GameStatus) -> Result<Game, DbErr> {
        let mut active: game::ActiveModel = game::Entity::find_by_id(id)
            .one(&self.connection)
            .await?
            .ok_or_else(|| DbErr::Custom("Game not found".to_string()))?
            .into();
        active.status = Set(status);
        active.updated_at = Set(chrono::Utc::now());
        active.update(&self.connection).await
    }

    #[allow(dead_code)]
    pub async fn update_winner(&self, id: Uuid, winner_id: Option<Uuid>) -> Result<Game, DbErr> {
        let mut active: game::ActiveModel = game::Entity::find_by_id(id)
            .one(&self.connection)
            .await?
            .ok_or_else(|| DbErr::Custom("Game not found".to_string()))?
            .into();
        active.winner_id = Set(winner_id);
        active.updated_at = Set(chrono::Utc::now());
        active.update(&self.connection).await
    }

    #[allow(dead_code)]
    pub async fn update_player_positions(
        &self,
        id: Uuid,
        player_id: Uuid,
        position: i32,
    ) -> Result<Game, DbErr> {
        let game_model = game::Entity::find_by_id(id)
            .one(&self.connection)
            .await?
            .ok_or_else(|| DbErr::Custom("Game not found".to_string()))?;

        let current_positions = game_model.player_positions.clone();
        let mut positions_map = current_positions.as_object().cloned().unwrap_or_default();
        positions_map.insert(player_id.to_string(), json!(position));

        let mut active: game::ActiveModel = game_model.into();
        active.player_positions = Set(json!(positions_map));
        active.updated_at = Set(chrono::Utc::now());
        active.update(&self.connection).await
    }

    #[allow(dead_code)]
    pub async fn list_players(&self, game_id: Uuid) -> Result<Vec<Player>, DbErr> {
        use crate::database::models::player;
        player::Entity::find()
            .filter(player::Column::GameId.eq(game_id))
            .order_by_asc(player::Column::Position)
            .all(&self.connection)
            .await
    }
}

#[async_trait]
#[allow(dead_code)]
impl GameRepoTrait for GameRepository {
    async fn create(&self, bet: i32, auto: bool) -> Result<Game, DbErr> {
        self.create(bet, auto).await
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<Game>, DbErr> {
        self.find_by_id(id).await
    }

    async fn update_rank(&self, id: Uuid, rank: Option<i32>) -> Result<Game, DbErr> {
        self.update_rank(id, rank).await
    }

    async fn update_status(&self, id: Uuid, status: GameStatus) -> Result<Game, DbErr> {
        self.update_status(id, status).await
    }

    async fn update_winner(&self, id: Uuid, winner_id: Option<Uuid>) -> Result<Game, DbErr> {
        self.update_winner(id, winner_id).await
    }

    async fn list_players(&self, game_id: Uuid) -> Result<Vec<Player>, DbErr> {
        self.list_players(game_id).await
    }
}

pub struct PlayerRepository {
    connection: DatabaseConnection,
}

impl PlayerRepository {
    pub fn new(connection: DatabaseConnection) -> Self {
        Self { connection }
    }

    pub async fn create(
        &self,
        game_id: Uuid,
        player_type: PlayerType,
        name: &str,
        position: i32,
    ) -> Result<Player, DbErr> {
        use crate::database::models::player;
        let id = Uuid::new_v4();
        let now = chrono::Utc::now();

        let player_active = player::ActiveModel {
            id: Set(id),
            game_id: Set(game_id),
            player_type: Set(player_type),
            name: Set(name.to_string()),
            position: Set(position),
            credits: Set(500),
            created_at: Set(now),
            user_id: ActiveValue::NotSet,
        };
        let insert_result = player::Entity::insert(player_active)
            .exec(&self.connection)
            .await?;
        let inserted_id = insert_result.last_insert_id;
        let player = player::Entity::find_by_id(inserted_id)
            .one(&self.connection)
            .await?
            .ok_or_else(|| DbErr::Custom("Player not found after insertion".to_string()))?;
        Ok(player)
    }

    pub async fn list_by_game(&self, game_id: Uuid) -> Result<Vec<Player>, DbErr> {
        use crate::database::models::player;
        player::Entity::find()
            .filter(player::Column::GameId.eq(game_id))
            .order_by_asc(player::Column::Position)
            .all(&self.connection)
            .await
    }

    pub async fn update_credits(&self, player_id: Uuid, credits: i32) -> Result<Player, DbErr> {
        use crate::database::models::player;
        let model = player::Entity::find_by_id(player_id)
            .one(&self.connection)
            .await?
            .ok_or_else(|| DbErr::Custom("Player not found".to_string()))?;
        let mut active: player::ActiveModel = model.into();
        active.credits = Set(credits);
        active.update(&self.connection).await
    }

    pub async fn create_with_user(
        &self,
        game_id: Uuid,
        player_type: PlayerType,
        name: &str,
        position: i32,
        user_id: Uuid,
    ) -> Result<Player, DbErr> {
        use crate::database::models::player;
        let id = Uuid::new_v4();
        let now = chrono::Utc::now();

        let player_active = player::ActiveModel {
            id: Set(id),
            game_id: Set(game_id),
            player_type: Set(player_type),
            name: Set(name.to_string()),
            position: Set(position),
            credits: Set(500),
            created_at: Set(now),
            user_id: Set(Some(user_id)),
        };
        let insert_result = player::Entity::insert(player_active)
            .exec(&self.connection)
            .await?;
        let inserted_id = insert_result.last_insert_id;
        let player = player::Entity::find_by_id(inserted_id)
            .one(&self.connection)
            .await?
            .ok_or_else(|| DbErr::Custom("Player not found after insertion".to_string()))?;
        Ok(player)
    }

    #[allow(dead_code)]
    pub async fn find_by_game_and_user(
        &self,
        game_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<Player>, DbErr> {
        use crate::database::models::player;
        player::Entity::find()
            .filter(player::Column::GameId.eq(game_id))
            .filter(player::Column::UserId.eq(user_id))
            .one(&self.connection)
            .await
    }
}

#[async_trait]
#[allow(dead_code)]
impl PlayerRepoTrait for PlayerRepository {
    async fn create(
        &self,
        game_id: Uuid,
        player_type: PlayerType,
        name: &str,
        position: i32,
    ) -> Result<Player, DbErr> {
        self.create(game_id, player_type, name, position).await
    }

    async fn create_with_user(
        &self,
        game_id: Uuid,
        player_type: PlayerType,
        name: &str,
        position: i32,
        user_id: Uuid,
    ) -> Result<Player, DbErr> {
        self.create_with_user(game_id, player_type, name, position, user_id)
            .await
    }

    async fn list_by_game(&self, game_id: Uuid) -> Result<Vec<Player>, DbErr> {
        self.list_by_game(game_id).await
    }

    async fn find_by_game_and_user(
        &self,
        game_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<Player>, DbErr> {
        self.find_by_game_and_user(game_id, user_id).await
    }
}

pub struct GameCardRepository {
    connection: DatabaseConnection,
}

impl GameCardRepository {
    pub fn new(connection: DatabaseConnection) -> Self {
        Self { connection }
    }

    pub async fn create(
        &self,
        game_id: Uuid,
        player_id: Option<Uuid>,
        card_index: i32,
        round: Option<i32>,
    ) -> Result<GameCard, DbErr> {
        use crate::database::models::game_card;
        let id = Uuid::new_v4();
        let now = chrono::Utc::now();

        let card_active = game_card::ActiveModel {
            id: Set(id),
            game_id: Set(game_id),
            player_id: Set(player_id),
            card_index: Set(card_index),
            played: Set(false),
            played_at: ActiveValue::NotSet,
            round: Set(round),
            created_at: Set(now),
        };
        let insert_result = game_card::Entity::insert(card_active)
            .exec(&self.connection)
            .await?;
        let inserted_id = insert_result.last_insert_id;
        let card = game_card::Entity::find_by_id(inserted_id)
            .one(&self.connection)
            .await?
            .ok_or_else(|| DbErr::Custom("GameCard not found after insertion".to_string()))?;
        Ok(card)
    }

    #[allow(dead_code)]
    pub async fn bulk_insert(&self, cards: Vec<(Uuid, Option<Uuid>, i32)>) -> Result<(), DbErr> {
        use crate::database::models::game_card;
        let now = chrono::Utc::now();
        let active_models: Vec<game_card::ActiveModel> = cards
            .into_iter()
            .map(|(game_id, player_id, card_index)| game_card::ActiveModel {
                id: Set(Uuid::new_v4()),
                game_id: Set(game_id),
                player_id: Set(player_id),
                card_index: Set(card_index),
                played: Set(false),
                played_at: ActiveValue::NotSet,
                round: Set(None),
                created_at: Set(now),
            })
            .collect();
        if !active_models.is_empty() {
            game_card::Entity::insert_many(active_models)
                .exec(&self.connection)
                .await?;
        }
        Ok(())
    }

    pub async fn list_by_player(&self, player_id: Uuid) -> Result<Vec<GameCard>, DbErr> {
        use crate::database::models::game_card;
        game_card::Entity::find()
            .filter(game_card::Column::PlayerId.eq(player_id))
            .order_by_asc(game_card::Column::CardIndex)
            .all(&self.connection)
            .await
    }

    pub async fn list_by_game_and_round(
        &self,
        game_id: Uuid,
        round: i32,
    ) -> Result<Vec<GameCard>, DbErr> {
        use crate::database::models::game_card;
        game_card::Entity::find()
            .filter(game_card::Column::GameId.eq(game_id))
            .filter(game_card::Column::Round.eq(round))
            .filter(game_card::Column::Played.eq(true))
            .order_by_asc(game_card::Column::CardIndex)
            .all(&self.connection)
            .await
    }

    #[allow(dead_code)]
    pub async fn list_by_player_and_round(
        &self,
        player_id: Uuid,
        round: i32,
    ) -> Result<Vec<GameCard>, DbErr> {
        use crate::database::models::game_card;
        game_card::Entity::find()
            .filter(game_card::Column::PlayerId.eq(player_id))
            .filter(game_card::Column::Round.eq(round))
            .order_by_asc(game_card::Column::CardIndex)
            .all(&self.connection)
            .await
    }
}

#[async_trait]
#[allow(dead_code)]
impl GameCardRepoTrait for GameCardRepository {
    async fn create(
        &self,
        game_id: Uuid,
        player_id: Option<Uuid>,
        card_index: i32,
        round: Option<i32>,
    ) -> Result<GameCard, DbErr> {
        self.create(game_id, player_id, card_index, round).await
    }

    async fn bulk_insert(&self, cards: Vec<(Uuid, Option<Uuid>, i32)>) -> Result<(), DbErr> {
        self.bulk_insert(cards).await
    }

    async fn list_by_player(&self, player_id: Uuid) -> Result<Vec<GameCard>, DbErr> {
        self.list_by_player(player_id).await
    }

    async fn list_by_game_and_round(
        &self,
        game_id: Uuid,
        round: i32,
    ) -> Result<Vec<GameCard>, DbErr> {
        self.list_by_game_and_round(game_id, round).await
    }
}

pub struct DashboardRepository {
    connection: DatabaseConnection,
}

impl DashboardRepository {
    pub fn new(connection: DatabaseConnection) -> Self {
        Self { connection }
    }

    pub async fn find_profile_by_user_id(
        &self,
        user_id: Uuid,
    ) -> Result<Option<PlayerProfile>, DbErr> {
        player_profile::Entity::find()
            .filter(player_profile::Column::UserId.eq(user_id))
            .one(&self.connection)
            .await
    }

    pub async fn list_players_for_user(&self, user_id: Uuid) -> Result<Vec<Player>, DbErr> {
        use crate::database::models::player;
        player::Entity::find()
            .filter(player::Column::UserId.eq(user_id))
            .order_by_desc(player::Column::CreatedAt)
            .all(&self.connection)
            .await
    }

    pub async fn find_player_by_game_and_user(
        &self,
        game_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<Player>, DbErr> {
        use crate::database::models::player;
        player::Entity::find()
            .filter(player::Column::GameId.eq(game_id))
            .filter(player::Column::UserId.eq(user_id))
            .one(&self.connection)
            .await
    }

    pub async fn find_game_by_id(&self, game_id: Uuid) -> Result<Option<Game>, DbErr> {
        game::Entity::find_by_id(game_id)
            .one(&self.connection)
            .await
    }

    pub async fn list_players_by_game_ordered(&self, game_id: Uuid) -> Result<Vec<Player>, DbErr> {
        use crate::database::models::player;
        player::Entity::find()
            .filter(player::Column::GameId.eq(game_id))
            .order_by_asc(player::Column::Position)
            .all(&self.connection)
            .await
    }

    pub async fn find_cards_for_player(
        &self,
        player_id: Uuid,
        unplayed_only: bool,
    ) -> Result<Vec<GameCard>, DbErr> {
        use crate::database::models::game_card;
        let mut query = game_card::Entity::find()
            .filter(game_card::Column::PlayerId.eq(player_id))
            .order_by_asc(game_card::Column::CardIndex);
        if unplayed_only {
            query = query.filter(game_card::Column::Played.eq(false));
        }
        query.all(&self.connection).await
    }

    pub async fn find_all_cards_for_game(&self, game_id: Uuid) -> Result<Vec<GameCard>, DbErr> {
        use crate::database::models::game_card;
        game_card::Entity::find()
            .filter(game_card::Column::GameId.eq(game_id))
            .all(&self.connection)
            .await
    }

    pub async fn list_players_for_user_filtered(
        &self,
        user_id: Uuid,
        filter: GameFilter,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<(Player, Game)>, u64), DbErr> {
        use crate::database::models::{game, player};

        let mut base_query = player::Entity::find()
            .filter(player::Column::UserId.eq(user_id))
            .join(JoinType::InnerJoin, player::Relation::Game.def());

        if !filter.statuses.is_empty() {
            let mut condition = sea_orm::Condition::any();
            for s in &filter.statuses {
                let status = match s.as_str() {
                    "pending" => GameStatus::Pending,
                    "active" => GameStatus::Active,
                    "finished" => GameStatus::Finished,
                    "cancelled" => GameStatus::Cancelled,
                    "kora" => GameStatus::Kora,
                    "double_kora" => GameStatus::DoubleKora,
                    "ready" => GameStatus::Ready,
                    _ => continue,
                };
                condition = condition.add(game::Column::Status.eq(status));
            }
            if !filter.statuses.is_empty() {
                base_query = base_query.filter(condition);
            }
        }

        if let Some(bet_min) = filter.bet_min {
            base_query = base_query.filter(game::Column::Bet.gte(bet_min));
        }
        if let Some(bet_max) = filter.bet_max {
            base_query = base_query.filter(game::Column::Bet.lte(bet_max));
        }

        let count_query = base_query.clone();
        let total = count_query.count(&self.connection).await?;

        let (order_col, order_dir) = match filter.order_by.as_str() {
            "date_asc" => (game::Column::CreatedAt, sea_orm::Order::Asc),
            "bet_desc" => (game::Column::Bet, sea_orm::Order::Desc),
            "bet_asc" => (game::Column::Bet, sea_orm::Order::Asc),
            _ => (game::Column::CreatedAt, sea_orm::Order::Desc),
        };

        let offset = page.saturating_sub(1) * per_page;
        let results = base_query
            .order_by(order_col, order_dir)
            .offset(offset)
            .limit(per_page)
            .all(&self.connection)
            .await?;

        let game_ids: Vec<Uuid> = results.iter().map(|p| p.game_id).collect();
        let games_map: std::collections::HashMap<Uuid, Game> = if game_ids.is_empty() {
            std::collections::HashMap::new()
        } else {
            game::Entity::find()
                .filter(game::Column::Id.is_in(game_ids))
                .all(&self.connection)
                .await?
                .into_iter()
                .map(|g| (g.id, g))
                .collect()
        };

        let pairs: Vec<(Player, Game)> = results
            .into_iter()
            .filter_map(|player| {
                games_map
                    .get(&player.game_id)
                    .map(|game| (player, game.clone()))
            })
            .collect();

        Ok((pairs, total))
    }
}

#[async_trait]
impl DashboardRepoTrait for DashboardRepository {
    async fn find_profile_by_user_id(&self, user_id: Uuid) -> Result<Option<PlayerProfile>, DbErr> {
        self.find_profile_by_user_id(user_id).await
    }

    async fn list_players_for_user(&self, user_id: Uuid) -> Result<Vec<Player>, DbErr> {
        self.list_players_for_user(user_id).await
    }

    async fn find_player_by_game_and_user(
        &self,
        game_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<Player>, DbErr> {
        self.find_player_by_game_and_user(game_id, user_id).await
    }

    async fn find_game_by_id(&self, game_id: Uuid) -> Result<Option<Game>, DbErr> {
        self.find_game_by_id(game_id).await
    }

    async fn list_players_by_game_ordered(&self, game_id: Uuid) -> Result<Vec<Player>, DbErr> {
        self.list_players_by_game_ordered(game_id).await
    }

    async fn find_cards_for_player(
        &self,
        player_id: Uuid,
        unplayed_only: bool,
    ) -> Result<Vec<GameCard>, DbErr> {
        self.find_cards_for_player(player_id, unplayed_only).await
    }

    async fn find_all_cards_for_game(&self, game_id: Uuid) -> Result<Vec<GameCard>, DbErr> {
        self.find_all_cards_for_game(game_id).await
    }

    async fn list_players_for_user_filtered(
        &self,
        user_id: Uuid,
        filter: GameFilter,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<(Player, Game)>, u64), DbErr> {
        self.list_players_for_user_filtered(user_id, filter, page, per_page)
            .await
    }

    async fn find_user_by_id(&self, id: Uuid) -> Result<Option<User>, DbErr> {
        user::Entity::find_by_id(id).one(&self.connection).await
    }

    async fn find_user_by_pseudo(&self, pseudo: &str) -> Result<Option<User>, DbErr> {
        user::Entity::find()
            .filter(user::Column::Pseudo.eq(pseudo))
            .one(&self.connection)
            .await
    }

    async fn find_users_by_pseudo_prefix(
        &self,
        prefix: &str,
        limit: u64,
    ) -> Result<Vec<User>, DbErr> {
        user::Entity::find()
            .filter(user::Column::Pseudo.starts_with(prefix))
            .limit(limit)
            .all(&self.connection)
            .await
    }

    async fn list_pending_invites_for_user(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<(game_invite::Model, Game)>, DbErr> {
        let invites = game_invite::Entity::find()
            .filter(game_invite::Column::InvitedUserId.eq(user_id))
            .filter(game_invite::Column::Status.eq(InviteStatus::Pending))
            .all(&self.connection)
            .await?;

        let mut results = Vec::new();
        for invite in invites {
            if let Some(game) = game::Entity::find_by_id(invite.game_id)
                .one(&self.connection)
                .await?
            {
                results.push((invite, game));
            }
        }
        Ok(results)
    }
}

pub struct GameInviteRepository {
    connection: DatabaseConnection,
}

#[allow(dead_code)]
impl GameInviteRepository {
    pub fn new(connection: DatabaseConnection) -> Self {
        Self { connection }
    }

    pub async fn create_invite(
        &self,
        game_id: Uuid,
        invited_user_id: Uuid,
    ) -> Result<game_invite::Model, DbErr> {
        let id = Uuid::new_v4();
        let now = chrono::Utc::now();

        let invite_active = game_invite::ActiveModel {
            id: Set(id),
            game_id: Set(game_id),
            invited_user_id: Set(invited_user_id),
            status: Set(InviteStatus::Pending),
            created_at: Set(now),
        };
        let insert_result = game_invite::Entity::insert(invite_active)
            .exec(&self.connection)
            .await?;
        let inserted_id = insert_result.last_insert_id;
        game_invite::Entity::find_by_id(inserted_id)
            .one(&self.connection)
            .await?
            .ok_or_else(|| DbErr::Custom("GameInvite not found after insertion".to_string()))
    }

    pub async fn find_invite(
        &self,
        game_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<game_invite::Model>, DbErr> {
        game_invite::Entity::find()
            .filter(game_invite::Column::GameId.eq(game_id))
            .filter(game_invite::Column::InvitedUserId.eq(user_id))
            .one(&self.connection)
            .await
    }

    pub async fn update_invite_status(
        &self,
        invite_id: Uuid,
        status: InviteStatus,
    ) -> Result<game_invite::Model, DbErr> {
        let model = game_invite::Entity::find_by_id(invite_id)
            .one(&self.connection)
            .await?
            .ok_or_else(|| DbErr::Custom("GameInvite not found".to_string()))?;
        let mut active: game_invite::ActiveModel = model.into();
        active.status = Set(status);
        active.update(&self.connection).await
    }

    pub async fn find_by_id(&self, invite_id: Uuid) -> Result<Option<game_invite::Model>, DbErr> {
        game_invite::Entity::find_by_id(invite_id)
            .one(&self.connection)
            .await
    }

    pub async fn list_pending_invites_for_user(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<(game_invite::Model, Game)>, DbErr> {
        let invites = game_invite::Entity::find()
            .filter(game_invite::Column::InvitedUserId.eq(user_id))
            .filter(game_invite::Column::Status.eq(InviteStatus::Pending))
            .all(&self.connection)
            .await?;

        let mut results = Vec::new();
        for invite in invites {
            if let Some(game) = game::Entity::find_by_id(invite.game_id)
                .one(&self.connection)
                .await?
            {
                results.push((invite, game));
            }
        }
        Ok(results)
    }
}

#[async_trait]
impl GameInviteRepoTrait for GameInviteRepository {
    async fn create_invite(
        &self,
        game_id: Uuid,
        invited_user_id: Uuid,
    ) -> Result<game_invite::Model, DbErr> {
        self.create_invite(game_id, invited_user_id).await
    }

    async fn find_invite(
        &self,
        game_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<game_invite::Model>, DbErr> {
        self.find_invite(game_id, user_id).await
    }

    async fn update_invite_status(
        &self,
        invite_id: Uuid,
        status: InviteStatus,
    ) -> Result<game_invite::Model, DbErr> {
        self.update_invite_status(invite_id, status).await
    }

    async fn list_pending_invites_for_user(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<(game_invite::Model, Game)>, DbErr> {
        self.list_pending_invites_for_user(user_id).await
    }
}
