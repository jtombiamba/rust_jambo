use super::*;
use jambo_core::payment::AccountDetails;

#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "Enum", enum_name = "cashout_status")]
pub enum CashoutStatus {
    #[sea_orm(string_value = "requested")]
    Requested,
    #[sea_orm(string_value = "approved")]
    Approved,
    #[sea_orm(string_value = "rejected")]
    Rejected,
    #[sea_orm(string_value = "processing")]
    Processing,
    #[sea_orm(string_value = "paid")]
    Paid,
}

impl std::fmt::Display for CashoutStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            CashoutStatus::Requested => "requested",
            CashoutStatus::Approved => "approved",
            CashoutStatus::Rejected => "rejected",
            CashoutStatus::Processing => "processing",
            CashoutStatus::Paid => "paid",
        };
        write!(f, "{}", s)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "Enum", enum_name = "payout_method")]
pub enum PayoutMethod {
    #[sea_orm(string_value = "paypal_email")]
    PaypalEmail,
    #[sea_orm(string_value = "stripe_connect")]
    StripeConnect,
}

impl std::fmt::Display for PayoutMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            PayoutMethod::PaypalEmail => "paypal_email",
            PayoutMethod::StripeConnect => "stripe_connect",
        };
        write!(f, "{}", s)
    }
}

impl PayoutMethod {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "paypal_email" => Some(PayoutMethod::PaypalEmail),
            "stripe_connect" => Some(PayoutMethod::StripeConnect),
            _ => None,
        }
    }
}

pub mod admin_key {
    use super::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
    #[sea_orm(table_name = "admin_keys")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: Uuid,
        pub label: String,
        pub key_hash: String,
        pub created_at: DateTime<Utc>,
        pub revoked_at: Option<DateTime<Utc>>,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

pub mod cashout_request {
    use super::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
    #[sea_orm(table_name = "cashout_requests")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: Uuid,
        pub user_id: Uuid,
        pub credits: i32,
        pub amount_eur_cents: i32,
        pub status: super::CashoutStatus,
        pub payout_method: super::PayoutMethod,
        pub payout_destination: String,
        pub payout_reference: Option<String>,
        pub admin_note: Option<String>,
        pub processed_by: Option<String>,
        pub created_at: DateTime<Utc>,
        pub updated_at: DateTime<Utc>,
        pub processed_at: Option<DateTime<Utc>>,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {
        #[sea_orm(
            belongs_to = "super::super::user::Entity",
            from = "Column::UserId",
            to = "super::super::user::Column::Id"
        )]
        User,
    }

    impl Related<super::super::user::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::User.def()
        }
    }

    impl ActiveModelBehavior for ActiveModel {}
}

/// Lifecycle status of a user's Stripe Connect (Express) account, mirroring the
/// Django `AccountStatusType` lifecycle. Used to gate payouts and drive
/// notifications.
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(
    rs_type = "String",
    db_type = "Enum",
    enum_name = "connect_account_status"
)]
pub enum ConnectAccountStatus {
    #[sea_orm(string_value = "not_created")]
    NotCreated,
    #[sea_orm(string_value = "onboarding_started")]
    OnboardingStarted,
    #[sea_orm(string_value = "pending")]
    Pending,
    #[sea_orm(string_value = "verified")]
    Verified,
    #[sea_orm(string_value = "verification_failed")]
    VerificationFailed,
    #[sea_orm(string_value = "restricted")]
    Restricted,
    #[sea_orm(string_value = "disabled")]
    Disabled,
    #[sea_orm(string_value = "payment_paused")]
    PaymentPaused,
}

impl std::fmt::Display for ConnectAccountStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            ConnectAccountStatus::NotCreated => "not_created",
            ConnectAccountStatus::OnboardingStarted => "onboarding_started",
            ConnectAccountStatus::Pending => "pending",
            ConnectAccountStatus::Verified => "verified",
            ConnectAccountStatus::VerificationFailed => "verification_failed",
            ConnectAccountStatus::Restricted => "restricted",
            ConnectAccountStatus::Disabled => "disabled",
            ConnectAccountStatus::PaymentPaused => "payment_paused",
        };
        write!(f, "{s}")
    }
}

impl ConnectAccountStatus {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "not_created" => Some(ConnectAccountStatus::NotCreated),
            "onboarding_started" => Some(ConnectAccountStatus::OnboardingStarted),
            "pending" => Some(ConnectAccountStatus::Pending),
            "verified" => Some(ConnectAccountStatus::Verified),
            "verification_failed" => Some(ConnectAccountStatus::VerificationFailed),
            "restricted" => Some(ConnectAccountStatus::Restricted),
            "disabled" => Some(ConnectAccountStatus::Disabled),
            "payment_paused" => Some(ConnectAccountStatus::PaymentPaused),
            _ => None,
        }
    }
}

/// Stripe `requirements.disabled_reason` values that mean the account was
/// rejected (as opposed to merely restricted/compliance-blocked).
fn is_rejected_reason(reason: &str) -> bool {
    matches!(
        reason,
        "rejected.other"
            | "rejected.fraud"
            | "rejected.terms_of_service"
            | "rejected.incomplete_verification"
            | "rejected.listed"
    )
}

