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
                    .table(Rooms::Table)
                    .if_not_exists()
                    .col(uuid(Rooms::Id).primary_key())
                    .col(string(Rooms::Name))
                    .col(uuid(Rooms::CreatorId))
                    .col(string(Rooms::InvitationCode).unique_key())
                    .col(timestamp_with_time_zone(Rooms::CreatedAt))
                    .col(timestamp_with_time_zone(Rooms::UpdatedAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_rooms_creator_id")
                            .from(Rooms::Table, Rooms::CreatorId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(RoomMembers::Table)
                    .if_not_exists()
                    .col(uuid(RoomMembers::Id).primary_key())
                    .col(uuid(RoomMembers::RoomId))
                    .col(uuid(RoomMembers::UserId))
                    .col(timestamp_with_time_zone(RoomMembers::JoinedAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_room_members_room_id")
                            .from(RoomMembers::Table, RoomMembers::RoomId)
                            .to(Rooms::Table, Rooms::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_room_members_user_id")
                            .from(RoomMembers::Table, RoomMembers::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("uq_room_members_room_user")
                    .table(RoomMembers::Table)
                    .col(RoomMembers::RoomId)
                    .col(RoomMembers::UserId)
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(GameRuns::Table)
                    .if_not_exists()
                    .col(uuid(GameRuns::Id).primary_key())
                    .col(uuid(GameRuns::RoomId))
                    .col(integer(GameRuns::NumGames))
                    .col(integer(GameRuns::BetPerGame))
                    .col(integer(GameRuns::NumPlayers))
                    .col(integer(GameRuns::CurrentGameIndex).default(0))
                    .col(string(GameRuns::Status).default("active"))
                    .col(uuid(GameRuns::CreatedBy))
                    .col(timestamp_with_time_zone(GameRuns::NextGameAutoStartAt).null())
                    .col(timestamp_with_time_zone(GameRuns::CreatedAt))
                    .col(timestamp_with_time_zone(GameRuns::UpdatedAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_game_runs_room_id")
                            .from(GameRuns::Table, GameRuns::RoomId)
                            .to(Rooms::Table, Rooms::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_game_runs_created_by")
                            .from(GameRuns::Table, GameRuns::CreatedBy)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("uq_room_active_run")
                    .table(GameRuns::Table)
                    .col(GameRuns::RoomId)
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(GameRunPlayers::Table)
                    .if_not_exists()
                    .col(uuid(GameRunPlayers::Id).primary_key())
                    .col(uuid(GameRunPlayers::GameRunId))
                    .col(uuid(GameRunPlayers::UserId))
                    .col(integer(GameRunPlayers::Position))
                    .col(integer(GameRunPlayers::ProvisionedCredits))
                    .col(boolean(GameRunPlayers::Kicked).default(false))
                    .col(timestamp_with_time_zone(GameRunPlayers::JoinedAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_game_run_players_run_id")
                            .from(GameRunPlayers::Table, GameRunPlayers::GameRunId)
                            .to(GameRuns::Table, GameRuns::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_game_run_players_user_id")
                            .from(GameRunPlayers::Table, GameRunPlayers::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("uq_game_run_players_run_user")
                    .table(GameRunPlayers::Table)
                    .col(GameRunPlayers::GameRunId)
                    .col(GameRunPlayers::UserId)
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(GameRunGames::Table)
                    .if_not_exists()
                    .col(uuid(GameRunGames::Id).primary_key())
                    .col(uuid(GameRunGames::GameRunId))
                    .col(uuid(GameRunGames::GameId))
                    .col(integer(GameRunGames::GameIndex))
                    .col(string(GameRunGames::Status).default("pending"))
                    .col(timestamp_with_time_zone(GameRunGames::CreatedAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_game_run_games_run_id")
                            .from(GameRunGames::Table, GameRunGames::GameRunId)
                            .to(GameRuns::Table, GameRuns::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_game_run_games_game_id")
                            .from(GameRunGames::Table, GameRunGames::GameId)
                            .to(Games::Table, Games::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("uq_game_run_games_run_index")
                    .table(GameRunGames::Table)
                    .col(GameRunGames::GameRunId)
                    .col(GameRunGames::GameIndex)
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(GameRunEvents::Table)
                    .if_not_exists()
                    .col(uuid(GameRunEvents::Id).primary_key())
                    .col(uuid(GameRunEvents::GameRunId))
                    .col(uuid(GameRunEvents::UserId).null())
                    .col(string(GameRunEvents::EventType))
                    .col(text(GameRunEvents::Data).null())
                    .col(timestamp_with_time_zone(GameRunEvents::CreatedAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_game_run_events_run_id")
                            .from(GameRunEvents::Table, GameRunEvents::GameRunId)
                            .to(GameRuns::Table, GameRuns::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_game_run_events_run_id")
                    .table(GameRunEvents::Table)
                    .col(GameRunEvents::GameRunId)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Games::Table)
                    .add_column(ColumnDef::new(Games::GameRunId).uuid().null())
                    .add_column(
                        ColumnDef::new(Games::KickedPlayers)
                            .json_binary()
                            .default("[]"),
                    )
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Games::Table)
                    .drop_column(Games::GameRunId)
                    .drop_column(Games::KickedPlayers)
                    .to_owned(),
            )
            .await?;

        manager
            .drop_index(
                Index::drop()
                    .name("idx_game_run_events_run_id")
                    .table(GameRunEvents::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .drop_table(Table::drop().table(GameRunEvents::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(GameRunGames::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(GameRunPlayers::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(GameRuns::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(RoomMembers::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Rooms::Table).to_owned())
            .await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum Rooms {
    Table,
    Id,
    Name,
    CreatorId,
    InvitationCode,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum RoomMembers {
    Table,
    Id,
    RoomId,
    UserId,
    JoinedAt,
}

#[derive(DeriveIden)]
enum GameRuns {
    Table,
    Id,
    RoomId,
    NumGames,
    BetPerGame,
    NumPlayers,
    CurrentGameIndex,
    Status,
    CreatedBy,
    NextGameAutoStartAt,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum GameRunPlayers {
    Table,
    Id,
    GameRunId,
    UserId,
    Position,
    ProvisionedCredits,
    Kicked,
    JoinedAt,
}

#[derive(DeriveIden)]
enum GameRunGames {
    Table,
    Id,
    GameRunId,
    GameId,
    GameIndex,
    Status,
    CreatedAt,
}

#[derive(DeriveIden)]
enum GameRunEvents {
    Table,
    Id,
    GameRunId,
    UserId,
    EventType,
    Data,
    CreatedAt,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Games {
    Table,
    Id,
    GameRunId,
    KickedPlayers,
}
