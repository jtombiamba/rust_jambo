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
                    .as_enum(TopupTransactionKind::Enum)
                    .values(vec![
                        TopupTransactionKind::Topup,
                        TopupTransactionKind::Unfreeze,
                    ])
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(TopupTransactions::Table)
                    .if_not_exists()
                    .col(uuid(TopupTransactions::Id).primary_key())
                    .col(uuid(TopupTransactions::UserId))
                    .col(
                        ColumnDef::new(TopupTransactions::Kind)
                            .custom(TopupTransactionKind::Enum)
                            .not_null(),
                    )
                    .col(integer(TopupTransactions::AmountEurCents))
                    .col(integer(TopupTransactions::Credits))
                    .col(timestamp_with_time_zone(TopupTransactions::CreatedAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_topup_transactions_user_id")
                            .from(TopupTransactions::Table, TopupTransactions::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_topup_transactions_user_id_created_at")
                    .table(TopupTransactions::Table)
                    .col(TopupTransactions::UserId)
                    .col(TopupTransactions::CreatedAt)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx_topup_transactions_user_id_created_at")
                    .table(TopupTransactions::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .drop_table(Table::drop().table(TopupTransactions::Table).to_owned())
            .await?;

        manager
            .drop_type(Type::drop().name(TopupTransactionKind::Enum).to_owned())
            .await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum TopupTransactionKind {
    #[sea_orm(iden = "topup_transaction_kind")]
    Enum,
    #[sea_orm(iden = "topup")]
    Topup,
    #[sea_orm(iden = "unfreeze")]
    Unfreeze,
}

#[derive(DeriveIden)]
enum TopupTransactions {
    Table,
    Id,
    UserId,
    Kind,
    AmountEurCents,
    Credits,
    CreatedAt,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}
