use async_trait::async_trait;
use sea_orm::{
    ColumnTrait, DatabaseConnection, DbErr, EntityTrait, JoinType, PaginatorTrait, QueryFilter,
    QueryOrder, QuerySelect, RelationTrait,
};
use uuid::Uuid;

use crate::api::dto::dashboard::GameFilter;
use crate::database::models::{
    game, game_invite, player, player_profile, user, Game, GameStatus, InviteStatus, Player,
    PlayerProfile, User,
};
use crate::database::traits::DashboardRepoTrait;

pub struct DashboardRepository {
    connection: DatabaseConnection,
}

impl DashboardRepository {
    pub fn new(connection: DatabaseConnection) -> Self {
        Self { connection }
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn find_profile_by_user_id(
        &self,
        user_id: Uuid,
    ) -> Result<Option<PlayerProfile>, DbErr> {
        player_profile::Entity::find()
            .filter(player_profile::Column::UserId.eq(user_id))
            .one(&self.connection)
            .await
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn list_players_for_user(&self, user_id: Uuid) -> Result<Vec<Player>, DbErr> {
        player::Entity::find()
            .filter(player::Column::UserId.eq(user_id))
            .order_by_desc(player::Column::CreatedAt)
            .all(&self.connection)
            .await
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn find_player_by_game_and_user(
        &self,
        game_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<Player>, DbErr> {
        player::Entity::find()
            .filter(player::Column::GameId.eq(game_id))
            .filter(player::Column::UserId.eq(user_id))
            .one(&self.connection)
            .await
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn find_game_by_id(&self, game_id: Uuid) -> Result<Option<Game>, DbErr> {
        game::Entity::find_by_id(game_id)
            .one(&self.connection)
            .await
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn list_players_by_game_ordered(&self, game_id: Uuid) -> Result<Vec<Player>, DbErr> {
        player::Entity::find()
            .filter(player::Column::GameId.eq(game_id))
            .order_by_asc(player::Column::Position)
            .all(&self.connection)
            .await
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    pub async fn list_players_for_user_filtered(
        &self,
        user_id: Uuid,
        filter: GameFilter,
        page: u64,
        per_page: u64,
    ) -> Result<(Vec<(Player, Game)>, u64), DbErr> {
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

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    async fn find_user_by_id(&self, id: Uuid) -> Result<Option<User>, DbErr> {
        user::Entity::find_by_id(id).one(&self.connection).await
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
    async fn find_user_by_pseudo(&self, pseudo: &str) -> Result<Option<User>, DbErr> {
        user::Entity::find()
            .filter(user::Column::Pseudo.eq(pseudo))
            .one(&self.connection)
            .await
    }

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
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

    #[tracing::instrument(skip(self), fields(db.statement, db.rows_affected))]
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
                if game.status == GameStatus::Pending || game.status == GameStatus::Ready {
                    results.push((invite, game));
                }
            }
        }
        Ok(results)
    }
}
