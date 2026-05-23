use async_trait::async_trait;
use handlebars::Handlebars;
use lettre::message::Mailbox;
use lettre::{AsyncTransport, Message};
use std::sync::Arc;

use super::{
    ContactFormEmail, FreezeExpiredEmail, InvitationEmail, MailerConfig, PasswordResetEmail,
};
use crate::mailer::Mailer;

pub struct SmtpMailer {
    pub(crate) mailer: lettre::AsyncSmtpTransport<lettre::Tokio1Executor>,
    pub(crate) from: Mailbox,
    pub(crate) handlebars: Arc<Handlebars<'static>>,
    pub(crate) config: MailerConfig,
}

impl SmtpMailer {
    pub fn new(config: MailerConfig) -> Result<Self, String> {
        let from: Mailbox = format!("{} <{}>", config.smtp_from_name, config.smtp_from_email)
            .parse()
            .map_err(|e| format!("Invalid from address: {e}"))?;

        let creds = lettre::transport::smtp::authentication::Credentials::new(
            config.smtp_username.clone(),
            config.smtp_password.clone(),
        );

        let transport = if config.smtp_tls {
            lettre::AsyncSmtpTransport::<lettre::Tokio1Executor>::starttls_relay(&config.smtp_host)
                .map_err(|e| format!("Failed to create SMTP transport: {e}"))?
                .port(config.smtp_port)
                .credentials(creds)
                .build()
        } else {
            lettre::AsyncSmtpTransport::<lettre::Tokio1Executor>::builder_dangerous(
                &config.smtp_host,
            )
            .port(config.smtp_port)
            .credentials(creds)
            .build()
        };

        let mut handlebars = Handlebars::new();
        handlebars.set_strict_mode(true);

        handlebars
            .register_template_string(
                "password_reset",
                include_str!("../../templates/password_reset.hbs"),
            )
            .map_err(|e| format!("Failed to register password_reset template: {e}"))?;

        handlebars
            .register_template_string("invitation", include_str!("../../templates/invitation.hbs"))
            .map_err(|e| format!("Failed to register invitation template: {e}"))?;

        handlebars
            .register_template_string(
                "freeze_expired",
                include_str!("../../templates/freeze_expired.hbs"),
            )
            .map_err(|e| format!("Failed to register freeze_expired template: {e}"))?;

        handlebars
            .register_template_string(
                "contact_form",
                include_str!("../../templates/contact_form.hbs"),
            )
            .map_err(|e| format!("Failed to register contact_form template: {e}"))?;

        Ok(Self {
            mailer: transport,
            from,
            handlebars: Arc::new(handlebars),
            config,
        })
    }
}

#[async_trait]
impl Mailer for SmtpMailer {
    async fn send_password_reset(&self, to_email: &str, reset_link: &str) -> Result<(), String> {
        let data = PasswordResetEmail {
            reset_link: reset_link.to_string(),
            frontend_url: self.config.frontend_url.clone(),
            app_name: self.config.smtp_from_name.clone(),
        };

        let html = self
            .handlebars
            .render("password_reset", &data)
            .map_err(|e| format!("Failed to render template: {e}"))?;

        let to: Mailbox = format!("<{to_email}>")
            .parse()
            .map_err(|e| format!("Invalid to address: {e}"))?;

        let email = Message::builder()
            .from(self.from.clone())
            .to(to)
            .subject("Reset your password - FapFap Game")
            .header(lettre::message::header::ContentType::TEXT_HTML)
            .body(html)
            .map_err(|e| format!("Failed to build email: {e}"))?;

        self.mailer
            .send(email)
            .await
            .map_err(|e| format!("Failed to send email: {e}"))?;

        tracing::info!("Password reset email sent to {to_email}");
        Ok(())
    }

    async fn send_invitation(
        &self,
        to_email: &str,
        inviter_name: &str,
        game_id: &str,
    ) -> Result<(), String> {
        let accept_link = format!(
            "{}?invite_game_id={game_id}&invite_action=accept",
            self.config.frontend_url
        );
        let decline_link = format!(
            "{}?invite_game_id={game_id}&invite_action=decline",
            self.config.frontend_url
        );

        let data = InvitationEmail {
            inviter_name: inviter_name.to_string(),
            game_id: game_id.to_string(),
            frontend_url: self.config.frontend_url.clone(),
            app_name: self.config.smtp_from_name.clone(),
            accept_link,
            decline_link,
        };

        let html = self
            .handlebars
            .render("invitation", &data)
            .map_err(|e| format!("Failed to render template: {e}"))?;

        let to: Mailbox = format!("<{to_email}>")
            .parse()
            .map_err(|e| format!("Invalid to address: {e}"))?;

        let subject = format!("{inviter_name} invited you to a game - FapFap Game");

        let email = Message::builder()
            .from(self.from.clone())
            .to(to)
            .subject(subject)
            .header(lettre::message::header::ContentType::TEXT_HTML)
            .body(html)
            .map_err(|e| format!("Failed to build email: {e}"))?;

        self.mailer
            .send(email)
            .await
            .map_err(|e| format!("Failed to send email: {e}"))?;

        tracing::info!("Invitation email sent to {to_email} for game {game_id}");
        Ok(())
    }

    async fn send_freeze_expired(&self, to_email: &str, credit: i32) -> Result<(), String> {
        let data = FreezeExpiredEmail {
            frontend_url: self.config.frontend_url.clone(),
            app_name: self.config.smtp_from_name.clone(),
            credit,
        };

        let html = self
            .handlebars
            .render("freeze_expired", &data)
            .map_err(|e| format!("Failed to render template: {e}"))?;

        let to: Mailbox = format!("<{to_email}>")
            .parse()
            .map_err(|e| format!("Invalid to address: {e}"))?;

        let email = Message::builder()
            .from(self.from.clone())
            .to(to)
            .subject("Your account has been unfrozen - FapFap Game")
            .header(lettre::message::header::ContentType::TEXT_HTML)
            .body(html)
            .map_err(|e| format!("Failed to build email: {e}"))?;

        self.mailer
            .send(email)
            .await
            .map_err(|e| format!("Failed to send email: {e}"))?;

        tracing::info!("Freeze expired email sent to {to_email}");
        Ok(())
    }

    async fn send_contact_form(
        &self,
        name: &str,
        email: &str,
        subject: &str,
        message: &str,
    ) -> Result<(), String> {
        let data = ContactFormEmail {
            name: name.to_string(),
            email: email.to_string(),
            subject: subject.to_string(),
            message: message.to_string(),
        };

        let html = self
            .handlebars
            .render("contact_form", &data)
            .map_err(|e| format!("Failed to render template: {e}"))?;

        let to: Mailbox = format!("<{}>", self.config.contact_to_email)
            .parse()
            .map_err(|e| format!("Invalid to address: {e}"))?;

        let email_msg = Message::builder()
            .from(self.from.clone())
            .reply_to(
                format!("{name} <{email}>")
                    .parse::<Mailbox>()
                    .map_err(|e| format!("Invalid reply-to address: {e}"))?,
            )
            .to(to)
            .subject(format!("Contact: {subject}"))
            .header(lettre::message::header::ContentType::TEXT_HTML)
            .body(html)
            .map_err(|e| format!("Failed to build email: {e}"))?;

        self.mailer
            .send(email_msg)
            .await
            .map_err(|e| format!("Failed to send email: {e}"))?;

        tracing::info!("Contact form email sent from {email}");
        Ok(())
    }
}
