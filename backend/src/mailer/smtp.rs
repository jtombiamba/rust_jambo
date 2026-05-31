use async_trait::async_trait;
use handlebars::Handlebars;
use lettre::message::Mailbox;
use lettre::{AsyncTransport, Message};
use std::sync::Arc;

use super::{
    ContactFormEmail, FreezeExpiredEmail, InvitationEmail, MailerConfig, PasswordResetEmail,
    StallKickedEmail, StallWarningEmail,
};
use crate::i18n::{Lang, Translator};
use crate::mailer::Mailer;

pub struct SmtpMailer {
    pub(crate) mailer: lettre::AsyncSmtpTransport<lettre::Tokio1Executor>,
    pub(crate) from: Mailbox,
    pub(crate) handlebars: Arc<Handlebars<'static>>,
    pub(crate) config: MailerConfig,
    translator: Arc<Translator>,
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

        handlebars
            .register_template_string(
                "en_stall_warning",
                include_str!("../../templates/en/stall_warning.hbs"),
            )
            .map_err(|e| format!("Failed to register en/stall_warning template: {e}"))?;

        handlebars
            .register_template_string(
                "fr_stall_warning",
                include_str!("../../templates/fr/stall_warning.hbs"),
            )
            .map_err(|e| format!("Failed to register fr/stall_warning template: {e}"))?;

        handlebars
            .register_template_string(
                "en_stall_kicked",
                include_str!("../../templates/en/stall_kicked.hbs"),
            )
            .map_err(|e| format!("Failed to register en/stall_kicked template: {e}"))?;

        handlebars
            .register_template_string(
                "fr_stall_kicked",
                include_str!("../../templates/fr/stall_kicked.hbs"),
            )
            .map_err(|e| format!("Failed to register fr/stall_kicked template: {e}"))?;

        handlebars
            .register_template_string(
                "en_room_invitation",
                include_str!("../../templates/en/room_invitation.hbs"),
            )
            .map_err(|e| format!("Failed to register en/room_invitation template: {e}"))?;

        handlebars
            .register_template_string(
                "fr_room_invitation",
                include_str!("../../templates/fr/room_invitation.hbs"),
            )
            .map_err(|e| format!("Failed to register fr/room_invitation template: {e}"))?;

        let translator = Arc::new(Translator::new());

        Ok(Self {
            mailer: transport,
            from,
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
impl Mailer for SmtpMailer {
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

        let to: Mailbox = format!("<{to_email}>")
            .parse()
            .map_err(|e| format!("Invalid to address: {e}"))?;

        let subject = self.translator.t("email.subject_password_reset", lang);

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

        tracing::info!("Password reset email sent to {to_email}");
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
            accept_link,
            decline_link,
        };

        let html = self
            .handlebars
            .render(&self.template_name("invitation", lang), &data)
            .map_err(|e| format!("Failed to render template: {e}"))?;

        let to: Mailbox = format!("<{to_email}>")
            .parse()
            .map_err(|e| format!("Invalid to address: {e}"))?;

        let subject_raw = self.translator.t("email.subject_invitation", lang);
        let subject = subject_raw.replace("{inviter_name}", inviter_name);

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

        let to: Mailbox = format!("<{to_email}>")
            .parse()
            .map_err(|e| format!("Invalid to address: {e}"))?;

        let subject = self.translator.t("email.subject_freeze_expired", lang);

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

        tracing::info!("Freeze expired email sent to {to_email}");
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

        let to: Mailbox = format!("<{}>", self.config.contact_to_email)
            .parse()
            .map_err(|e| format!("Invalid to address: {e}"))?;

        let subject_text =
            self.translator
                .t_replace("email.subject_contact", lang, "{subject}", subject);

        let email_msg = Message::builder()
            .from(self.from.clone())
            .reply_to(
                format!("{name} <{email}>")
                    .parse::<Mailbox>()
                    .map_err(|e| format!("Invalid reply-to address: {e}"))?,
            )
            .to(to)
            .subject(subject_text)
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

    async fn send_stall_warning(
        &self,
        to_email: &str,
        game_id: &str,
        inactive_minutes: i64,
        remaining_minutes: i64,
        lang: Lang,
    ) -> Result<(), String> {
        let data = StallWarningEmail {
            game_id: game_id.to_string(),
            inactive_minutes,
            remaining_minutes,
            frontend_url: self.config.frontend_url.clone(),
            app_name: self.config.smtp_from_name.clone(),
        };

        let html = self
            .handlebars
            .render(&self.template_name("stall_warning", lang), &data)
            .map_err(|e| format!("Failed to render template: {e}"))?;

        let to: Mailbox = format!("<{to_email}>")
            .parse()
            .map_err(|e| format!("Invalid to address: {e}"))?;

        let subject = self.translator.t("email.subject_stall_warning", lang);

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

        tracing::info!("Stall warning email sent to {to_email} for game {game_id}");
        Ok(())
    }

    async fn send_stall_kicked(
        &self,
        to_email: &str,
        game_id: &str,
        bet: i32,
        lang: Lang,
    ) -> Result<(), String> {
        let data = StallKickedEmail {
            game_id: game_id.to_string(),
            bet,
            frontend_url: self.config.frontend_url.clone(),
            app_name: self.config.smtp_from_name.clone(),
        };

        let html = self
            .handlebars
            .render(&self.template_name("stall_kicked", lang), &data)
            .map_err(|e| format!("Failed to render template: {e}"))?;

        let to: Mailbox = format!("<{to_email}>")
            .parse()
            .map_err(|e| format!("Invalid to address: {e}"))?;

        let subject = self.translator.t("email.subject_stall_kicked", lang);

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

        tracing::info!("Stall kicked email sent to {to_email} for game {game_id}");
        Ok(())
    }

    async fn send_room_invitation(
        &self,
        to_email: &str,
        inviter_name: &str,
        room_name: &str,
        invitation_code: &str,
        lang: Lang,
    ) -> Result<(), String> {
        let join_link = format!(
            "{}/rooms/join?code={invitation_code}",
            self.config.frontend_url
        );

        let data = super::RoomInvitationEmail {
            inviter_name: inviter_name.to_string(),
            room_name: room_name.to_string(),
            invitation_code: invitation_code.to_string(),
            join_link: join_link.clone(),
            frontend_url: self.config.frontend_url.clone(),
            app_name: self.config.smtp_from_name.clone(),
        };

        let html = self
            .handlebars
            .render(&self.template_name("room_invitation", lang), &data)
            .map_err(|e| format!("Failed to render template: {e}"))?;

        let to: Mailbox = format!("<{to_email}>")
            .parse()
            .map_err(|e| format!("Invalid to address: {e}"))?;

        let subject = self
            .translator
            .t("email.subject_room_invite", lang)
            .replace("{sender}", inviter_name)
            .replace("{room_name}", room_name);

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

        tracing::info!("Room invitation email sent to {to_email} for room {room_name}");
        Ok(())
    }
}
