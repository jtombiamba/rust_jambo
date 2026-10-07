use std::collections::HashSet;
use std::sync::Arc;

use serde::de::DeserializeOwned;
use tracing::error;
use uuid::Uuid;

use crate::api::dto::dashboard::{
    GameHistoryItem, GameHistoryResponse, PaginationParams, PlayerProfileResponse,
};
use crate::api::dto::requests::UserSearchQuery;
use crate::api::dto::responses::{
    InvitationItem, InvitationsResponse, QuickGameResponse, UserSearchItem, UserSearchResponse,
};
use crate::api::services::game_state::build_game_state_response;
use crate::cache::UserCache;
use crate::database::models::{GameStatus, User};
use crate::database::traits::{DashboardRepoTrait, GameCardRepoTrait, GameRepoTrait};
use crate::error::AppError;
use crate::messaging::RedisClient;
use crate::observability::metrics::{record_cache_hit, record_cache_miss};

const PROFILE_CACHE_TTL_SECS: u64 = 5 * 60;
const GAMES_CACHE_TTL_SECS: u64 = 30;

/// Trait seam for the dashboard service, enabling handler-level testing.
#[async_trait::async_trait]
pub trait DashboardServiceTrait: Send + Sync {
    async fn get_profile(&self, user_id: Uuid) -> Result<PlayerProfileResponse, AppError>;

    async fn list_games(
        &self,
        user_id: Uuid,
        query: PaginationParams,
    ) -> Result<GameHistoryResponse, AppError>;

    async fn get_game(&self, user_id: Uuid, game_id: Uuid) -> Result<QuickGameResponse, AppError>;

    async fn get_active_game(&self, user_id: Uuid) -> Result<QuickGameResponse, AppError>;

    async fn resolve_invite_user_ids(
        &self,
        params: &SendInvitesParams,
    ) -> Result<(Vec<Uuid>, HashSet<Uuid>, Vec<String>), AppError>;

    async fn check_existing_players(&self, game_id: Uuid) -> Result<HashSet<Uuid>, AppError>;

    async fn get_invitations(&self, user_id: Uuid) -> Result<InvitationsResponse, AppError>;

    async fn search_users(&self, query: &UserSearchQuery) -> Result<UserSearchResponse, AppError>;

    async fn find_users_by_ids(&self, ids: &[Uuid]) -> Result<Vec<User>, AppError>;
}

pub struct DashboardService<R: DashboardRepoTrait> {
    repo: Arc<R>,
    game_repo: Arc<dyn GameRepoTrait>,
    card_repo: Arc<dyn GameCardRepoTrait>,
    user_cache: Arc<UserCache>,
    redis_client: Option<RedisClient>,
    default_credit: i32,
}

#[derive(Debug)]
pub struct SendInvitesParams {
    pub user_ids: Vec<Uuid>,
    pub pseudos: Vec<String>,
}

impl<R: DashboardRepoTrait> DashboardService<R> {
    pub fn new(
        repo: Arc<R>,
        game_repo: Arc<dyn GameRepoTrait>,
        card_repo: Arc<dyn GameCardRepoTrait>,
        user_cache: Arc<UserCache>,
        default_credit: i32,
    ) -> Self {
        Self {
            repo,
            game_repo,
            card_repo,
            user_cache,
            redis_client: None,
            default_credit,
        }
    }

    pub fn new_with_redis(
        repo: Arc<R>,
        game_repo: Arc<dyn GameRepoTrait>,
        card_repo: Arc<dyn GameCardRepoTrait>,
        user_cache: Arc<UserCache>,
        redis_client: RedisClient,
        default_credit: i32,
    ) -> Self {
        Self {
            repo,
            game_repo,
            card_repo,
            user_cache,
            redis_client: Some(redis_client),
            default_credit,
        }
    }

