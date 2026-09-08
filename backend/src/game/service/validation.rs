use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter};
use uuid::Uuid;

use crate::database::models::player_profile;
use crate::error::GameError;
use crate::game::constants::KORA_CREDIT_MULTIPLIER;

pub(crate) async fn load_and_validate_profile<C: ConnectionTrait>(
    conn: &C,
    user_id: Uuid,
) -> Result<player_profile::Model, GameError> {
    let profile = player_profile::Entity::find()
        .filter(player_profile::Column::UserId.eq(user_id))
        .one(conn)
        .await?
        .ok_or(GameError::ProfileNotFound)?;

    if let Some(frozen_until) = profile.frozen_until {
        if frozen_until > chrono::Utc::now() {
            return Err(GameError::AccountFrozen {
                until: frozen_until.to_rfc3339(),
            });
        }
    }
    Ok(profile)
}

pub(crate) fn validate_sufficient_credit(
    profile: &player_profile::Model,
    bet: i32,
) -> Result<(), GameError> {
    let required_credit = bet * KORA_CREDIT_MULTIPLIER;
    if profile.credit < required_credit {
        return Err(GameError::InsufficientCredits {
            required: required_credit,
            current: profile.credit,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_profile(credit: i32) -> player_profile::Model {
        player_profile::Model {
            id: Uuid::now_v7(),
            user_id: Uuid::now_v7(),
            player_type: crate::database::models::PlayerType::Human,
            credit,
            game_played: 0,
            wins: 0,
            kora_wins: 0,
            winning_streak: 0,
            latitude: None,
            longitude: None,
            country_code: None,
            city: None,
            frozen_until: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn test_validate_sufficient_credit_ok() {
        let profile = make_profile(500);
        assert!(validate_sufficient_credit(&profile, 100).is_ok());
    }

    #[test]
    fn test_validate_sufficient_credit_fails() {
        let profile = make_profile(50);
        assert!(matches!(
            validate_sufficient_credit(&profile, 100),
            Err(GameError::InsufficientCredits { .. })
        ));
    }

    #[test]
    fn test_validate_sufficient_credit_exact_boundary() {
        let profile = make_profile(200);
        assert!(validate_sufficient_credit(&profile, 100).is_ok());
    }

    fn make_frozen_profile(frozen_until: Option<chrono::DateTime<Utc>>) -> player_profile::Model {
        let mut profile = make_profile(500);
        profile.frozen_until = frozen_until;
        profile
    }

    #[tokio::test]
    async fn test_load_and_validate_profile_not_found() {
        let db = sea_orm::MockDatabase::new(sea_orm::DatabaseBackend::Postgres)
            .append_query_results(vec![Vec::<player_profile::Model>::new()])
            .into_connection();

        let result = load_and_validate_profile(&db, Uuid::now_v7()).await;
        assert!(matches!(result, Err(GameError::ProfileNotFound)));
    }

    #[tokio::test]
    async fn test_load_and_validate_profile_frozen() {
        let frozen_until = Utc::now() + chrono::Duration::hours(1);
        let db = sea_orm::MockDatabase::new(sea_orm::DatabaseBackend::Postgres)
            .append_query_results(vec![vec![make_frozen_profile(Some(frozen_until))]])
            .into_connection();

        let result = load_and_validate_profile(&db, Uuid::now_v7()).await;
        assert!(matches!(result, Err(GameError::AccountFrozen { .. })));
    }

    #[tokio::test]
    async fn test_load_and_validate_profile_frozen_in_past_is_ok() {
        let frozen_until = Utc::now() - chrono::Duration::hours(1);
        let db = sea_orm::MockDatabase::new(sea_orm::DatabaseBackend::Postgres)
            .append_query_results(vec![vec![make_frozen_profile(Some(frozen_until))]])
            .into_connection();

        let profile = load_and_validate_profile(&db, Uuid::now_v7())
            .await
            .unwrap();
        assert_eq!(profile.credit, 500);
    }

    #[tokio::test]
    async fn test_load_and_validate_profile_ok() {
        let db = sea_orm::MockDatabase::new(sea_orm::DatabaseBackend::Postgres)
            .append_query_results(vec![vec![make_profile(500)]])
            .into_connection();

        let profile = load_and_validate_profile(&db, Uuid::now_v7())
            .await
            .unwrap();
        assert_eq!(profile.credit, 500);
    }
}
