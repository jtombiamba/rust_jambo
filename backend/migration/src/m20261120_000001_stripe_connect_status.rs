use sea_orm_migration::prelude::{sea_query::extension::postgres::Type, *};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_type(
                Type::create()
                    .as_enum(ConnectAccountStatusEnum::Enum)
                    .values(vec![
                        ConnectAccountStatusEnum::NotCreated,
                        ConnectAccountStatusEnum::OnboardingStarted,
                        ConnectAccountStatusEnum::Pending,
                        ConnectAccountStatusEnum::Verified,
                        ConnectAccountStatusEnum::VerificationFailed,
                        ConnectAccountStatusEnum::Restricted,
                        ConnectAccountStatusEnum::Disabled,
                        ConnectAccountStatusEnum::PaymentPaused,
                    ])
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Users::Table)
                    .add_column_if_not_exists(
                        ColumnDef::new(Users::StripeConnectAccountStatus)
                            .custom(ConnectAccountStatusEnum::Enum)
                            .not_null()
                            .default("not_created"),
                    )
                    .to_owned(),
            )
            .await?;

        // Extend the existing cashout_status enum with the intermediate
        // "processing" value (transfer submitted, awaiting webhook confirmation).
        manager
            .alter_type(
                Type::alter()
                    .name(CashoutStatusEnum::Enum)
                    .add_value(CashoutStatusEnum::Processing)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Users::Table)
                    .drop_column(Users::StripeConnectAccountStatus)
                    .to_owned(),
            )
            .await?;

        manager
            .drop_type(Type::drop().name(ConnectAccountStatusEnum::Enum).to_owned())
            .await?;

        // NOTE: Postgres does not support removing an enum value, so the
        // "processing" value added to `cashout_status` is intentionally left in
        // place on rollback.
        Ok(())
    }
}

#[derive(DeriveIden)]
enum ConnectAccountStatusEnum {
    #[sea_orm(iden = "connect_account_status")]
    Enum,
    #[sea_orm(iden = "not_created")]
    NotCreated,
    #[sea_orm(iden = "onboarding_started")]
    OnboardingStarted,
    #[sea_orm(iden = "pending")]
    Pending,
    #[sea_orm(iden = "verified")]
    Verified,
    #[sea_orm(iden = "verification_failed")]
    VerificationFailed,
    #[sea_orm(iden = "restricted")]
    Restricted,
    #[sea_orm(iden = "disabled")]
    Disabled,
    #[sea_orm(iden = "payment_paused")]
    PaymentPaused,
}

#[derive(DeriveIden)]
enum CashoutStatusEnum {
    #[sea_orm(iden = "cashout_status")]
    Enum,
    #[sea_orm(iden = "processing")]
    Processing,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    StripeConnectAccountStatus,
}
