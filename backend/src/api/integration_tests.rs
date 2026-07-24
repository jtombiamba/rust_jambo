#[cfg(test)]
mod tests {
    use actix_web::{test, web, App};
    use std::sync::Arc;
    use uuid::Uuid;

    use crate::api::game::play_card;
    use crate::error::GameError;
    use crate::game::service::mock::MockGameService;
    use crate::game::service::types::{GameServiceTrait, PlayCardOutcome, QuickGameOutcome};

    fn play_outcome(card_id: Uuid) -> PlayCardOutcome {
        PlayCardOutcome {
            card_id,
            next_turn: Some(Uuid::new_v4()),
            game_ended: false,
            round_completed: false,
            current_round: 1,
        }
    }

    fn quick_outcome(game_id: Uuid, step_by_step: bool) -> QuickGameOutcome {
        QuickGameOutcome {
            game_id,
            players: vec![],
            status: "active".into(),
            current_turn: 0,
            bet: 10,
            max_players: 4,
            invite_expires_at: None,
            deck_slots: None,
            ws_token: None,
            step_by_step,
        }
    }

    #[actix_web::test]
    async fn play_card_happy_path() {
        let card_id = Uuid::new_v4();
        let mock: Arc<dyn GameServiceTrait> = Arc::new(MockGameService::new(
            Ok(play_outcome(card_id)),
            Ok(quick_outcome(Uuid::new_v4(), false)),
        ));
        let app =
            test::init_service(App::new().app_data(web::Data::new(mock)).service(play_card)).await;

        let game_id = Uuid::new_v4();
        let player_id = Uuid::new_v4();
        let req = test::TestRequest::post()
            .uri(&format!("/game/{}/play", game_id))
            .set_json(serde_json::json!({"player_id": player_id, "card_index": 5}))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);

        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["success"], true);
        assert_eq!(body["card_id"], card_id.to_string());
        assert_eq!(body["game_ended"], false);
    }

    #[actix_web::test]
    async fn play_card_game_finished_returns_409() {
        let mock: Arc<dyn GameServiceTrait> = Arc::new(MockGameService::new(
            Err(GameError::GameFinished),
            Ok(quick_outcome(Uuid::new_v4(), false)),
        ));
        let app =
            test::init_service(App::new().app_data(web::Data::new(mock)).service(play_card)).await;

        let game_id = Uuid::new_v4();
        let player_id = Uuid::new_v4();
        let req = test::TestRequest::post()
            .uri(&format!("/game/{}/play", game_id))
            .set_json(serde_json::json!({"player_id": player_id, "card_index": 0}))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 409);
    }

    #[actix_web::test]
    async fn play_card_insufficient_credits_returns_402() {
        let mock: Arc<dyn GameServiceTrait> = Arc::new(MockGameService::new(
            Err(GameError::InsufficientCredits {
                required: 10,
                current: 5,
            }),
            Ok(quick_outcome(Uuid::new_v4(), false)),
        ));
        let app =
            test::init_service(App::new().app_data(web::Data::new(mock)).service(play_card)).await;

        let game_id = Uuid::new_v4();
        let player_id = Uuid::new_v4();
        let req = test::TestRequest::post()
            .uri(&format!("/game/{}/play", game_id))
            .set_json(serde_json::json!({"player_id": player_id, "card_index": 0}))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 402);
    }
}
