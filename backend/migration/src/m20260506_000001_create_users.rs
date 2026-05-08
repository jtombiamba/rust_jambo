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
                    .table(Users::Table)
                    .if_not_exists()
                    .col(uuid(Users::Id).primary_key())
                    .col(string_uniq(Users::Pseudo))
                    .col(string_uniq(Users::Email))
                    .col(string(Users::PasswordHash))
                    .col(string_null(Users::LastIpHash))
                    .col(timestamp_with_time_zone(Users::CreatedAt))
                    .col(timestamp_with_time_zone(Users::UpdatedAt))
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(PlayerProfiles::Table)
                    .if_not_exists()
                    .col(uuid(PlayerProfiles::Id).primary_key())
                    .col(uuid(PlayerProfiles::UserId))
                    .col(
                        ColumnDef::new(PlayerProfiles::PlayerType)
                            .custom(PlayerTypeEnum::Enum)
                            .not_null()
                            .default("human"),
                    )
                    .col(integer(PlayerProfiles::Credit).default(500))
                    .col(integer(PlayerProfiles::GamePlayed).default(0))
                    .col(integer(PlayerProfiles::Wins).default(0))
                    .col(integer(PlayerProfiles::KoraWins).default(0))
                    .col(double_null(PlayerProfiles::Latitude))
                    .col(double_null(PlayerProfiles::Longitude))
                    .col(string_null(PlayerProfiles::CountryCode))
                    .col(string_null(PlayerProfiles::City))
                    .col(timestamp_with_time_zone(PlayerProfiles::CreatedAt))
                    .col(timestamp_with_time_zone(PlayerProfiles::UpdatedAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_player_profiles_user_id")
                            .from(PlayerProfiles::Table, PlayerProfiles::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("uq_player_profiles_user_id")
                    .table(PlayerProfiles::Table)
                    .col(PlayerProfiles::UserId)
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Players::Table)
                    .add_column_if_not_exists(uuid_null(Players::UserId))
                    .to_owned(),
            )
            .await?;

        manager
            .create_foreign_key(
                ForeignKey::create()
                    .name("fk_players_user_id")
                    .from(Players::Table, Players::UserId)
                    .to(Users::Table, Users::Id)
                    .on_delete(ForeignKeyAction::SetNull)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_players_user_id")
                    .table(Players::Table)
                    .col(Players::UserId)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx_players_user_id")
                    .table(Players::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .drop_foreign_key(
                ForeignKey::drop()
                    .table(Players::Table)
                    .name("fk_players_user_id")
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Players::Table)
                    .drop_column(Players::UserId)
                    .to_owned(),
            )
            .await?;

        manager
            .drop_index(
                Index::drop()
                    .name("uq_player_profiles_user_id")
                    .table(PlayerProfiles::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .drop_table(Table::drop().table(PlayerProfiles::Table).to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(Users::Table).to_owned())
            .await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
    Pseudo,
    Email,
    PasswordHash,
    LastIpHash,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum PlayerProfiles {
    Table,
    Id,
    UserId,
    PlayerType,
    Credit,
    GamePlayed,
    Wins,
    KoraWins,
    Latitude,
    Longitude,
    CountryCode,
    City,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum Players {
    Table,
    UserId,
}

#[derive(DeriveIden)]
enum PlayerTypeEnum {
    #[sea_orm(iden = "player_type")]
    Enum,
}