/// Map a Stripe Connect account's current state to a simplified business status.
///
/// Pure function so the status machine is unit-testable in isolation. Ordering
/// matters and mirrors the Django `evaluate_account_status` (with the documented
/// `CREATED`/`ONBOARDING_STARTED` drift resolved and `rejected.listed` handled as
/// a rejection).
pub fn evaluate_account_status(
    details: &AccountDetails,
    current: ConnectAccountStatus,
) -> ConnectAccountStatus {
    if !details.details_submitted {
        return ConnectAccountStatus::OnboardingStarted;
    }

    if current == ConnectAccountStatus::Pending {
        if !details.errors.is_empty() || !details.eventually_due.is_empty() {
            return ConnectAccountStatus::VerificationFailed;
        }
    } else if !details.currently_due.is_empty() || !details.eventually_due.is_empty() {
        return ConnectAccountStatus::Pending;
    }

    if let Some(reason) = &details.disabled_reason {
        if is_rejected_reason(reason) {
            return ConnectAccountStatus::Disabled;
        }
        return ConnectAccountStatus::Restricted;
    }

    if !details.past_due.is_empty() {
        return ConnectAccountStatus::Restricted;
    }

    if !details.charges_enabled || !details.payouts_enabled {
        return ConnectAccountStatus::PaymentPaused;
    }

    ConnectAccountStatus::Verified
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(clippy::too_many_arguments)]
    fn details(
        details_submitted: bool,
        charges_enabled: bool,
        payouts_enabled: bool,
        currently_due: Vec<&str>,
        eventually_due: Vec<&str>,
        past_due: Vec<&str>,
        errors: usize,
        disabled_reason: Option<&str>,
    ) -> AccountDetails {
        AccountDetails {
            details_submitted,
            charges_enabled,
            payouts_enabled,
            currently_due: currently_due.into_iter().map(String::from).collect(),
            eventually_due: eventually_due.into_iter().map(String::from).collect(),
            past_due: past_due.into_iter().map(String::from).collect(),
            errors: (0..errors)
                .map(|_| serde_json::json!({"code": "x"}))
                .collect(),
            disabled_reason: disabled_reason.map(String::from),
        }
    }

    #[test]
    fn onboarding_not_submitted() {
        let d = details(false, false, false, vec![], vec![], vec![], 0, None);
        assert_eq!(
            evaluate_account_status(&d, ConnectAccountStatus::NotCreated),
            ConnectAccountStatus::OnboardingStarted
        );
    }

    #[test]
    fn fully_verified() {
        let d = details(true, true, true, vec![], vec![], vec![], 0, None);
        assert_eq!(
            evaluate_account_status(&d, ConnectAccountStatus::OnboardingStarted),
            ConnectAccountStatus::Verified
        );
    }

    #[test]
    fn pending_documents() {
        let d = details(
            true,
            true,
            true,
            vec!["individual.dob"],
            vec![],
            vec![],
            0,
            None,
        );
        assert_eq!(
            evaluate_account_status(&d, ConnectAccountStatus::OnboardingStarted),
            ConnectAccountStatus::Pending
        );
    }

    #[test]
    fn verification_failed_when_previously_pending() {
        let d = details(true, true, true, vec![], vec![], vec![], 1, None);
        assert_eq!(
            evaluate_account_status(&d, ConnectAccountStatus::Pending),
            ConnectAccountStatus::VerificationFailed
        );
    }

    #[test]
    fn past_due_is_restricted() {
        let d = details(true, true, true, vec![], vec![], vec!["x"], 0, None);
        assert_eq!(
            evaluate_account_status(&d, ConnectAccountStatus::OnboardingStarted),
            ConnectAccountStatus::Restricted
        );
    }

    #[test]
    fn rejected_reason_is_disabled() {
        let d = details(
            true,
            true,
            true,
            vec![],
            vec![],
            vec![],
            0,
            Some("rejected.fraud"),
        );
        assert_eq!(
            evaluate_account_status(&d, ConnectAccountStatus::OnboardingStarted),
            ConnectAccountStatus::Disabled
        );
    }

    #[test]
    fn non_rejected_reason_is_restricted() {
        let d = details(
            true,
            true,
            true,
            vec![],
            vec![],
            vec![],
            0,
            Some("requirements.pending_verification"),
        );
        assert_eq!(
            evaluate_account_status(&d, ConnectAccountStatus::OnboardingStarted),
            ConnectAccountStatus::Restricted
        );
    }

    #[test]
    fn payment_paused_when_disabled_flags() {
        let d = details(true, false, true, vec![], vec![], vec![], 0, None);
        assert_eq!(
            evaluate_account_status(&d, ConnectAccountStatus::Verified),
            ConnectAccountStatus::PaymentPaused
        );
    }

    #[test]
    fn parse_round_trips() {
        for s in [
            "not_created",
            "onboarding_started",
            "pending",
            "verified",
            "verification_failed",
            "restricted",
            "disabled",
            "payment_paused",
        ] {
            assert_eq!(ConnectAccountStatus::parse(s).unwrap().to_string(), s);
        }
        assert!(ConnectAccountStatus::parse("bogus").is_none());
    }
}
