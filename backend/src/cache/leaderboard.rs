use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::error;
use uuid::Uuid;

use crate::cache::UserCache;
use crate::database::models::PlayerProfile;
use crate::messaging::RedisClient;
use crate::observability::metrics::{record_cache_hit, record_cache_miss};

const LEADERBOARD_WINS_KEY: &str = "leaderboard:wins";
const LEADERBOARD_STREAK_KEY: &str = "leaderboard:streak";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaderboardEntry {
    pub rank: u64,
    pub user_id: Uuid,
    pub pseudo: String,
    pub wins: i32,
    pub winning_streak: i32,
    pub is_current_user: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaderboardResponse {
    pub top_by_wins: Vec<LeaderboardEntry>,
    pub top_by_streak: Vec<LeaderboardEntry>,
    pub current_user_wins_rank: Option<u64>,
    pub current_user_streak_rank: Option<u64>,
}

#[allow(dead_code)]
pub async fn refresh_leaderboard(mut redis: RedisClient, profiles: &[PlayerProfile]) {
    if let Err(e) = redis.del(LEADERBOARD_WINS_KEY).await {
        error!("Failed to clear leaderboard wins: {}", e);
        return;
    }
    if let Err(e) = redis.del(LEADERBOARD_STREAK_KEY).await {
        error!("Failed to clear leaderboard streak: {}", e);
        return;
    }

    for profile in profiles {
        let member = profile.user_id.to_string();
        if let Err(e) = redis
            .zadd(LEADERBOARD_WINS_KEY, member.clone(), profile.wins as f64)
            .await
        {
            error!("Failed to ZADD wins leaderboard {}: {}", member, e);
        }
        if let Err(e) = redis
            .zadd(
                LEADERBOARD_STREAK_KEY,
                member,
                profile.winning_streak as f64,
            )
            .await
        {
            error!(
                "Failed to ZADD streak leaderboard {}: {}",
                profile.user_id, e
            );
        }
    }
}

pub async fn get_leaderboard(
    mut redis: RedisClient,
    current_user_id: Uuid,
    user_cache: &UserCache,
) -> Option<LeaderboardResponse> {
    let wins_raw: Vec<(String, f64)> =
        match redis.zrevrange_withscores(LEADERBOARD_WINS_KEY, 0, 9).await {
            Ok(data) => {
                if data.is_empty() {
                    record_cache_miss();
                } else {
                    record_cache_hit();
                }
                data
            }
            Err(e) => {
                error!("Failed to ZREVRANGE wins: {}", e);
                record_cache_miss();
                return None;
            }
        };

    let streak_raw: Vec<(String, f64)> = match redis
        .zrevrange_withscores(LEADERBOARD_STREAK_KEY, 0, 9)
        .await
    {
        Ok(data) => {
            if data.is_empty() {
                record_cache_miss();
            } else {
                record_cache_hit();
            }
            data
        }
        Err(e) => {
            error!("Failed to ZREVRANGE streak: {}", e);
            record_cache_miss();
            return None;
        }
    };

    let current_user_wins_rank: Option<u64> = match redis
        .zrevrank(LEADERBOARD_WINS_KEY, current_user_id.to_string())
        .await
    {
        Ok(Some(rank)) => {
            record_cache_hit();
            Some(rank + 1)
        }
        Ok(None) => {
            record_cache_miss();
            None
        }
        Err(e) => {
            error!("Failed to ZREVRANK wins: {}", e);
            record_cache_miss();
            None
        }
    };

    let current_user_streak_rank: Option<u64> = match redis
        .zrevrank(LEADERBOARD_STREAK_KEY, current_user_id.to_string())
        .await
    {
        Ok(Some(rank)) => {
            record_cache_hit();
            Some(rank + 1)
        }
        Ok(None) => {
            record_cache_miss();
            None
        }
        Err(e) => {
            error!("Failed to ZREVRANK streak: {}", e);
            record_cache_miss();
            None
        }
    };

    let user_ids: Vec<Uuid> = wins_raw
        .iter()
        .chain(streak_raw.iter())
        .filter_map(|(id, _)| Uuid::parse_str(id).ok())
        .collect();

    let mut all_ids = user_ids.clone();
    if !all_ids.contains(&current_user_id) {
        all_ids.push(current_user_id);
    }
    let cached_users = user_cache.get_by_uuids(&all_ids).await;
    let mut pseudo_map = HashMap::new();
    for (id, cached) in all_ids.iter().zip(cached_users.iter()) {
        if let Some(cu) = cached {
            pseudo_map.insert(*id, cu.pseudo.clone());
        }
    }

    let top_by_wins: Vec<LeaderboardEntry> = wins_raw
        .into_iter()
        .enumerate()
        .filter_map(|(i, (uid_str, score))| {
            let user_id = Uuid::parse_str(&uid_str).ok()?;
            let pseudo = pseudo_map
                .get(&user_id)
                .cloned()
                .unwrap_or_else(|| "Unknown".to_string());
            Some(LeaderboardEntry {
                rank: i as u64 + 1,
                user_id,
                pseudo,
                wins: score as i32,
                winning_streak: 0,
                is_current_user: user_id == current_user_id,
            })
        })
        .collect();

    let top_by_streak: Vec<LeaderboardEntry> = streak_raw
        .into_iter()
        .enumerate()
        .filter_map(|(i, (uid_str, score))| {
            let user_id = Uuid::parse_str(&uid_str).ok()?;
            let pseudo = pseudo_map
                .get(&user_id)
                .cloned()
                .unwrap_or_else(|| "Unknown".to_string());
            Some(LeaderboardEntry {
                rank: i as u64 + 1,
                user_id,
                pseudo,
                wins: 0,
                winning_streak: score as i32,
                is_current_user: user_id == current_user_id,
            })
        })
        .collect();

    Some(LeaderboardResponse {
        top_by_wins,
        top_by_streak,
        current_user_wins_rank,
        current_user_streak_rank,
    })
}
