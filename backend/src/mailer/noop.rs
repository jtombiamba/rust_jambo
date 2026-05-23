use async_trait::async_trait;
use handlebars::Handlebars;
use std::sync::Arc;

use super::{
    ContactFormEmail, FreezeExpiredEmail, InvitationEmail, MailerConfig, PasswordResetEmail,
};
use crate::i18n::{Lang, Translator};
use crate::mailer::Mailer;

pub struct NoopMailer {
    pub(crate) handlebars: Arc<Handlebars<'static>>,
    pub(crate) config: MailerConfig,
    #[allow(dead_code)]
    translator: Arc<Translator>,
}

impl NoopMailer {
    pub fn new(config: MailerConfig) -> Result<Self, String> {
        let mut handlebars = Handlebars::new();
        handlebars.set_strict_mode(true);

        handlebars
            .register_template_string(
                "en_password_reset",
                include_str!("../../templates/en/password_reset.hbs"),
            )
            .map_err(|e| format!("Failed to register en/password_reset template: {e}"))?;

        handlebars
            .register_template_string(
                "fr_password_reset",
                include_str!("../../templates/fr/password_reset.hbs"),
            )
            .map_err(|e| format!("Failed to register fr/password_reset template: {e}"))?;

        handlebars
            .register_template_string(
                "en_invitation",
                include_str!("../../templates/en/invitation.hbs"),
            )
            .map_err(|e| format!("Failed to register en/invitation template: {e}"))?;

        handlebars
            .register_template_string(
                "fr_invitation",
                include_str!("../../templates/fr/invitation.hbs"),
            )
            .map_err(|e| format!("Failed to register fr/invitation template: {e}"))?;

        handlebars
            .register_template_string(
                "en_freeze_expired",
                include_str!("../../templates/en/freeze_expired.hbs"),
            )
            .map_err(|e| format!("Failed to register en/freeze_expired template: {e}"))?;

        handlebars
            .register_template_string(
                "fr_freeze_expired",
                include_str!("../../templates/fr/freeze_expired.hbs"),
            )
            .map_err(|e| format!("Failed to register fr/freeze_expired template: {e}"))?;

        handlebars
            .register_template_string(
                "en_contact_form",
                include_str!("../../templates/en/contact_form.hbs"),
            )
            .map_err(|e| format!("Failed to register en/contact_form template: {e}"))?;

        handlebars
            .register_template_string(
                "fr_contact_form",
                include_str!("../../templates/fr/contact_form.hbs"),
            )
            .map_err(|e| format!("Failed to register fr/contact_form template: {e}"))?;

        let translator = Arc::new(Translator::new());

        Ok(Self {
            handlebars: Arc::new(handlebars),
            config,
            translator,
        })
    }

    fn template_name(&self, name: &str, lang: Lang) -> String {
        format!("{}_{name}", lang.as_str())
    }
}

#[async_trait]
impl Mailer for NoopMailer {
    async fn send_password_reset(
        &self,
        to_email: &str,
        reset_link: &str,
        lang: Lang,
    ) -> Result<(), String> {
        let data = PasswordResetEmail {
            reset_link: reset_link.to_string(),
            frontend_url: self.config.frontend_url.clone(),
            app_name: self.config.smtp_from_name.clone(),
        };

        let html = self
            .handlebars
            .render(&self.template_name("password_reset", lang), &data)
            .map_err(|e| format!("Failed to render template: {e}"))?;

        tracing::info!(
            "[MAILER] Password reset email for {to_email}:\nReset link: {reset_link}\nHTML:\n{html}"
        );
        Ok(())
    }

    async fn send_invitation(
        &self,
        to_email: &str,
        inviter_name: &str,
        game_id: &str,
        lang: Lang,
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
            accept_link: accept_link.clone(),
            decline_link: decline_link.clone(),
        };

        let html = self
            .handlebars
            .render(&self.template_name("invitation", lang), &data)
            .map_err(|e| format!("Failed to render template: {e}"))?;

        tracing::info!(
            "[MAILER] Invitation email for {to_email} from {inviter_name} (game {game_id}):\nAccept: {accept_link}\nDecline: {decline_link}\nHTML:\n{html}"
        );
        Ok(())
    }

    async fn send_freeze_expired(
        &self,
        to_email: &str,
        credit: i32,
        lang: Lang,
    ) -> Result<(), String> {
        let data = FreezeExpiredEmail {
            frontend_url: self.config.frontend_url.clone(),
            app_name: self.config.smtp_from_name.clone(),
            credit,
        };

        let html = self
            .handlebars
            .render(&self.template_name("freeze_expired", lang), &data)
            .map_err(|e| format!("Failed to render template: {e}"))?;

        tracing::info!(
            "[MAILER] Freeze expired email for {to_email} (credit={credit}):\nHTML:\n{html}"
        );
        Ok(())
    }

    async fn send_contact_form(
        &self,
        name: &str,
        email: &str,
        subject: &str,
        message: &str,
        lang: Lang,
    ) -> Result<(), String> {
        let data = ContactFormEmail {
            name: name.to_string(),
            email: email.to_string(),
            subject: subject.to_string(),
            message: message.to_string(),
        };

        let html = self
            .handlebars
            .render(&self.template_name("contact_form", lang), &data)
            .map_err(|e| format!("Failed to render template: {e}"))?;

        tracing::info!(
            "[MAILER] Contact form from {name} <{email}>, subject: {subject}:\nHTML:\n{html}"
        );
        Ok(())
    }
}
