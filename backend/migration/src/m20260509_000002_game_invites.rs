use sea_orm_migration::prelude::{sea_query::extension::postgres::Type, *};
use sea_orm_migration::schema::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_type(
                Type::alter()
                    .name(GameStatusEnum::Enum)
                    .add_value(GameStatusEnum::Ready)
                    .to_owned(),
            )
            .await?;

        manager
            .create_type(
                Type::create()
                    .as_enum(InviteStatusEnum::Enum)
                    .values(vec![
                        InviteStatusEnum::Pending,
                        InviteStatusEnum::Accepted,
                        InviteStatusEnum::Declined,
                    ])
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(GameInvites::Table)
                    .if_not_exists()
                    .col(uuid(GameInvites::Id).primary_key())
                    .col(uuid(GameInvites::GameId))
                    .col(uuid(GameInvites::InvitedUserId))
                    .col(
                        ColumnDef::new(GameInvites::Status)
                            .custom(InviteStatusEnum::Enum)
                            .not_null()
                            .default("pending"),
                    )
                    .col(timestamp_with_time_zone(GameInvites::CreatedAt))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_game_invites_game_id")
                            .from(GameInvites::Table, GameInvites::GameId)
                            .to(Games::Table, Games::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_game_invites_user_id")
                            .from(GameInvites::Table, GameInvites::InvitedUserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_game_invites_game_id")
                    .table(GameInvites::Table)
                    .col(GameInvites::GameId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_game_invites_user_id_status")
                    .table(GameInvites::Table)
                    .col(GameInvites::InvitedUserId)
                    .col(GameInvites::Status)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx_game_invites_user_id_status")
                    .table(GameInvites::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .drop_index(
                Index::drop()
                    .name("idx_game_invites_game_id")
                    .table(GameInvites::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .drop_table(Table::drop().table(GameInvites::Table).to_owned())
            .await?;

        manager
            .drop_type(
                Type::drop()
                    .if_exists()
                    .name(InviteStatusEnum::Enum)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum GameInvites {
    Table,
    Id,
    GameId,
    InvitedUserId,
    Status,
    CreatedAt,
}

#[derive(DeriveIden)]
enum Games {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum GameStatusEnum {
    #[sea_orm(iden = "game_status")]
    Enum,
    #[sea_orm(iden = "ready")]
    Ready,
}

#[derive(DeriveIden)]
enum InviteStatusEnum {
    #[sea_orm(iden = "invite_status")]
    Enum,
    #[sea_orm(iden = "pending")]
    Pending,
    #[sea_orm(iden = "accepted")]
    Accepted,
    #[sea_orm(iden = "declined")]
    Declined,
}