    pub async fn get_profile(&self, user_id: Uuid) -> Result<PlayerProfileResponse, AppError> {
        if let Some(cached) = self
            .get_cached::<PlayerProfileResponse>(&format!("dashboard:profile:{user_id}"))
            .await
        {
            tracing::info!("Cache hit for profile of user_id: {}", user_id);
            return Ok(cached);
        }

        let profile = self
            .repo
            .find_profile_by_user_id(user_id)
            .await
            .map_err(AppError::Database)?;

        let response = match profile {
            Some(p) => PlayerProfileResponse {
                credit: p.credit,
                game_played: p.game_played,
                wins: p.wins,
                kora_wins: p.kora_wins,
                frozen_until: p.frozen_until.map(|t| t.to_rfc3339()),
                cashout_locked: p.cashout_locked,
            },
            None => PlayerProfileResponse {
                credit: self.default_credit,
                game_played: 0,
                wins: 0,
                kora_wins: 0,
                frozen_until: None,
                cashout_locked: false,
            },
        };

        self.set_cached(
            &format!("dashboard:profile:{user_id}"),
            &response,
            PROFILE_CACHE_TTL_SECS,
        )
        .await;

        Ok(response)
    }

    pub async fn list_games(
        &self,
        user_id: Uuid,
        query: PaginationParams,
    ) -> Result<GameHistoryResponse, AppError> {
        let page = query.page.unwrap_or(1).max(1);
        let per_page = query.per_page.unwrap_or(10).clamp(1, 100);
        let filter = query.to_filter();

        let filter_hash = format!(
            "{:?}:{:?}:{:?}:{:?}:{:?}",
            query.status, query.order_by, query.bet_min, query.bet_max, filter.order_by
        );
        let cache_key = format!("dashboard:games:{user_id}:{page}:{per_page}:{filter_hash}");

        if let Some(cached) = self.get_cached::<GameHistoryResponse>(&cache_key).await {
            return Ok(cached);
        }

        let (pairs, total) = self
            .repo
            .list_players_for_user_filtered(user_id, filter, page, per_page)
            .await
            .map_err(AppError::Database)?;

        let mut game_items = Vec::new();
        for (player, game) in pairs {
            let result = if let Some(wid) = game.winner_id {
                if player.id == wid {
                    "win".to_string()
                } else {
                    "loss".to_string()
                }
            } else {
                "draw".to_string()
            };

            let status_str = match game.status {
                GameStatus::Pending => "pending",
                GameStatus::Active => "active",
                GameStatus::Finished => "finished",
                GameStatus::Cancelled => "cancelled",
                GameStatus::Kora => "kora",
                GameStatus::DoubleKora => "double_kora",
                GameStatus::Ready => "ready",
            };

            game_items.push(GameHistoryItem {
                game_id: player.game_id.to_string(),
                status: status_str.to_string(),
                bet: game.bet,
                result,
                played_at: game.created_at.to_rfc3339(),
                player_count: game.max_players as i32,
            });
        }

        let response = GameHistoryResponse {
            games: game_items,
            total,
            page,
            per_page,
        };

        self.set_cached(&cache_key, &response, GAMES_CACHE_TTL_SECS)
            .await;

        Ok(response)
    }

