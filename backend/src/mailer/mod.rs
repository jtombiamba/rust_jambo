use async_trait::async_trait;
use serde::Deserialize;
use std::sync::Arc;

use crate::i18n::Lang;

pub mod noop;
pub mod smtp;
pub use noop::NoopMailer;
pub use smtp::SmtpMailer;

#[derive(Clone, Deserialize)]
pub struct MailerConfig {
    pub mailer_mode: String,
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_username: String,
    pub smtp_password: String,
    pub smtp_tls: bool,
    pub smtp_from_email: String,
    pub smtp_from_name: String,
    pub frontend_url: String,
    pub contact_to_email: String,
}

impl std::fmt::Debug for MailerConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MailerConfig")
            .field("mailer_mode", &self.mailer_mode)
            .field("smtp_host", &self.smtp_host)
            .field("smtp_port", &self.smtp_port)
            .field("smtp_username", &self.smtp_username)
            .field("smtp_password", &"***")
            .field("smtp_tls", &self.smtp_tls)
            .field("smtp_from_email", &self.smtp_from_email)
            .field("smtp_from_name", &self.smtp_from_name)
            .field("frontend_url", &self.frontend_url)
            .field("contact_to_email", &self.contact_to_email)
            .finish()
    }
}

impl MailerConfig {
    pub fn from_env() -> Self {
        let mailer_mode = std::env::var("MAILER_MODE").unwrap_or_else(|_| "console".to_string());
        let smtp_host = std::env::var("SMTP_HOST").unwrap_or_else(|_| "smtp.gmail.com".to_string());
        let smtp_port = std::env::var("SMTP_PORT")
            .unwrap_or_else(|_| "587".to_string())
            .parse()
            .unwrap_or(587);
        let smtp_username = std::env::var("SMTP_USERNAME").unwrap_or_default();
        let smtp_password = std::env::var("SMTP_PASSWORD").unwrap_or_default();
        let smtp_tls = std::env::var("SMTP_TLS")
            .unwrap_or_else(|_| "true".to_string())
            .parse()
            .unwrap_or(true);
        let smtp_from_email =
            std::env::var("SMTP_FROM_EMAIL").unwrap_or_else(|_| "noreply@example.com".to_string());
        let smtp_from_name =
            std::env::var("SMTP_FROM_NAME").unwrap_or_else(|_| "FapFap Game".to_string());
        let frontend_url =
            std::env::var("FRONTEND_URL").unwrap_or_else(|_| "http://localhost:5173".to_string());
        let contact_to_email =
            std::env::var("CONTACT_TO_EMAIL").unwrap_or_else(|_| smtp_from_email.clone());

        Self {
            mailer_mode,
            smtp_host,
            smtp_port,
            smtp_username,
            smtp_password,
            smtp_tls,
            smtp_from_email,
            smtp_from_name,
            frontend_url,
            contact_to_email,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PasswordResetEmail {
    pub reset_link: String,
    pub frontend_url: String,
    pub app_name: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct InvitationEmail {
    pub inviter_name: String,
    pub game_id: String,
    pub frontend_url: String,
    pub app_name: String,
    pub accept_link: String,
    pub decline_link: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct FreezeExpiredEmail {
    pub frontend_url: String,
    pub app_name: String,
    pub credit: i32,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ContactFormEmail {
    pub name: String,
    pub email: String,
    pub subject: String,
    pub message: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct StallWarningEmail {
    pub game_id: String,
    pub inactive_minutes: i64,
    pub remaining_minutes: i64,
    pub frontend_url: String,
    pub app_name: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct StallKickedEmail {
    pub game_id: String,
    pub bet: i32,
    pub frontend_url: String,
    pub app_name: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct RoomInvitationEmail {
    pub inviter_name: String,
    pub room_name: String,
    pub invitation_code: String,
    pub join_link: String,
    pub frontend_url: String,
    pub app_name: String,
}

#[async_trait]
pub trait Mailer: Send + Sync {
    async fn send_password_reset(
        &self,
        to_email: &str,
        reset_link: &str,
        lang: Lang,
    ) -> Result<(), String>;

    async fn send_invitation(
        &self,
        to_email: &str,
        inviter_name: &str,
        game_id: &str,
        lang: Lang,
    ) -> Result<(), String>;

    async fn send_freeze_expired(
        &self,
        to_email: &str,
        credit: i32,
        lang: Lang,
    ) -> Result<(), String>;

    async fn send_contact_form(
        &self,
        name: &str,
        email: &str,
        subject: &str,
        message: &str,
        lang: Lang,
    ) -> Result<(), String>;

    async fn send_stall_warning(
        &self,
        to_email: &str,
        game_id: &str,
        inactive_minutes: i64,
        remaining_minutes: i64,
        lang: Lang,
    ) -> Result<(), String>;

    async fn send_stall_kicked(
        &self,
        to_email: &str,
        game_id: &str,
        bet: i32,
        lang: Lang,
    ) -> Result<(), String>;

    async fn send_room_invitation(
        &self,
        to_email: &str,
        inviter_name: &str,
        room_name: &str,
        invitation_code: &str,
        lang: Lang,
    ) -> Result<(), String>;
}

pub fn create_mailer(config: MailerConfig) -> Result<Arc<dyn Mailer>, String> {
    match config.mailer_mode.as_str() {
        "smtp" => {
            tracing::info!(
                "Creating SMTP mailer (host={}:{})",
                config.smtp_host,
                config.smtp_port
            );
            let smtp_mailer = SmtpMailer::new(config)?;
            Ok(Arc::new(smtp_mailer))
        }
        _ => {
            tracing::info!("Creating noop (console) mailer");
            let noop_mailer = NoopMailer::new(config)?;
            Ok(Arc::new(noop_mailer))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[tokio::test]
    async fn test_noop_mailer_password_reset() {
        let config = MailerConfig {
            mailer_mode: "console".to_string(),
            smtp_host: "".to_string(),
            smtp_port: 0,
            smtp_username: "".to_string(),
            smtp_password: "".to_string(),
            smtp_tls: true,
            smtp_from_email: "test@test.com".to_string(),
            smtp_from_name: "Test".to_string(),
            frontend_url: "http://localhost:3000".to_string(),
            contact_to_email: "support@example.com".to_string(),
        };
        let mailer = NoopMailer::new(config).unwrap();
        let result = mailer
            .send_password_reset(
                "user@example.com",
                "http://localhost:3000/reset?token=abc",
                Lang::En,
            )
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_noop_mailer_invitation() {
        let config = MailerConfig {
            mailer_mode: "console".to_string(),
            smtp_host: "".to_string(),
            smtp_port: 0,
            smtp_username: "".to_string(),
            smtp_password: "".to_string(),
            smtp_tls: true,
            smtp_from_email: "test@test.com".to_string(),
            smtp_from_name: "Test".to_string(),
            frontend_url: "http://localhost:3000".to_string(),
            contact_to_email: "support@example.com".to_string(),
        };
        let mailer = NoopMailer::new(config).unwrap();
        let result = mailer
            .send_invitation(
                "user@example.com",
                "PlayerOne",
                "550e8400-e29b-41d4-a716-446655440000",
                Lang::En,
            )
            .await;
        assert!(result.is_ok());
    }

    #[test]
    #[serial]
    fn test_mailer_config_from_env_defaults() {
        std::env::remove_var("MAILER_MODE");
        let config = MailerConfig::from_env();
        assert_eq!(config.mailer_mode, "console");
        assert_eq!(config.smtp_host, "smtp.gmail.com");
        assert_eq!(config.smtp_port, 587);
    }

    #[test]
    #[serial]
    fn test_mailer_config_from_env_smtp() {
        std::env::set_var("MAILER_MODE", "smtp");
        std::env::set_var("SMTP_HOST", "smtp.example.com");
        std::env::set_var("SMTP_PORT", "465");
        let config = MailerConfig::from_env();
        assert_eq!(config.mailer_mode, "smtp");
        assert_eq!(config.smtp_host, "smtp.example.com");
        assert_eq!(config.smtp_port, 465);
        std::env::remove_var("MAILER_MODE");
        std::env::remove_var("SMTP_HOST");
        std::env::remove_var("SMTP_PORT");
    }

    #[test]
    fn test_create_mailer_console_mode() {
        let config = MailerConfig {
            mailer_mode: "console".to_string(),
            smtp_host: "".to_string(),
            smtp_port: 0,
            smtp_username: "".to_string(),
            smtp_password: "".to_string(),
            smtp_tls: true,
            smtp_from_email: "test@test.com".to_string(),
            smtp_from_name: "Test".to_string(),
            frontend_url: "http://localhost:3000".to_string(),
            contact_to_email: "support@example.com".to_string(),
        };
        let mailer = create_mailer(config).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        assert!(rt
            .block_on(mailer.send_password_reset("test@test.com", "http://link", Lang::En))
            .is_ok());
    }

    #[test]
    fn test_template_rendering_password_reset() {
        let config = MailerConfig {
            mailer_mode: "console".to_string(),
            smtp_host: "".to_string(),
            smtp_port: 0,
            smtp_username: "".to_string(),
            smtp_password: "".to_string(),
            smtp_tls: true,
            smtp_from_email: "test@test.com".to_string(),
            smtp_from_name: "Test Game".to_string(),
            frontend_url: "http://localhost:3000".to_string(),
            contact_to_email: "support@example.com".to_string(),
        };
        let mailer = NoopMailer::new(config).unwrap();
        let data = PasswordResetEmail {
            reset_link: "http://localhost:3000/reset?token=abc123".to_string(),
            frontend_url: "http://localhost:3000".to_string(),
            app_name: "Test Game".to_string(),
        };
        let html = mailer
            .handlebars
            .render("en_password_reset", &data)
            .unwrap();
        assert!(html.contains("Reset your password"));
        assert!(html.contains("abc123"));
    }

    #[test]
    fn test_template_rendering_invitation() {
        let config = MailerConfig {
            mailer_mode: "console".to_string(),
            smtp_host: "".to_string(),
            smtp_port: 0,
            smtp_username: "".to_string(),
            smtp_password: "".to_string(),
            smtp_tls: true,
            smtp_from_email: "test@test.com".to_string(),
            smtp_from_name: "Test Game".to_string(),
            frontend_url: "http://localhost:3000".to_string(),
            contact_to_email: "support@example.com".to_string(),
        };
        let mailer = NoopMailer::new(config).unwrap();
        let data = InvitationEmail {
            inviter_name: "PlayerOne".to_string(),
            game_id: "game-id-123".to_string(),
            frontend_url: "http://localhost:3000".to_string(),
            app_name: "Test Game".to_string(),
            accept_link: "http://localhost:3000?invite_game_id=game-id-123&invite_action=accept"
                .to_string(),
            decline_link: "http://localhost:3000?invite_game_id=game-id-123&invite_action=decline"
                .to_string(),
        };
        let html = mailer.handlebars.render("en_invitation", &data).unwrap();
        assert!(html.contains("PlayerOne"));
        assert!(html.contains("invited you"));
        assert!(html.contains("Accept"));
        assert!(html.contains("Decline"));
    }
}
