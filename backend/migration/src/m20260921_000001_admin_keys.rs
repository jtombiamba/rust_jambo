use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(AdminKeys::Table)
                    .if_not_exists()
                    .col(uuid(AdminKeys::Id).primary_key())
                    .col(string(AdminKeys::Label).unique_key())
                    .col(string(AdminKeys::KeyHash))
                    .col(timestamp_with_time_zone(AdminKeys::CreatedAt))
                    .col(timestamp_with_time_zone(AdminKeys::RevokedAt).null())
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(AdminKeys::Table).to_owned())
            .await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum AdminKeys {
    Table,
    Id,
    Label,
    KeyHash,
    CreatedAt,
    RevokedAt,
}