    async fn get_cached<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        let mut redis = self.redis_client.clone()?;
        let data = redis.get(key).await.unwrap_or_else(|e| {
            error!("Dashboard cache get error: {}", e);
            None
        });
        match data {
            Some(raw) => {
                let deserialized: Option<T> = serde_json::from_str(&raw).ok();
                if deserialized.is_some() {
                    record_cache_hit();
                    deserialized
                } else {
                    record_cache_miss();
                    None
                }
            }
            None => {
                record_cache_miss();
                None
            }
        }
    }

    async fn set_cached<T: serde::Serialize>(&self, key: &str, value: &T, ttl: u64) {
        let mut redis = match self.redis_client.clone() {
            Some(r) => r,
            None => return,
        };
        if let Ok(data) = serde_json::to_string(value) {
            let _ = redis.set_ex(key, &data, ttl).await;
        }
    }

    pub async fn get_game(
        &self,
        user_id: Uuid,
        game_id: Uuid,
    ) -> Result<QuickGameResponse, AppError> {
        let _player = self
            .repo
            .find_player_by_game_and_user(game_id, user_id)
            .await
            .map_err(AppError::Database)?
            .ok_or_else(|| AppError::NotFound("game.not_participant"))?;

        let game = self
            .repo
            .find_game_by_id(game_id)
            .await
            .map_err(AppError::Database)?
            .ok_or_else(|| AppError::NotFound("game.not_found"))?;

        match game.status {
            GameStatus::Active | GameStatus::Pending | GameStatus::Ready => {}
            _ => {
                return Err(AppError::Conflict("game.game_finished"));
            }
        }

        build_game_state_response(&*self.game_repo, &*self.card_repo, &game, user_id).await
    }

    pub async fn get_active_game(&self, user_id: Uuid) -> Result<QuickGameResponse, AppError> {
        let players = self
            .repo
            .list_players_for_user(user_id)
            .await
            .map_err(AppError::Database)?;

        if players.is_empty() {
            return Err(AppError::NotFound("game.no_active_game"));
        }

        for p in &players {
            if let Ok(Some(game)) = self
                .repo
                .find_game_by_id(p.game_id)
                .await
                .map_err(AppError::Database)
            {
                if game.status == GameStatus::Active {
                    return build_game_state_response(
                        &*self.game_repo,
                        &*self.card_repo,
                        &game,
                        user_id,
                    )
                    .await;
                }
            }
        }

        Err(AppError::NotFound("game.no_active_game"))
    }

    pub async fn resolve_invite_user_ids(
        &self,
        params: &SendInvitesParams,
    ) -> Result<(Vec<Uuid>, HashSet<Uuid>, Vec<String>), AppError> {
        let mut invited_user_ids: Vec<Uuid> = params.user_ids.clone();
        let mut resolved_from_pseudos: Vec<(Uuid, String)> = Vec::new();

        for pseudo in &params.pseudos {
            let trimmed = pseudo.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Some(uuid) = self.user_cache.get_uuid_by_pseudo(trimmed).await {
                resolved_from_pseudos.push((uuid, trimmed.to_string()));
                continue;
            }
            if let Some(user) = self
                .repo
                .find_user_by_pseudo(trimmed)
                .await
                .map_err(AppError::Database)?
            {
                self.user_cache
                    .put(user.id, user.pseudo.clone(), user.email)
                    .await;
                resolved_from_pseudos.push((user.id, user.pseudo));
            }
        }

        let mut seen_uuid = HashSet::new();
        let mut seen_pseudo = HashSet::new();
        let mut duplicates: Vec<String> = Vec::new();

        for (uuid, pseudo) in &resolved_from_pseudos {
            if !seen_uuid.insert(*uuid) {
                duplicates.push(pseudo.clone());
            }
            seen_pseudo.insert(pseudo.clone());
        }

        for uid in &invited_user_ids {
            if !seen_uuid.insert(*uid) {
                duplicates.push(uid.to_string());
            }
        }

        invited_user_ids.extend(resolved_from_pseudos.into_iter().map(|(u, _)| u));

        Ok((invited_user_ids, seen_uuid, duplicates))
    }

    pub async fn check_existing_players(&self, game_id: Uuid) -> Result<HashSet<Uuid>, AppError> {
        let existing_players = self
            .repo
            .list_players_by_game_ordered(game_id)
            .await
            .map_err(AppError::Database)?;
        Ok(existing_players.iter().filter_map(|p| p.user_id).collect())
    }

    pub async fn get_invitations(&self, user_id: Uuid) -> Result<InvitationsResponse, AppError> {
        let pending = self
            .repo
            .list_pending_invites_for_user(user_id)
            .await
            .map_err(AppError::Database)?;

        let mut items = Vec::new();
        for (invite, game) in pending {
            let player_count = self
                .repo
                .list_players_by_game_ordered(game.id)
                .await
                .map_err(AppError::Database)?
                .len() as i64;

            let creator_pseudo = match game.creator_id {
                Some(uid) => self
                    .repo
                    .find_user_by_id(uid)
                    .await
                    .map_err(AppError::Database)?
                    .map(|u| u.pseudo)
                    .unwrap_or_else(|| "Unknown".to_string()),
                None => "Unknown".to_string(),
            };

            items.push(InvitationItem {
                invite_id: invite.id,
                game_id: game.id,
                creator_pseudo,
                bet: game.bet,
                player_count,
                max_players: game.max_players as i32,
                created_at: invite.created_at.to_rfc3339(),
                expires_at: game.invite_expires_at.map(|t| t.to_rfc3339()),
            });
        }

        Ok(InvitationsResponse { invitations: items })
    }

    pub async fn search_users(
        &self,
        query: &UserSearchQuery,
    ) -> Result<UserSearchResponse, AppError> {
        if query.q.trim().len() < 2 {
            return Ok(UserSearchResponse { users: vec![] });
        }

        // TODO: define a max limit as environment variable for the query to avoid abuse
        let users = self
            .repo
            .find_users_by_pseudo_prefix(query.q.trim(), query.limit)
            .await
            .map_err(AppError::Database)?;

        let bulk: Vec<(Uuid, String, String)> = users
            .iter()
            .map(|u| (u.id, u.pseudo.clone(), u.email.clone()))
            .collect();
        self.user_cache.populate_bulk(&bulk).await;

        let items: Vec<UserSearchItem> = users
            .into_iter()
            .map(|u| UserSearchItem {
                id: u.id,
                pseudo: u.pseudo,
            })
            .collect();

        Ok(UserSearchResponse { users: items })
    }

    pub async fn find_users_by_ids(&self, ids: &[Uuid]) -> Result<Vec<User>, AppError> {
        let mut users = Vec::with_capacity(ids.len());
        for &id in ids {
            if let Some(user) = self
                .repo
                .find_user_by_id(id)
                .await
                .map_err(AppError::Database)?
            {
                users.push(user);
            }
        }
        Ok(users)
    }
}

