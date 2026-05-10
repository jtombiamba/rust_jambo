use std::collections::HashMap;
use std::sync::Arc;

use actix_web::HttpResponse;
use serde_json::json;
use uuid::Uuid;

use crate::api::dto::dashboard::{
    GameHistoryItem, GameHistoryResponse, PaginationParams, PlayerProfileResponse,
};
use crate::api::dto::responses::{PlayerInfoDto, QuickGameResponse};
use crate::database::models::{GameStatus, PlayerType};
use crate::database::traits::DashboardRepoTrait;
use crate::game::service::compute_display_position;

pub struct DashboardService<R: DashboardRepoTrait> {
    repo: Arc<R>,
}

impl<R: DashboardRepoTrait> DashboardService<R> {
    pub fn new(repo: Arc<R>) -> Self {
        Self { repo }
    }

    pub async fn get_profile(&self, user_id: Uuid) -> Result<HttpResponse, actix_web::Error> {
        let profile = self.repo.find_profile_by_user_id(user_id).await;

        match profile {
            Ok(Some(p)) => Ok(HttpResponse::Ok().json(PlayerProfileResponse {
                credit: p.credit,
                game_played: p.game_played,
                wins: p.wins,
                kora_wins: p.kora_wins,
            })),
            Ok(None) => Ok(HttpResponse::Ok().json(PlayerProfileResponse {
                credit: 500,
                game_played: 0,
                wins: 0,
                kora_wins: 0,
            })),
            Err(e) => {
                tracing::error!("Failed to fetch profile: {}", e);
                Ok(HttpResponse::InternalServerError()
                    .json(json!({"error": "Internal server error"})))
            }
        }
    }

    pub async fn list_games(
        &self,
        user_id: Uuid,
        query: PaginationParams,
    ) -> Result<HttpResponse, actix_web::Error> {
        let page = query.page.unwrap_or(1).max(1);
        let per_page = query.per_page.unwrap_or(10).clamp(1, 100);
        let filter = query.to_filter();

        let (pairs, total) = match self
            .repo
            .list_players_for_user_filtered(user_id, filter, page, per_page)
            .await
        {
            Ok(result) => result,
            Err(e) => {
                tracing::error!("Failed to fetch filtered player records: {}", e);
                return Ok(HttpResponse::InternalServerError()
                    .json(json!({"error": "Internal server error"})));
            }
        };

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
                credits_after: player.credits,
                player_count: game.max_players as i32,
            });
        }

        Ok(HttpResponse::Ok().json(GameHistoryResponse {
            games: game_items,
            total,
            page,
            per_page,
        }))
    }

    pub async fn get_game(
        &self,
        user_id: Uuid,
        game_id: Uuid,
    ) -> Result<HttpResponse, actix_web::Error> {
        let _player = match self
            .repo
            .find_player_by_game_and_user(game_id, user_id)
            .await
        {
            Ok(Some(p)) => p,
            Ok(None) => {
                return Ok(HttpResponse::NotFound()
                    .json(json!({"error": "Game not found or you are not a participant"})));
            }
            Err(e) => {
                tracing::error!("Failed to fetch player: {}", e);
                return Ok(HttpResponse::InternalServerError()
                    .json(json!({"error": "Internal server error"})));
            }
        };

        let game = match self.repo.find_game_by_id(game_id).await {
            Ok(Some(g)) => g,
            Ok(None) => {
                return Ok(HttpResponse::NotFound().json(json!({"error": "Game not found"})));
            }
            Err(e) => {
                tracing::error!("Failed to fetch game: {}", e);
                return Ok(HttpResponse::InternalServerError()
                    .json(json!({"error": "Internal server error"})));
            }
        };

        match game.status {
            GameStatus::Active | GameStatus::Pending | GameStatus::Ready => {}
            _ => {
                return Ok(HttpResponse::Gone().json(json!({
                    "error": "Game already finished",
                    "status": format!("{:?}", game.status),
                })));
            }
        }

        build_game_state_response(&*self.repo, &game, user_id).await
    }

    pub async fn get_active_game(&self, user_id: Uuid) -> Result<HttpResponse, actix_web::Error> {
        let players = match self.repo.list_players_for_user(user_id).await {
            Ok(rows) if !rows.is_empty() => rows,
            Ok(_) => {
                return Ok(HttpResponse::NotFound().json(json!({"error": "No active game found"})));
            }
            Err(e) => {
                tracing::error!("Failed to fetch players: {}", e);
                return Ok(HttpResponse::InternalServerError()
                    .json(json!({"error": "Internal server error"})));
            }
        };

        for p in &players {
            if let Ok(Some(game)) = self.repo.find_game_by_id(p.game_id).await {
                if game.status == GameStatus::Active {
                    return build_game_state_response(&*self.repo, &game, user_id).await;
                }
            }
        }

        Ok(HttpResponse::NotFound().json(json!({"error": "No active game found"})))
    }
}

