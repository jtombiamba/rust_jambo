use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "Enum", enum_name = "cashout_status")]
pub enum CashoutStatus {
    #[sea_orm(string_value = "requested")]
    Requested,
    #[sea_orm(string_value = "approved")]
    Approved,
    #[sea_orm(string_value = "rejected")]
    Rejected,
    #[sea_orm(string_value = "paid")]
    Paid,
}

impl std::fmt::Display for CashoutStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            CashoutStatus::Requested => "requested",
            CashoutStatus::Approved => "approved",
            CashoutStatus::Rejected => "rejected",
            CashoutStatus::Paid => "paid",
        };
        write!(f, "{}", s)
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
        pub paypal_email: String,
        pub paypal_payout_batch_id: Option<String>,
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