#[async_trait::async_trait]
impl<R: DashboardRepoTrait> DashboardServiceTrait for DashboardService<R> {
    async fn get_profile(&self, user_id: Uuid) -> Result<PlayerProfileResponse, AppError> {
        DashboardService::get_profile(self, user_id).await
    }

    async fn list_games(
        &self,
        user_id: Uuid,
        query: PaginationParams,
    ) -> Result<GameHistoryResponse, AppError> {
        DashboardService::list_games(self, user_id, query).await
    }

    async fn get_game(&self, user_id: Uuid, game_id: Uuid) -> Result<QuickGameResponse, AppError> {
        DashboardService::get_game(self, user_id, game_id).await
    }

    async fn get_active_game(&self, user_id: Uuid) -> Result<QuickGameResponse, AppError> {
        DashboardService::get_active_game(self, user_id).await
    }

    async fn resolve_invite_user_ids(
        &self,
        params: &SendInvitesParams,
    ) -> Result<(Vec<Uuid>, HashSet<Uuid>, Vec<String>), AppError> {
        DashboardService::resolve_invite_user_ids(self, params).await
    }

    async fn check_existing_players(&self, game_id: Uuid) -> Result<HashSet<Uuid>, AppError> {
        DashboardService::check_existing_players(self, game_id).await
    }

    async fn get_invitations(&self, user_id: Uuid) -> Result<InvitationsResponse, AppError> {
        DashboardService::get_invitations(self, user_id).await
    }

    async fn search_users(&self, query: &UserSearchQuery) -> Result<UserSearchResponse, AppError> {
        DashboardService::search_users(self, query).await
    }

    async fn find_users_by_ids(&self, ids: &[Uuid]) -> Result<Vec<User>, AppError> {
        DashboardService::find_users_by_ids(self, ids).await
    }
}
