#[cfg(test)]
mod tests {
    use sea_orm::{DatabaseBackend, MockDatabase, MockExecResult};
    use std::sync::Arc;
    use uuid::Uuid;

    use crate::api::services::room_service::RoomService;
    use crate::config::Config;
    use crate::mailer::noop::NoopMailer;
    use crate::mailer::MailerConfig;

    fn make_mailer() -> Arc<dyn crate::mailer::Mailer> {
        let config = MailerConfig {
            mailer_mode: "console".to_string(),
            smtp_host: "".to_string(),
            smtp_port: 0,
            smtp_username: "".to_string(),
            smtp_password: "".to_string(),
            smtp_tls: false,
            smtp_from_email: "test@test.com".to_string(),
            smtp_from_name: "Test".to_string(),
            frontend_url: "http://localhost:3000".to_string(),
            contact_to_email: "support@test.com".to_string(),
        };
        let mailer = NoopMailer::new(config).unwrap();
        Arc::new(mailer)
    }

    fn make_config() -> Config {
        Config::default()
    }

    fn make_room_model(
        id: Uuid,
        creator_id: Uuid,
        name: &str,
        code: &str,
    ) -> crate::database::models::room::Model {
        use chrono::DateTime;
        crate::database::models::room::Model {
            id,
            creator_id,
            name: name.to_string(),
            invitation_code: code.to_string(),
            created_at: DateTime::from_timestamp(0, 0).unwrap(),
            updated_at: DateTime::from_timestamp(0, 0).unwrap(),
        }
    }

    fn make_room_member(
        id: Uuid,
        room_id: Uuid,
        user_id: Uuid,
    ) -> crate::database::models::room_member::Model {
        use chrono::DateTime;
        crate::database::models::room_member::Model {
            id,
            room_id,
            user_id,
            joined_at: DateTime::from_timestamp(0, 0).unwrap(),
        }
    }

    fn make_user_model(id: Uuid, pseudo: &str) -> crate::database::models::user::Model {
        use chrono::DateTime;
        crate::database::models::user::Model {
            id,
            pseudo: pseudo.to_string(),
            email: format!("{}@test.com", pseudo),
            password_hash: "hash".to_string(),
            last_ip_hash: None,
            language: "en".to_string(),
            created_at: DateTime::from_timestamp(0, 0).unwrap(),
            updated_at: DateTime::from_timestamp(0, 0).unwrap(),
        }
    }