async fn build_game_state_response(
    repo: &dyn DashboardRepoTrait,
    game: &crate::database::models::Game,
    user_id: Uuid,
) -> Result<HttpResponse, actix_web::Error> {
    let all_players = match repo.list_players_by_game_ordered(game.id).await {
        Ok(rows) => rows,
        Err(e) => {
            tracing::error!("Failed to fetch game players: {}", e);
            return Ok(
                HttpResponse::InternalServerError().json(json!({"error": "Internal server error"}))
            );
        }
    };

    let num_players = all_players.len();

    let my_player = all_players.iter().find(|p| p.user_id == Some(user_id));

    let my_position = my_player.map(|p| p.position as usize).unwrap_or(0);

    let my_cards: Vec<i32> = if let Some(mp) = my_player {
        match repo.find_cards_for_player(mp.id, true).await {
            Ok(cards) => cards.iter().map(|c| c.card_index).collect(),
            Err(e) => {
                tracing::error!("Failed to fetch my cards: {}", e);
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };

    let all_game_cards = match repo.find_all_cards_for_game(game.id).await {
        Ok(cards) => cards,
        Err(e) => {
            tracing::error!("Failed to fetch game cards: {}", e);
            Vec::new()
        }
    };

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

    Ok(HttpResponse::Ok().json(QuickGameResponse {
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
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::dto::dashboard::GameFilter;
    use crate::database::models::{Game, GameCard, Player, PlayerProfile};
    use actix_web::http::StatusCode;
    use async_trait::async_trait;
    use sea_orm::DbErr;
    use std::sync::Mutex;

    struct MockDashboardRepo {
        profile: Mutex<Option<PlayerProfile>>,
        players: Mutex<Vec<Player>>,
        games: Mutex<HashMap<Uuid, Game>>,
        cards: Mutex<Vec<GameCard>>,
    }

    impl MockDashboardRepo {
        fn new() -> Self {
            Self {
                profile: Mutex::new(None),
                players: Mutex::new(Vec::new()),
                games: Mutex::new(HashMap::new()),
                cards: Mutex::new(Vec::new()),
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

        async fn find_cards_for_player(
            &self,
            player_id: Uuid,
            _unplayed_only: bool,
        ) -> Result<Vec<GameCard>, DbErr> {
            Ok(self
                .cards
                .lock()
                .unwrap()
                .iter()
                .filter(|c| c.player_id == Some(player_id))
                .cloned()
                .collect())
        }

        async fn find_all_cards_for_game(&self, game_id: Uuid) -> Result<Vec<GameCard>, DbErr> {
            Ok(self
                .cards
                .lock()
                .unwrap()
                .iter()
                .filter(|c| c.game_id == game_id)
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
    }

    #[tokio::test]
    async fn test_get_profile_not_found_returns_defaults() {
        let repo = Arc::new(MockDashboardRepo::new());
        let service = DashboardService::new(repo);
        let resp = service.get_profile(Uuid::new_v4()).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_list_games_empty() {
        let repo = Arc::new(MockDashboardRepo::new());
        let service = DashboardService::new(repo);
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
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
