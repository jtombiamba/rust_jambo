use handlebars::Handlebars;
use lettre::message::Mailbox;
use std::sync::Arc;

use super::MailerConfig;
use jambo_i18n::Translator;

pub struct SmtpMailer {
    pub(crate) mailer: lettre::AsyncSmtpTransport<lettre::Tokio1Executor>,
    pub(crate) from: Mailbox,
    pub(crate) handlebars: Arc<Handlebars<'static>>,
    pub(crate) config: MailerConfig,
    pub(crate) translator: Arc<Translator>,
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
            .register_template_string("en_password_reset", jambo_templates::en::PASSWORD_RESET)
            .map_err(|e| format!("Failed to register en/password_reset template: {e}"))?;

        handlebars
            .register_template_string("fr_password_reset", jambo_templates::fr::PASSWORD_RESET)
            .map_err(|e| format!("Failed to register fr/password_reset template: {e}"))?;

        handlebars
            .register_template_string("en_invitation", jambo_templates::en::INVITATION)
            .map_err(|e| format!("Failed to register en/invitation template: {e}"))?;

        handlebars
            .register_template_string("fr_invitation", jambo_templates::fr::INVITATION)
            .map_err(|e| format!("Failed to register fr/invitation template: {e}"))?;

        handlebars
            .register_template_string("en_freeze_expired", jambo_templates::en::FREEZE_EXPIRED)
            .map_err(|e| format!("Failed to register en/freeze_expired template: {e}"))?;

        handlebars
            .register_template_string("fr_freeze_expired", jambo_templates::fr::FREEZE_EXPIRED)
            .map_err(|e| format!("Failed to register fr/freeze_expired template: {e}"))?;

        handlebars
            .register_template_string("en_contact_form", jambo_templates::en::CONTACT_FORM)
            .map_err(|e| format!("Failed to register en/contact_form template: {e}"))?;

        handlebars
            .register_template_string("fr_contact_form", jambo_templates::fr::CONTACT_FORM)
            .map_err(|e| format!("Failed to register fr/contact_form template: {e}"))?;

        handlebars
            .register_template_string("en_stall_warning", jambo_templates::en::STALL_WARNING)
            .map_err(|e| format!("Failed to register en/stall_warning template: {e}"))?;

        handlebars
            .register_template_string("fr_stall_warning", jambo_templates::fr::STALL_WARNING)
            .map_err(|e| format!("Failed to register fr/stall_warning template: {e}"))?;

        handlebars
            .register_template_string("en_stall_kicked", jambo_templates::en::STALL_KICKED)
            .map_err(|e| format!("Failed to register en/stall_kicked template: {e}"))?;

        handlebars
            .register_template_string("fr_stall_kicked", jambo_templates::fr::STALL_KICKED)
            .map_err(|e| format!("Failed to register fr/stall_kicked template: {e}"))?;

        handlebars
            .register_template_string("en_room_invitation", jambo_templates::en::ROOM_INVITATION)
            .map_err(|e| format!("Failed to register en/room_invitation template: {e}"))?;

        handlebars
            .register_template_string("fr_room_invitation", jambo_templates::fr::ROOM_INVITATION)
            .map_err(|e| format!("Failed to register fr/room_invitation template: {e}"))?;

        super::register_cashout_templates(&mut handlebars)?;

        handlebars
            .register_template_string("en_deletion_confirm", jambo_templates::en::DELETION_CONFIRM)
            .map_err(|e| format!("Failed to register en/deletion_confirm template: {e}"))?;
        handlebars
            .register_template_string("fr_deletion_confirm", jambo_templates::fr::DELETION_CONFIRM)
            .map_err(|e| format!("Failed to register fr/deletion_confirm template: {e}"))?;

        let translator = Arc::new(Translator::new());

        Ok(Self {
            mailer: transport,
            from,
            handlebars: Arc::new(handlebars),
            config,
            translator,
        })
    }
}
