//! Embedded Handlebars email templates, exposed as `pub static` strings.
//!
//! Templates are compiled into the binary via `include_str!`, so no runtime
//! `templates/` directory is required.

pub mod en {
    pub static PASSWORD_RESET: &str = include_str!("../templates/en/password_reset.hbs");
    pub static INVITATION: &str = include_str!("../templates/en/invitation.hbs");
    pub static FREEZE_EXPIRED: &str = include_str!("../templates/en/freeze_expired.hbs");
    pub static CONTACT_FORM: &str = include_str!("../templates/en/contact_form.hbs");
    pub static STALL_WARNING: &str = include_str!("../templates/en/stall_warning.hbs");
    pub static STALL_KICKED: &str = include_str!("../templates/en/stall_kicked.hbs");
    pub static ROOM_INVITATION: &str = include_str!("../templates/en/room_invitation.hbs");
    pub static CASHOUT_REQUESTED: &str = include_str!("../templates/en/cashout_requested.hbs");
    pub static CASHOUT_REJECTED: &str = include_str!("../templates/en/cashout_rejected.hbs");
    pub static CASHOUT_ADMIN_ALERT: &str = include_str!("../templates/en/cashout_admin_alert.hbs");
    pub static CASHOUT_PAID: &str = include_str!("../templates/en/cashout_paid.hbs");
    pub static CASHOUT_FAILED: &str = include_str!("../templates/en/cashout_failed.hbs");
    pub static CONNECT_ACCOUNT_VERIFIED: &str =
        include_str!("../templates/en/connect_account_verified.hbs");
    pub static CONNECT_ACCOUNT_VERIFICATION_FAILED: &str =
        include_str!("../templates/en/connect_account_verification_failed.hbs");
    pub static CONNECT_ACCOUNT_ONBOARDING_INTERRUPTED: &str =
        include_str!("../templates/en/connect_account_onboarding_interrupted.hbs");
    pub static DELETION_CONFIRM: &str = include_str!("../templates/en/deletion_confirm.hbs");
}

pub mod fr {
    pub static PASSWORD_RESET: &str = include_str!("../templates/fr/password_reset.hbs");
    pub static INVITATION: &str = include_str!("../templates/fr/invitation.hbs");
    pub static FREEZE_EXPIRED: &str = include_str!("../templates/fr/freeze_expired.hbs");
    pub static CONTACT_FORM: &str = include_str!("../templates/fr/contact_form.hbs");
    pub static STALL_WARNING: &str = include_str!("../templates/fr/stall_warning.hbs");
    pub static STALL_KICKED: &str = include_str!("../templates/fr/stall_kicked.hbs");
    pub static ROOM_INVITATION: &str = include_str!("../templates/fr/room_invitation.hbs");
    pub static CASHOUT_REQUESTED: &str = include_str!("../templates/fr/cashout_requested.hbs");
    pub static CASHOUT_REJECTED: &str = include_str!("../templates/fr/cashout_rejected.hbs");
    pub static CASHOUT_PAID: &str = include_str!("../templates/fr/cashout_paid.hbs");
    pub static CASHOUT_FAILED: &str = include_str!("../templates/fr/cashout_failed.hbs");
    pub static CONNECT_ACCOUNT_VERIFIED: &str =
        include_str!("../templates/fr/connect_account_verified.hbs");
    pub static CONNECT_ACCOUNT_VERIFICATION_FAILED: &str =
        include_str!("../templates/fr/connect_account_verification_failed.hbs");
    pub static CONNECT_ACCOUNT_ONBOARDING_INTERRUPTED: &str =
        include_str!("../templates/fr/connect_account_onboarding_interrupted.hbs");
    pub static DELETION_CONFIRM: &str = include_str!("../templates/fr/deletion_confirm.hbs");
}
