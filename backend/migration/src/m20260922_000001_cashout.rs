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
                    .as_enum(CashoutStatusEnum::Enum)
                    .values(vec![
                        CashoutStatusEnum::Requested,
                        CashoutStatusEnum::Approved,
                        CashoutStatusEnum::Rejected,
                        CashoutStatusEnum::Paid,
                    ])
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(PlayerProfiles::Table)
                    .add_column_if_not_exists(
                        ColumnDef::new(PlayerProfiles::CashoutLocked)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(CashoutRequests::Table)
                    .if_not_exists()
                    .col(uuid(CashoutRequests::Id).primary_key())
                    .col(uuid(CashoutRequests::UserId))
                    .col(integer(CashoutRequests::Credits))
                    .col(integer(CashoutRequests::AmountEurCents))
                    .col(
                        ColumnDef::new(CashoutRequests::Status)
                            .custom(CashoutStatusEnum::Enum)
                            .not_null(),
                    )
                    .col(string(CashoutRequests::PaypalEmail))
                    .col(string(CashoutRequests::PaypalPayoutBatchId).null())
                    .col(text(CashoutRequests::AdminNote).null())
                    .col(string(CashoutRequests::ProcessedBy).null())
                    .col(timestamp_with_time_zone(CashoutRequests::CreatedAt))
                    .col(timestamp_with_time_zone(CashoutRequests::UpdatedAt))
                    .col(timestamp_with_time_zone(CashoutRequests::ProcessedAt).null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_cashout_requests_user_id")
                            .from(CashoutRequests::Table, CashoutRequests::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_cashout_requests_user_id")
                    .table(CashoutRequests::Table)
                    .col(CashoutRequests::UserId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_cashout_requests_status")
                    .table(CashoutRequests::Table)
                    .col(CashoutRequests::Status)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx_cashout_requests_status")
                    .table(CashoutRequests::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .drop_index(
                Index::drop()
                    .name("idx_cashout_requests_user_id")
                    .table(CashoutRequests::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .drop_table(Table::drop().table(CashoutRequests::Table).to_owned())
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(PlayerProfiles::Table)
                    .drop_column(PlayerProfiles::CashoutLocked)
                    .to_owned(),
            )
            .await?;

        manager
            .drop_type(Type::drop().name(CashoutStatusEnum::Enum).to_owned())
            .await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum CashoutStatusEnum {
    #[sea_orm(iden = "cashout_status")]
    Enum,
    #[sea_orm(iden = "requested")]
    Requested,
    #[sea_orm(iden = "approved")]
    Approved,
    #[sea_orm(iden = "rejected")]
    Rejected,
    #[sea_orm(iden = "paid")]
    Paid,
}

#[derive(DeriveIden)]
enum PlayerProfiles {
    Table,
    CashoutLocked,
}

#[derive(DeriveIden)]
enum CashoutRequests {
    Table,
    Id,
    UserId,
    Credits,
    AmountEurCents,
    Status,
    PaypalEmail,
    PaypalPayoutBatchId,
    AdminNote,
    ProcessedBy,
    CreatedAt,
    UpdatedAt,
    ProcessedAt,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}
