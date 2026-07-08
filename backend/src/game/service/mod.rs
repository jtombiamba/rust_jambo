mod ai_task;
mod caching;
mod creation;
mod evaluation;
mod events;
mod gameplay;
mod invites;
mod lifecycle;
mod recovery;
#[cfg(test)]
mod tests;
pub mod types;

use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use crate::config::Config;
use crate::mailer::Mailer;
use crate::messaging::RedisClient;
pub use types::{CardPlayResult, GameServiceError, MultiplayerGameOutcome};

pub const fn compute_display_position(
    actual_pos: usize,
    num_players: usize,
    my_pos: usize,
) -> usize {
    (num_players + actual_pos - my_pos) % num_players
}

fn is_unique_violation(e: &sea_orm::DbErr) -> bool {
    if let sea_orm::DbErr::Exec(exec_err) = e {
        exec_err.to_string().contains("23505")
    } else {
        false
    }
}

pub struct GameService {
    db: sea_orm::DatabaseConnection,
    redis_client: Option<RedisClient>,
    accept_invite_locks: tokio::sync::Mutex<HashMap<Uuid, Arc<tokio::sync::Mutex<()>>>>,
    pub(crate) freeze_duration_secs: u64,
    pub(crate) unfreeze_credit_no_payment: i32,
    mailer: Option<Arc<dyn Mailer>>,
}

impl GameService {
    #[allow(dead_code)]
    pub fn new(db: sea_orm::DatabaseConnection) -> Self {
        Self {
            db,
            redis_client: None,
            accept_invite_locks: tokio::sync::Mutex::new(HashMap::new()),
            freeze_duration_secs: 86400,
            unfreeze_credit_no_payment: 250,
            mailer: None,
        }
    }

    pub fn new_with_redis(
        db: sea_orm::DatabaseConnection,
        redis_client: Option<RedisClient>,
    ) -> Self {
        Self {
            db,
            redis_client,
            accept_invite_locks: tokio::sync::Mutex::new(HashMap::new()),
            freeze_duration_secs: 86400,
            unfreeze_credit_no_payment: 250,
            mailer: None,
        }
    }

    pub fn with_config(mut self, config: &Config, mailer: Arc<dyn Mailer>) -> Self {
        self.freeze_duration_secs = config.freeze_duration_secs;
        self.unfreeze_credit_no_payment = config.unfreeze_credit_no_payment;
        self.mailer = Some(mailer);
        self
    }

    #[allow(dead_code)]
    pub fn redis_client(&self) -> Option<RedisClient> {
        self.redis_client.clone()
    }

    pub(crate) async fn send_unfreeze_email(&self, user_id: Uuid) {
        let mailer = match &self.mailer {
            Some(m) => m.clone(),
            None => return,
        };

        use crate::observability::metrics::EMAIL_SEND_ERRORS_TOTAL;
        use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
        let profile = match crate::database::models::player_profile::Entity::find()
            .filter(crate::database::models::player_profile::Column::UserId.eq(user_id))
            .one(&self.db)
            .await
        {
            Ok(Some(p)) => p,
            Ok(None) => {
                tracing::warn!(
                    "No profile found for user {}, skipping unfreeze email",
                    user_id
                );
                EMAIL_SEND_ERRORS_TOTAL
                    .with_label_values(&["unfreeze"])
                    .inc();
                return;
            }
            Err(e) => {
                tracing::error!(
                    "DB error looking up profile for unfreeze email to {}: {}",
                    user_id,
                    e
                );
                EMAIL_SEND_ERRORS_TOTAL
                    .with_label_values(&["unfreeze"])
                    .inc();
                return;
            }
        };

        let user = match crate::database::models::user::Entity::find_by_id(profile.user_id)
            .one(&self.db)
            .await
        {
            Ok(Some(u)) => u,
            Ok(None) => {
                tracing::warn!("User {} not found for unfreeze email", profile.user_id);
                EMAIL_SEND_ERRORS_TOTAL
                    .with_label_values(&["unfreeze"])
                    .inc();
                return;
            }
            Err(e) => {
                tracing::error!(
                    "DB error looking up user {} for unfreeze email: {}",
                    profile.user_id,
                    e
                );
                EMAIL_SEND_ERRORS_TOTAL
                    .with_label_values(&["unfreeze"])
                    .inc();
                return;
            }
        };

        if let Err(e) = mailer
            .send_freeze_expired(
                &user.email,
                profile.credit,
                crate::i18n::Lang::parse(&user.language).unwrap_or_default(),
            )
            .await
        {
            tracing::error!("Failed to send unfreeze email to {}: {}", user.email, e);
            EMAIL_SEND_ERRORS_TOTAL
                .with_label_values(&["unfreeze"])
                .inc();
        }
    }
}
