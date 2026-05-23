use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde::de::DeserializeOwned;
use tracing::error;
use uuid::Uuid;

use crate::api::dto::dashboard::{
    GameHistoryItem, GameHistoryResponse, PaginationParams, PlayerProfileResponse,
};
use crate::api::dto::requests::UserSearchQuery;
use crate::api::dto::responses::{
    InvitationItem, InvitationsResponse, PlayerInfoDto, QuickGameResponse, UserSearchItem,
    UserSearchResponse,
};
use crate::cache::UserCache;
use crate::database::models::{GameStatus, PlayerType, User};
use crate::database::traits::DashboardRepoTrait;
use crate::error::AppError;
use crate::game::service::compute_display_position;
use crate::messaging::RedisClient;

const PROFILE_CACHE_TTL_SECS: u64 = 5 * 60;
const GAMES_CACHE_TTL_SECS: u64 = 30;

pub struct DashboardService<R: DashboardRepoTrait> {
    repo: Arc<R>,
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
    pub fn new(repo: Arc<R>, user_cache: Arc<UserCache>, default_credit: i32) -> Self {
        Self {
            repo,
            user_cache,
            redis_client: None,
            default_credit,
        }
    }

    pub fn new_with_redis(
        repo: Arc<R>,
        user_cache: Arc<UserCache>,
        redis_client: RedisClient,
        default_credit: i32,
    ) -> Self {
        Self {
            repo,
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
            },
            None => PlayerProfileResponse {
                credit: self.default_credit,
                game_played: 0,
                wins: 0,
                kora_wins: 0,
                frozen_until: None,
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
        })?;
        serde_json::from_str(&data).ok()
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
            .ok_or_else(|| {
                AppError::NotFound("Game not found or you are not a participant".into())
            })?;

        let game = self
            .repo
            .find_game_by_id(game_id)
            .await
            .map_err(AppError::Database)?
            .ok_or_else(|| AppError::NotFound("Game not found".into()))?;

        match game.status {
            GameStatus::Active | GameStatus::Pending | GameStatus::Ready => {}
            _ => {
                return Err(AppError::Conflict(format!(
                    "Game already finished: {:?}",
                    game.status
                )));
            }
        }

        build_game_state_response(&*self.repo, &game, user_id).await
    }

    pub async fn get_active_game(&self, user_id: Uuid) -> Result<QuickGameResponse, AppError> {
        let players = self
            .repo
            .list_players_for_user(user_id)
            .await
            .map_err(AppError::Database)?;

        if players.is_empty() {
            return Err(AppError::NotFound("No active game found".into()));
        }

        for p in &players {
            if let Ok(Some(game)) = self
                .repo
                .find_game_by_id(p.game_id)
                .await
                .map_err(AppError::Database)
            {
                if game.status == GameStatus::Active {
                    return build_game_state_response(&*self.repo, &game, user_id).await;
                }
            }
        }

        Err(AppError::NotFound("No active game found".into()))
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

async fn build_game_state_response(
    repo: &dyn DashboardRepoTrait,
    game: &crate::database::models::Game,
    user_id: Uuid,
) -> Result<QuickGameResponse, AppError> {
    let all_players = repo
        .list_players_by_game_ordered(game.id)
        .await
        .map_err(AppError::Database)?;

    let num_players = all_players.len();

    let my_player = all_players.iter().find(|p| p.user_id == Some(user_id));
    let my_position = my_player.map(|p| p.position as usize).unwrap_or(0);

    let my_cards: Vec<i32> = if let Some(mp) = my_player {
        match repo.find_cards_for_player(mp.id, true).await {
            Ok(cards) => cards.iter().map(|c| c.card_index).collect(),
            Err(_) => Vec::new(),
        }
    } else {
        Vec::new()
    };

    let all_game_cards = repo
        .find_all_cards_for_game(game.id)
        .await
        .unwrap_or_default();

    let mut remaining_counts: HashMap<Uuid, usize> = HashMap::new();
    for player in &all_players {
        let unplayed = all_game_cards
            .iter()
            .filter(|c| c.player_id == Some(player.id) && !c.played)
            .count();
        remaining_counts.insert(player.id, unplayed);
    }

    let current_round = game.roll;
    let mut deck_slots: Vec<Option<i32>> = vec![None; num_players];
    for card in &all_game_cards {
        if card.played && card.round == Some(current_round) {
            if let Some(pid) = card.player_id {
                if let Some(pos) = all_players.iter().position(|p| p.id == pid) {
                    let display_pos = compute_display_position(pos, num_players, my_position);
                    deck_slots[display_pos] = Some(card.card_index);
                }
            }
        }
    }

    let players_json: Vec<PlayerInfoDto> = all_players
        .iter()
        .enumerate()
        .map(|(idx, player)| {
            let player_type = match player.player_type {
                PlayerType::Human => "human",
                PlayerType::Bot => "bot",
            };
            let is_me = player.user_id == Some(user_id);
            let cards = if is_me { my_cards.clone() } else { Vec::new() };
            let cards_count = *remaining_counts.get(&player.id).unwrap_or(&0) as i32;
            let display_pos = compute_display_position(idx, num_players, my_position);
            PlayerInfoDto {
                id: player.id,
                player_type: player_type.to_string(),
                name: player.name.clone(),
                position: player.position,
                display_position: display_pos as i32,
                cards,
                cards_count,
                is_current_user: is_me,
            }
        })
        .collect();

    let current_turn = game.rank.unwrap_or(0);
    let current_turn_display =
        compute_display_position(current_turn as usize, num_players, my_position) as i32;

    Ok(QuickGameResponse {
        game_id: game.id,
        players: players_json,
        status: match game.status {
            GameStatus::Active => "active".to_string(),
            GameStatus::Ready => "ready".to_string(),
            _ => "pending".to_string(),
        },
        current_turn: current_turn_display,
        bet: game.bet,
        max_players: game.max_players as i32,
        invite_expires_at: game.invite_expires_at.map(|t| t.to_rfc3339()),
        deck_slots: Some(deck_slots),
    })
}

#[cfg(test)]
#[path = "dashboard_service_tests.rs"]
mod tests;
