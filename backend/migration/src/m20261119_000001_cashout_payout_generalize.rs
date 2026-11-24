use sea_orm_migration::prelude::{sea_query::extension::postgres::Type, *};
use sea_orm_migration::schema::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_type(
                Type::create()
                    .as_enum(PayoutMethodEnum::Enum)
                    .values(vec![
                        PayoutMethodEnum::PaypalEmail,
                        PayoutMethodEnum::StripeConnect,
                    ])
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(CashoutRequests::Table)
                    .rename_column(Alias::new("paypal_email"), Alias::new("payout_destination"))
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(CashoutRequests::Table)
                    .rename_column(
                        Alias::new("paypal_payout_batch_id"),
                        Alias::new("payout_reference"),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(CashoutRequests::Table)
                    .add_column(
                        ColumnDef::new(CashoutRequests::PayoutMethod)
                            .custom(PayoutMethodEnum::Enum)
                            .not_null()
                            .default("paypal_email"),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Users::Table)
                    .add_column_if_not_exists(string_null(Users::StripeConnectAccountId))
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
                    .drop_column(Users::StripeConnectAccountId)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(CashoutRequests::Table)
                    .drop_column(CashoutRequests::PayoutMethod)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(CashoutRequests::Table)
                    .rename_column(Alias::new("payout_destination"), Alias::new("paypal_email"))
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(CashoutRequests::Table)
                    .rename_column(
                        Alias::new("payout_reference"),
                        Alias::new("paypal_payout_batch_id"),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .drop_type(Type::drop().name(PayoutMethodEnum::Enum).to_owned())
            .await?;

        Ok(())
    }

    fn use_transaction(&self) -> Option<bool> {
        Some(true)
    }
}

#[derive(DeriveIden)]
enum PayoutMethodEnum {
    #[sea_orm(iden = "payout_method")]
    Enum,
    #[sea_orm(iden = "paypal_email")]
    PaypalEmail,
    #[sea_orm(iden = "stripe_connect")]
    StripeConnect,
}

#[derive(DeriveIden)]
enum CashoutRequests {
    Table,
    PayoutMethod,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    StripeConnectAccountId,
}
