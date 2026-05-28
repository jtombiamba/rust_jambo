#[cfg(test)]
mod tests {
    use sea_orm::{DatabaseBackend, MockDatabase};
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
    async fn join_run_require_active_status() {
        let db = MockDatabase::new(DatabaseBackend::Postgres).into_connection();
        let mailer = make_mailer();
        let config = make_config();
        let service = RoomService::new(db, mailer, config, None);

        let result = service.join_run(Uuid::new_v4(), Uuid::new_v4()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn leave_run_requires_existing_run_player() {
        let db = MockDatabase::new(DatabaseBackend::Postgres).into_connection();
        let mailer = make_mailer();
        let config = make_config();
        let service = RoomService::new(db, mailer, config, None);

        let result = service.leave_run(Uuid::new_v4(), Uuid::new_v4()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn check_stalled_runs_returns_zero_on_db_error() {
        let db = MockDatabase::new(DatabaseBackend::Postgres).into_connection();
        let mailer = make_mailer();

        let count = RoomService::check_stalled_runs(db, mailer, 1800).await;
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn create_room_rejects_invalid_name() {
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
    async fn list_user_rooms_empty_when_no_memberships() {
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
}
