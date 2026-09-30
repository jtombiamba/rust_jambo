use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(
    rs_type = "String",
    db_type = "Enum",
    enum_name = "topup_transaction_kind"
)]
pub enum TopupTransactionKind {
    #[sea_orm(string_value = "topup")]
    Topup,
    #[sea_orm(string_value = "unfreeze")]
    Unfreeze,
}

impl std::fmt::Display for TopupTransactionKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            TopupTransactionKind::Topup => "topup",
            TopupTransactionKind::Unfreeze => "unfreeze",
        };
        write!(f, "{}", s)
    }
}

pub mod topup_transaction {
    use super::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
    #[sea_orm(table_name = "topup_transactions")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: Uuid,
        pub user_id: Uuid,
        pub kind: super::TopupTransactionKind,
        pub amount_eur_cents: i32,
        pub credits: i32,
        pub order_id: Option<String>,
        pub created_at: DateTime<Utc>,
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

#[cfg(test)]
mod tests {
    use super::TopupTransactionKind;

    #[test]
    fn kind_display_matches_enum_values() {
        assert_eq!(TopupTransactionKind::Topup.to_string(), "topup");
        assert_eq!(TopupTransactionKind::Unfreeze.to_string(), "unfreeze");
    }
}
