//! Payout and Stripe Connect notification email rendering for the SMTP mailer,
//! plus the full `Mailer` trait implementation, split out of `smtp.rs` to keep
//! that file within the line limit.

use async_trait::async_trait;
use lettre::message::Mailbox;
use lettre::{AsyncTransport, Message};

use super::{
    CashoutResultEmail, ConnectAccountStatusEmail, ContactFormEmail, FreezeExpiredEmail,
    InvitationEmail, PasswordResetEmail, StallKickedEmail, StallWarningEmail,
};
use crate::smtp::SmtpMailer;
use crate::Mailer;
use jambo_i18n::Lang;

impl SmtpMailer {
    pub(crate) fn template_name(&self, name: &str, lang: Lang) -> String {
        format!("{}_{name}", lang.as_str())
    }

    pub(crate) async fn send_html(
        &self,
        to: Mailbox,
        subject: String,
        html: String,
    ) -> Result<(), String> {
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
        Ok(())
    }

    pub(crate) async fn send_cashout_result(
        &self,
        to_email: &str,
        amount_eur_cents: i32,
        lang: Lang,
        succeeded: bool,
    ) -> Result<(), String> {
        let (template, kind) = if succeeded {
            ("cashout_paid", "cashout_paid")
        } else {
            ("cashout_failed", "cashout_failed")
        };
        let data = CashoutResultEmail {
            amount_eur: super::format_cents(amount_eur_cents),
            frontend_url: self.config.frontend_url.clone(),
            app_name: self.config.smtp_from_name.clone(),
        };
        let html = self
            .handlebars
            .render(&self.template_name(template, lang), &data)
            .map_err(|e| format!("Failed to render template: {e}"))?;
        let to: Mailbox = format!("<{to_email}>")
            .parse()
            .map_err(|e| format!("Invalid to address: {e}"))?;
        let subject = super::cashout_subject(kind, lang);
        self.send_html(to, subject, html).await
    }

    pub(crate) async fn send_connect_account_status(
        &self,
        to_email: &str,
        lang: Lang,
        kind: &str,
    ) -> Result<(), String> {
        let template = match kind {
            "verified" => "connect_account_verified",
            "verification_failed" => "connect_account_verification_failed",
            "onboarding_interrupted" => "connect_account_onboarding_interrupted",
            _ => "connect_account_verified",
        };
        let data = ConnectAccountStatusEmail {
            frontend_url: self.config.frontend_url.clone(),
            app_name: self.config.smtp_from_name.clone(),
        };
        let html = self
            .handlebars
            .render(&self.template_name(template, lang), &data)
            .map_err(|e| format!("Failed to render template: {e}"))?;
        let to: Mailbox = format!("<{to_email}>")
            .parse()
            .map_err(|e| format!("Invalid to address: {e}"))?;
        let subject = super::connect_account_subject(kind, lang);
        self.send_html(to, subject, html).await
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

    async fn send_cashout_requested(
        &self,
        to_email: &str,
        credits: i32,
        amount_eur_cents: i32,
        payout_destination: &str,
        lang: Lang,
    ) -> Result<(), String> {
        let data = super::CashoutRequestedEmail {
            credits,
            amount_eur: super::format_cents(amount_eur_cents),
            payout_destination: payout_destination.to_string(),
            frontend_url: self.config.frontend_url.clone(),
            app_name: self.config.smtp_from_name.clone(),
        };

        let html = self
            .handlebars
            .render(&self.template_name("cashout_requested", lang), &data)
            .map_err(|e| format!("Failed to render template: {e}"))?;

        let to: Mailbox = format!("<{to_email}>")
            .parse()
            .map_err(|e| format!("Invalid to address: {e}"))?;

        let subject = super::cashout_subject("cashout_requested", lang);
        self.send_html(to, subject, html).await
    }

    async fn send_cashout_rejected(
        &self,
        to_email: &str,
        credits: i32,
        amount_eur_cents: i32,
        lang: Lang,
    ) -> Result<(), String> {
        let data = super::CashoutRejectedEmail {
            credits,
            amount_eur: super::format_cents(amount_eur_cents),
            frontend_url: self.config.frontend_url.clone(),
            app_name: self.config.smtp_from_name.clone(),
        };

        let html = self
            .handlebars
            .render(&self.template_name("cashout_rejected", lang), &data)
            .map_err(|e| format!("Failed to render template: {e}"))?;

        let to: Mailbox = format!("<{to_email}>")
            .parse()
            .map_err(|e| format!("Invalid to address: {e}"))?;

        let subject = super::cashout_subject("cashout_rejected", lang);
        self.send_html(to, subject, html).await
    }

    async fn send_cashout_admin_alert(
        &self,
        to_email: &str,
        pseudo: &str,
        email: &str,
        credits: i32,
        amount_eur_cents: i32,
        payout_destination: &str,
    ) -> Result<(), String> {
        let data = super::CashoutAdminAlertEmail {
            pseudo: pseudo.to_string(),
            email: email.to_string(),
            credits,
            amount_eur: super::format_cents(amount_eur_cents),
            payout_destination: payout_destination.to_string(),
        };

        let html = self
            .handlebars
            .render("en_cashout_admin_alert", &data)
            .map_err(|e| format!("Failed to render template: {e}"))?;

        let to: Mailbox = format!("<{to_email}>")
            .parse()
            .map_err(|e| format!("Invalid to address: {e}"))?;

        self.send_html(to, "New cashout request".to_string(), html)
            .await
    }

    async fn send_cashout_paid(
        &self,
        to_email: &str,
        amount_eur_cents: i32,
        lang: Lang,
    ) -> Result<(), String> {
        self.send_cashout_result(to_email, amount_eur_cents, lang, true)
            .await
    }

    async fn send_cashout_failed(
        &self,
        to_email: &str,
        amount_eur_cents: i32,
        lang: Lang,
    ) -> Result<(), String> {
        self.send_cashout_result(to_email, amount_eur_cents, lang, false)
            .await
    }

    async fn send_connect_account_verified(
        &self,
        to_email: &str,
        lang: Lang,
    ) -> Result<(), String> {
        self.send_connect_account_status(to_email, lang, "verified")
            .await
    }

    async fn send_connect_account_verification_failed(
        &self,
        to_email: &str,
        lang: Lang,
    ) -> Result<(), String> {
        self.send_connect_account_status(to_email, lang, "verification_failed")
            .await
    }

    async fn send_connect_account_onboarding_interrupted(
        &self,
        to_email: &str,
        lang: Lang,
    ) -> Result<(), String> {
        self.send_connect_account_status(to_email, lang, "onboarding_interrupted")
            .await
    }

    async fn send_deletion_confirmation(
        &self,
        to_email: &str,
        confirm_link: &str,
        lang: Lang,
    ) -> Result<(), String> {
        let data = super::DeletionConfirmationEmail {
            confirm_link: confirm_link.to_string(),
            frontend_url: self.config.frontend_url.clone(),
            app_name: self.config.smtp_from_name.clone(),
        };

        let html = self
            .handlebars
            .render(&self.template_name("deletion_confirm", lang), &data)
            .map_err(|e| format!("Failed to render template: {e}"))?;

        let to: Mailbox = format!("<{to_email}>")
            .parse()
            .map_err(|e| format!("Invalid to address: {e}"))?;

        let subject = self.translator.t("email.subject_deletion_confirm", lang);

        self.send_html(to, subject, html).await
    }
}
