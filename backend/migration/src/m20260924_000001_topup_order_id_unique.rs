use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(TopupTransactions::Table)
                    .add_column_if_not_exists(
                        ColumnDef::new(TopupTransactions::OrderId).string().null(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("uq_topup_transactions_order_id")
                    .table(TopupTransactions::Table)
                    .col(TopupTransactions::OrderId)
                    .unique()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("uq_topup_transactions_order_id")
                    .table(TopupTransactions::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(TopupTransactions::Table)
                    .drop_column(TopupTransactions::OrderId)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum TopupTransactions {
    Table,
    OrderId,
}
