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
                    .as_enum(GameModeEnum::Enum)
                    .values(vec![GameModeEnum::Solo, GameModeEnum::Multiplayer])
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Games::Table)
                    .add_column_if_not_exists(uuid_null(Games::CreatorId))
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Games::Table)
                    .add_column_if_not_exists(
                        ColumnDef::new(Games::GameMode)
                            .custom(GameModeEnum::Enum)
                            .not_null()
                            .default("solo"),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Games::Table)
                    .add_column_if_not_exists(small_integer(Games::MaxPlayers).default(4))
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Games::Table)
                    .add_column_if_not_exists(timestamp_with_time_zone_null(Games::InviteExpiresAt))
                    .to_owned(),
            )
            .await?;

        manager
            .create_foreign_key(
                ForeignKey::create()
                    .name("fk_games_creator_id")
                    .from(Games::Table, Games::CreatorId)
                    .to(Users::Table, Users::Id)
                    .on_delete(ForeignKeyAction::SetNull)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_games_creator_id")
                    .table(Games::Table)
                    .col(Games::CreatorId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_games_status_invite_expires")
                    .table(Games::Table)
                    .col(Games::Status)
                    .col(Games::InviteExpiresAt)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx_games_status_invite_expires")
                    .table(Games::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .drop_index(
                Index::drop()
                    .name("idx_games_creator_id")
                    .table(Games::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .drop_foreign_key(
                ForeignKey::drop()
                    .table(Games::Table)
                    .name("fk_games_creator_id")
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Games::Table)
                    .drop_column(Games::InviteExpiresAt)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Games::Table)
                    .drop_column(Games::MaxPlayers)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Games::Table)
                    .drop_column(Games::GameMode)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Games::Table)
                    .drop_column(Games::CreatorId)
                    .to_owned(),
            )
            .await?;

        manager
            .drop_type(Type::drop().if_exists().name(GameModeEnum::Enum).to_owned())
            .await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum Games {
    Table,
    CreatorId,
    GameMode,
    MaxPlayers,
    InviteExpiresAt,
    Status,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum GameModeEnum {
    #[sea_orm(iden = "game_mode")]
    Enum,
    #[sea_orm(iden = "solo")]
    Solo,
    #[sea_orm(iden = "multiplayer")]
    Multiplayer,
}