    #[tokio::test]
    async fn create_room_rejects_empty_name() {
        let db = MockDatabase::new(DatabaseBackend::Postgres).into_connection();
        let mailer = make_mailer();
        let config = make_config();
        let service = RoomService::new(db, mailer, config, None);

        let result = service.create_room(Uuid::new_v4(), "").await;
        assert!(result.is_err());

        let result = service.create_room(Uuid::new_v4(), "   ").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn list_user_rooms_empty_when_no_rooms() {
        let db = MockDatabase::new(DatabaseBackend::Postgres)
            .append_query_results(vec![Vec::<crate::database::models::room::Model>::new()])
            .into_connection();
        let mailer = make_mailer();
        let config = make_config();
        let service = RoomService::new(db, mailer, config, None);

        let result = service.list_user_rooms(Uuid::new_v4()).await;
        assert!(result.is_ok());
        let rooms = result.unwrap();
        assert!(rooms.is_empty());
    }

    #[tokio::test]
    async fn join_room_invalid_code_returns_error() {
        let db = MockDatabase::new(DatabaseBackend::Postgres)
            .append_query_results(vec![Vec::<crate::database::models::room::Model>::new()])
            .into_connection();
        let mailer = make_mailer();
        let config = make_config();
        let service = RoomService::new(db, mailer, config, None);

        let result = service.join_room(Uuid::new_v4(), "INVALID").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn join_room_already_member_returns_error() {
        let user_id = Uuid::new_v4();
        let room_id = Uuid::new_v4();
        let room = make_room_model(room_id, user_id, "Room", "CODE1234");
        let member = make_room_member(Uuid::new_v4(), room_id, user_id);

        let db = MockDatabase::new(DatabaseBackend::Postgres)
            .append_query_results(vec![vec![room]])
            .append_query_results(vec![vec![member]])
            .into_connection();
        let mailer = make_mailer();
        let config = make_config();
        let service = RoomService::new(db, mailer, config, None);

        let result = service.join_room(user_id, "CODE1234").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn leave_room_not_member_returns_error() {
        let room_id = Uuid::new_v4();
        let room = make_room_model(room_id, Uuid::new_v4(), "Room", "CODE");

        let db = MockDatabase::new(DatabaseBackend::Postgres)
            .append_query_results(vec![vec![room]])
            .append_query_results(vec![
                Vec::<crate::database::models::room_member::Model>::new(),
            ])
            .into_connection();
        let mailer = make_mailer();
        let config = make_config();
        let service = RoomService::new(db, mailer, config, None);

        let result = service.leave_room(room_id, Uuid::new_v4()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn leave_room_deletes_if_last_member() {
        let user_id = Uuid::new_v4();
        let room_id = Uuid::new_v4();
        let member_id = Uuid::new_v4();
        let room = make_room_model(room_id, user_id, "Room", "CODE");
        let member = make_room_member(member_id, room_id, user_id);

        let db = MockDatabase::new(DatabaseBackend::Postgres)
            .append_query_results(vec![vec![room]])
            .append_query_results(vec![vec![member]])
            .append_query_results(vec![Vec::<crate::database::models::game_run::Model>::new()])
            .append_exec_results(vec![
                MockExecResult {
                    last_insert_id: 1,
                    rows_affected: 1,
                },
                MockExecResult {
                    last_insert_id: 2,
                    rows_affected: 1,
                },
            ])
            .append_query_results(vec![
                Vec::<crate::database::models::room_member::Model>::new(),
            ])
            .append_exec_results(vec![MockExecResult {
                last_insert_id: 3,
                rows_affected: 1,
            }])
            .append_query_results(vec![Vec::<crate::database::models::user::Model>::new()])
            .into_connection();
        let mailer = make_mailer();
        let config = make_config();
        let service = RoomService::new(db, mailer, config, None);

        let result = service.leave_room(room_id, user_id).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn invite_to_room_not_member_returns_error() {
        let room_id = Uuid::new_v4();
        let room = make_room_model(room_id, Uuid::new_v4(), "Room", "CODE");

        let db = MockDatabase::new(DatabaseBackend::Postgres)
            .append_query_results(vec![vec![room]])
            .append_query_results(vec![
                Vec::<crate::database::models::room_member::Model>::new(),
            ])
            .into_connection();
        let mailer = make_mailer();
        let config = make_config();
        let service = RoomService::new(db, mailer, config, None);

        let result = service
            .invite_to_room(room_id, Uuid::new_v4(), "friend@test.com")
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn get_room_detail_room_not_found() {
        let db = MockDatabase::new(DatabaseBackend::Postgres)
            .append_query_results(vec![Vec::<crate::database::models::room::Model>::new()])
            .into_connection();
        let mailer = make_mailer();
        let config = make_config();
        let service = RoomService::new(db, mailer, config, None);

        let result = service
            .get_room_detail(Uuid::new_v4(), Uuid::new_v4())
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn get_room_detail_not_member_returns_error() {
        let room_id = Uuid::new_v4();
        let room = make_room_model(room_id, Uuid::new_v4(), "Room", "CODE");

        let db = MockDatabase::new(DatabaseBackend::Postgres)
            .append_query_results(vec![vec![room]])
            .append_query_results(vec![
                Vec::<crate::database::models::room_member::Model>::new(),
            ])
            .into_connection();
        let mailer = make_mailer();
        let config = make_config();
        let service = RoomService::new(db, mailer, config, None);

        let result = service.get_room_detail(room_id, Uuid::new_v4()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn get_room_detail_success() {
        let user_id = Uuid::new_v4();
        let room_id = Uuid::new_v4();
        let member_id = Uuid::new_v4();
        let room = make_room_model(room_id, user_id, "Test Room", "INV001");
        let member = make_room_member(member_id, room_id, user_id);
        let user = make_user_model(user_id, "player1");

        let db = MockDatabase::new(DatabaseBackend::Postgres)
            .append_query_results(vec![vec![room]])
            .append_query_results(vec![vec![member.clone()]])
            .append_query_results(vec![vec![member.clone()]])
            .append_query_results(vec![vec![user]])
            .append_query_results(vec![Vec::<crate::database::models::game_run::Model>::new()])
            .into_connection();
        let mailer = make_mailer();
        let config = make_config();
        let service = RoomService::new(db, mailer, config, None);

        let result = service.get_room_detail(room_id, user_id).await;
        assert!(result.is_ok());
        let detail = result.unwrap();
        assert_eq!(detail["id"], serde_json::json!(room_id));
        assert_eq!(detail["name"], serde_json::json!("Test Room"));
        assert_eq!(detail["member_count"], serde_json::json!(1));
    }

    #[tokio::test]
    async fn create_run_not_member_returns_error() {
        let room_id = Uuid::new_v4();
        let room = make_room_model(room_id, Uuid::new_v4(), "Room", "CODE");

        let db = MockDatabase::new(DatabaseBackend::Postgres)
            .append_query_results(vec![vec![room]])
            .append_query_results(vec![
                Vec::<crate::database::models::room_member::Model>::new(),
            ])
            .into_connection();
        let mailer = make_mailer();
        let config = make_config();
        let service = RoomService::new(db, mailer, config, None);

        let result = service
            .create_run(
                room_id,
                Uuid::new_v4(),
                3,
                100,
                &[Uuid::new_v4(), Uuid::new_v4()],
            )
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn create_run_requires_positive_num_games() {
        let db = MockDatabase::new(DatabaseBackend::Postgres).into_connection();
        let mailer = make_mailer();
        let config = make_config();
        let service = RoomService::new(db, mailer, config, None);

        let result = service
            .create_run(
                Uuid::new_v4(),
                Uuid::new_v4(),
                0,
                100,
                &[Uuid::new_v4(), Uuid::new_v4()],
            )
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn create_run_requires_positive_bet() {
        let db = MockDatabase::new(DatabaseBackend::Postgres).into_connection();
        let mailer = make_mailer();
        let config = make_config();
        let service = RoomService::new(db, mailer, config, None);

        let result = service
            .create_run(
                Uuid::new_v4(),
                Uuid::new_v4(),
                3,
                0,
                &[Uuid::new_v4(), Uuid::new_v4()],
            )
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn create_run_requires_at_least_two_players() {
        let db = MockDatabase::new(DatabaseBackend::Postgres).into_connection();
        let mailer = make_mailer();
        let config = make_config();
        let service = RoomService::new(db, mailer, config, None);

        let result = service
            .create_run(Uuid::new_v4(), Uuid::new_v4(), 3, 100, &[Uuid::new_v4()])
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn create_run_too_many_players_rejected() {
        let db = MockDatabase::new(DatabaseBackend::Postgres).into_connection();
        let mailer = make_mailer();
        let config = make_config();
        let service = RoomService::new(db, mailer, config, None);

        let player_ids: Vec<Uuid> = (0..10).map(|_| Uuid::new_v4()).collect();
        let result = service
            .create_run(Uuid::new_v4(), Uuid::new_v4(), 3, 100, &player_ids)
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn create_run_run_already_active_returns_error() {
        let user_id = Uuid::new_v4();
        let room_id = Uuid::new_v4();
        let room = make_room_model(room_id, user_id, "Room", "CODE");
        let member = make_room_member(Uuid::new_v4(), room_id, user_id);

        use sea_orm::prelude::DateTimeUtc;
        let active_run = crate::database::models::game_run::Model {
            id: Uuid::new_v4(),
            room_id,
            num_games: 3,
            bet_per_game: 100,
            num_players: 2,
            current_game_index: 0,
            status: "active".to_string(),
            created_by: user_id,
            next_game_auto_start_at: None,
            stall_warning_sent_at: None,
            stall_cancelled_at: None,
            created_at: DateTimeUtc::from_timestamp(0, 0).unwrap(),
            updated_at: DateTimeUtc::from_timestamp(0, 0).unwrap(),
        };

        let db = MockDatabase::new(DatabaseBackend::Postgres)
            .append_query_results(vec![vec![room]])
            .append_query_results(vec![vec![member]])
            .append_query_results(vec![vec![active_run]])
            .into_connection();
        let mailer = make_mailer();
        let config = make_config();
        let service = RoomService::new(db, mailer, config, None);

        let result = service
            .create_run(room_id, user_id, 3, 100, &[user_id, Uuid::new_v4()])
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn create_run_not_all_players_are_members() {
        let user_id = Uuid::new_v4();
        let room_id = Uuid::new_v4();
        let player2 = Uuid::new_v4();
        let room = make_room_model(room_id, user_id, "Room", "CODE");
        let member = make_room_member(Uuid::new_v4(), room_id, user_id);

        let db = MockDatabase::new(DatabaseBackend::Postgres)
            .append_query_results(vec![vec![room]])
            .append_query_results(vec![vec![member.clone()]])
            .append_query_results(vec![Vec::<crate::database::models::game_run::Model>::new()])
            .append_query_results(vec![vec![member]])
            .into_connection();
        let mailer = make_mailer();
        let config = make_config();
        let service = RoomService::new(db, mailer, config, None);

        let result = service
            .create_run(room_id, user_id, 3, 100, &[user_id, player2])
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn list_runs_not_member_returns_error() {
        let room_id = Uuid::new_v4();
        let room = make_room_model(room_id, Uuid::new_v4(), "Room", "CODE");

        let db = MockDatabase::new(DatabaseBackend::Postgres)
            .append_query_results(vec![vec![room]])
            .append_query_results(vec![
                Vec::<crate::database::models::room_member::Model>::new(),
            ])
            .into_connection();
        let mailer = make_mailer();
        let config = make_config();
        let service = RoomService::new(db, mailer, config, None);

        let result = service.list_runs(room_id, Uuid::new_v4()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn join_run_not_found_returns_error() {
        let db = MockDatabase::new(DatabaseBackend::Postgres)
            .append_query_results(vec![Vec::<crate::database::models::game_run::Model>::new()])
            .into_connection();
        let mailer = make_mailer();
        let config = make_config();
        let service = RoomService::new(db, mailer, config, None);

        let result = service.join_run(Uuid::new_v4(), Uuid::new_v4()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn leave_run_require_existing_run_player() {
        let run_id = Uuid::new_v4();
        use sea_orm::prelude::DateTimeUtc;
        let run = crate::database::models::game_run::Model {
            id: run_id,
            room_id: Uuid::new_v4(),
            num_games: 3,
            bet_per_game: 100,
            num_players: 2,
            current_game_index: 1,
            status: "active".to_string(),
            created_by: Uuid::new_v4(),
            next_game_auto_start_at: None,
            stall_warning_sent_at: None,
            stall_cancelled_at: None,
            created_at: DateTimeUtc::from_timestamp(0, 0).unwrap(),
            updated_at: DateTimeUtc::from_timestamp(0, 0).unwrap(),
        };

        let db = MockDatabase::new(DatabaseBackend::Postgres)
            .append_query_results(vec![vec![run]])
            .append_query_results(vec![
                Vec::<crate::database::models::game_run_player::Model>::new(),
            ])
            .into_connection();
        let mailer = make_mailer();
        let config = make_config();
        let service = RoomService::new(db, mailer, config, None);

        let result = service.leave_run(run_id, Uuid::new_v4()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn join_run_require_active_status() {
        let db = MockDatabase::new(DatabaseBackend::Postgres).into_connection();
        let mailer = make_mailer();
        let config = make_config();
        let service = RoomService::new(db, mailer, config, None);

        let result = service.join_run(Uuid::new_v4(), Uuid::new_v4()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn get_active_run_not_member_returns_error() {
        let room_id = Uuid::new_v4();
        let room = make_room_model(room_id, Uuid::new_v4(), "Room", "CODE");

        let db = MockDatabase::new(DatabaseBackend::Postgres)
            .append_query_results(vec![vec![room]])
            .append_query_results(vec![
                Vec::<crate::database::models::room_member::Model>::new(),
            ])
            .into_connection();
        let mailer = make_mailer();
        let config = make_config();
        let service = RoomService::new(db, mailer, config, None);

        let result = service.get_active_run(room_id, Uuid::new_v4()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn get_current_game_not_found() {
        let run_id = Uuid::new_v4();
        let db = MockDatabase::new(DatabaseBackend::Postgres)
            .append_query_results(vec![Vec::<crate::database::models::game_run::Model>::new()])
            .into_connection();
        let mailer = make_mailer();
        let config = make_config();
        let service = RoomService::new(db, mailer, config, None);

        let result = service.get_current_game(run_id, Uuid::new_v4()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn start_next_game_not_found() {
        let db = MockDatabase::new(DatabaseBackend::Postgres)
            .append_query_results(vec![Vec::<crate::database::models::game_run::Model>::new()])
            .into_connection();
        let mailer = make_mailer();
        let config = make_config();
        let service = RoomService::new(db, mailer, config, None);

        let result = service
            .start_next_game(Uuid::new_v4(), Uuid::new_v4())
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn check_stalled_runs_returns_zero_on_empty_db() {
        let db = MockDatabase::new(DatabaseBackend::Postgres)
            .append_query_results(vec![Vec::<crate::database::models::game_run::Model>::new()])
            .into_connection();
        let mailer = make_mailer();

        let count = RoomService::check_stalled_runs(db, mailer, 1800).await;
        assert_eq!(count, 0);
    }
}
