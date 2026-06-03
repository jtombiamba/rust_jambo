use sea_orm_migration::prelude::{sea_query::extension::postgres::Type, *};
use sea_orm_migration::schema::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Step 1: Create custom enum types
        manager
            .create_type(
                Type::create()
                    .as_enum(PlayerTypeEnum::Enum)
                    .values(vec![PlayerTypeEnum::Human, PlayerTypeEnum::Bot])
                    .to_owned(),
            )
            .await?;

        manager
            .create_type(
                Type::create()
                    .as_enum(GameStatusEnum::Enum)
                    .values(vec![
                        GameStatusEnum::Pending,
                        GameStatusEnum::Active,
                        GameStatusEnum::Finished,
                        GameStatusEnum::Cancelled,
                        GameStatusEnum::Kora,
                        GameStatusEnum::DoubleKora,
                    ])
                    .to_owned(),
            )
            .await?;

        // Step 2: Create players table (without FK to games yet, to avoid circular dependency)
        manager
            .create_table(
                Table::create()
                    .table(Players::Table)
                    .if_not_exists()
                    .col(uuid(Players::Id).primary_key())
                    .col(uuid(Players::GameId))
                    .col(
                        ColumnDef::new(Players::PlayerType)
                            .custom(PlayerTypeEnum::Enum)
                            .not_null(),
                    )
                    .col(string(Players::Name))
                    .col(integer(Players::Position))
                    .col(integer(Players::Credits).default(500))
                    .col(timestamp_with_time_zone(Players::CreatedAt))
                    .to_owned(),
            )
            .await?;

        // Step 3: Create games table (with FK to players for winner_id)
        manager
            .create_table(
                Table::create()
                    .table(Games::Table)
                    .if_not_exists()
                    .col(uuid(Games::Id).primary_key())
                    .col(
                        ColumnDef::new(Games::Status)
                            .custom(GameStatusEnum::Enum)
                            .not_null()
                            .default("pending"),
                    )
                    .col(integer(Games::Bet))
                    .col(timestamp_with_time_zone(Games::CreatedAt))
                    .col(timestamp_with_time_zone(Games::UpdatedAt))
                    .col(timestamp_with_time_zone(Games::FinishedAt).null())
                    .col(integer(Games::Rank).null())
                    .col(integer(Games::Roll).default(1))
                    .col(boolean(Games::Auto).default(false))
                    .col(uuid(Games::WinnerId).null())
                    .col(json_binary(Games::PlayerPositions).default("{}"))
                    .col(integer(Games::CurrentWinningCard).null())
                    .col(integer(Games::CurrentWinningPlayerPosition).null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_games_winner_id")
                            .from(Games::Table, Games::WinnerId)
                            .to(Players::Table, Players::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await?;

        // Step 4: Add FK from players.game_id to games.id (now that games table exists)
        manager
            .create_foreign_key(
                ForeignKey::create()
                    .name("fk_players_game_id")
                    .from(Players::Table, Players::GameId)
                    .to(Games::Table, Games::Id)
                    .on_delete(ForeignKeyAction::Cascade)
                    .to_owned(),
            )
            .await?;

        // Step 5: Create game_cards table
        manager
            .create_table(
                Table::create()
                    .table(GameCards::Table)
                    .if_not_exists()
                    .col(uuid(GameCards::Id).primary_key())
                    .col(uuid(GameCards::GameId))
                    .col(uuid(GameCards::PlayerId).null())
                    .col(integer(GameCards::CardIndex))
                    .col(boolean(GameCards::Played).default(false))
                    .col(timestamp_with_time_zone(GameCards::PlayedAt).null())
                    .col(integer(GameCards::Round).null())
                    .col(timestamp_with_time_zone(GameCards::CreatedAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_game_cards_game_id")
                            .from(GameCards::Table, GameCards::GameId)
                            .to(Games::Table, Games::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_game_cards_player_id")
                            .from(GameCards::Table, GameCards::PlayerId)
                            .to(Players::Table, Players::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await?;

        // Step 7: Create indexes for common query patterns
        manager
            .create_index(
                Index::create()
                    .name("idx_players_game_id")
                    .table(Players::Table)
                    .col(Players::GameId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_game_cards_game_round")
                    .table(GameCards::Table)
                    .col(GameCards::GameId)
                    .col(GameCards::Round)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_game_cards_player_id")
                    .table(GameCards::Table)
                    .col(GameCards::PlayerId)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx_game_cards_player_id")
                    .table(GameCards::Table)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_index(
                Index::drop()
                    .name("idx_game_cards_game_round")
                    .table(GameCards::Table)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_index(
                Index::drop()
                    .name("idx_players_game_id")
                    .table(Players::Table)
                    .to_owned(),
            )
            .await?;

        // Drop tables in reverse order (respecting FK constraints)
        manager
            .drop_table(Table::drop().table(GameCards::Table).to_owned())
            .await?;

        // Drop FK on players before dropping games
        manager
            .drop_foreign_key(
                ForeignKey::drop()
                    .table(Players::Table)
                    .name("fk_players_game_id")
                    .to_owned(),
            )
            .await?;

        manager
            .drop_table(Table::drop().table(Games::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Players::Table).to_owned())
            .await?;

        // Drop custom enum types
        manager
            .drop_type(
                Type::drop()
                    .if_exists()
                    .name(GameStatusEnum::Enum)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_type(
                Type::drop()
                    .if_exists()
                    .name(PlayerTypeEnum::Enum)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}

// ──── Identifier definitions ────

#[derive(DeriveIden)]
enum Players {
    Table,
    Id,
    GameId,
    PlayerType,
    Name,
    Position,
    Credits,
    CreatedAt,
}

#[derive(DeriveIden)]
enum Games {
    Table,
    Id,
    Status,
    Bet,
    CreatedAt,
    UpdatedAt,
    FinishedAt,
    Rank,
    Roll,
    Auto,
    WinnerId,
    PlayerPositions,
    CurrentWinningCard,
    CurrentWinningPlayerPosition,
}

#[derive(DeriveIden)]
enum GameCards {
    Table,
    Id,
    GameId,
    PlayerId,
    CardIndex,
    Played,
    PlayedAt,
    Round,
    CreatedAt,
}

#[derive(DeriveIden)]
pub enum PlayerTypeEnum {
    #[sea_orm(iden = "player_type")]
    Enum,
    #[sea_orm(iden = "human")]
    Human,
    #[sea_orm(iden = "bot")]
    Bot,
}

#[derive(DeriveIden)]
enum GameStatusEnum {
    #[sea_orm(iden = "game_status")]
    Enum,
    #[sea_orm(iden = "pending")]
    Pending,
    #[sea_orm(iden = "active")]
    Active,
    #[sea_orm(iden = "finished")]
    Finished,
    #[sea_orm(iden = "cancelled")]
    Cancelled,
    #[sea_orm(iden = "kora")]
    Kora,
    #[sea_orm(iden = "double_kora")]
    DoubleKora,
}
