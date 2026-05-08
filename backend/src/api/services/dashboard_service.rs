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

        let all_players = match self.repo.list_players_for_user(user_id).await {
            Ok(rows) => rows,
            Err(e) => {
                tracing::error!("Failed to fetch player records: {}", e);
                return Ok(HttpResponse::InternalServerError()
                    .json(json!({"error": "Internal server error"})));
            }
        };

        let total = all_players.len() as u64;
        let start = ((page - 1) as usize * per_page as usize).min(all_players.len());
        let end = (start + per_page as usize).min(all_players.len());
        let page_players = &all_players[start..end];

        let mut game_items = Vec::new();
        for player in page_players {
            let game = self.repo.find_game_by_id(player.game_id).await;

            let (status, bet, played_at, winner_id) = match game {
                Ok(Some(g)) => (g.status, g.bet, g.created_at, g.winner_id),
                _ => continue,
            };

            let result = if let Some(wid) = winner_id {
                if player.id == wid {
                    "win".to_string()
                } else {
                    "loss".to_string()
                }
            } else {
                "draw".to_string()
            };

            let status_str = match status {
                GameStatus::Pending => "pending",
                GameStatus::Active => "active",
                GameStatus::Finished => "finished",
                GameStatus::Cancelled => "cancelled",
                GameStatus::Kora => "kora",
                GameStatus::DoubleKora => "double_kora",
            };

            game_items.push(GameHistoryItem {
                game_id: player.game_id.to_string(),
                status: status_str.to_string(),
                bet,
                result,
                played_at: played_at.to_rfc3339(),
                credits_after: player.credits,
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
            GameStatus::Active | GameStatus::Pending => {}
            _ => {
                return Ok(HttpResponse::Gone().json(json!({
                    "error": "Game already finished",
                    "status": format!("{:?}", game.status),
                })));
            }
        }

        build_game_state_response(&*self.repo, &game).await
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
                    return build_game_state_response(&*self.repo, &game).await;
                }
            }
        }

        Ok(HttpResponse::NotFound().json(json!({"error": "No active game found"})))
    }
}

async fn build_game_state_response(
    repo: &dyn DashboardRepoTrait,
    game: &crate::database::models::Game,
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

    let human_player = all_players
        .iter()
        .find(|ap| matches!(ap.player_type, PlayerType::Human));

    let human_cards: Vec<i32> = if let Some(hp) = human_player {
        match repo.find_cards_for_player(hp.id, true).await {
            Ok(cards) => cards.iter().map(|c| c.card_index).collect(),
            Err(e) => {
                tracing::error!("Failed to fetch human cards: {}", e);
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
    let num_players = all_players.len();
    let mut deck_slots: Vec<Option<i32>> = vec![None; num_players];
    for card in &all_game_cards {
        if card.played && card.round == Some(current_round) {
            if let Some(pid) = card.player_id {
                if let Some(pos) = all_players.iter().position(|p| p.id == pid) {
                    deck_slots[pos] = Some(card.card_index);
                }
            }
        }
    }

    let players_json: Vec<PlayerInfoDto> = all_players
        .iter()
        .map(|player| {
            let player_type = match player.player_type {
                PlayerType::Human => "human",
                PlayerType::Bot => "bot",
            };
            let cards = if matches!(player.player_type, PlayerType::Human) {
                human_cards.clone()
            } else {
                Vec::new()
            };
            let cards_count = *remaining_counts.get(&player.id).unwrap_or(&0) as i32;
            PlayerInfoDto {
                id: player.id,
                player_type: player_type.to_string(),
                name: player.name.clone(),
                position: player.position,
                cards,
                cards_count,
            }
        })
        .collect();

    let current_turn = game.rank.unwrap_or(0);

    Ok(HttpResponse::Ok().json(QuickGameResponse {
        game_id: game.id,
        players: players_json,
        status: match game.status {
            GameStatus::Active => "active".to_string(),
            _ => "pending".to_string(),
        },
        current_turn,
        bet: game.bet,
        deck_slots: Some(deck_slots),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
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
                },
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
