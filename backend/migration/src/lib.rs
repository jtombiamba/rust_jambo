pub use sea_orm_migration::prelude::*;

mod m20220101_000001_create_table;
mod m20260506_000001_create_users;
mod m20260509_000001_multiplayer_game;
mod m20260509_000002_game_invites;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20220101_000001_create_table::Migration),
            Box::new(m20260506_000001_create_users::Migration),
            Box::new(m20260509_000001_multiplayer_game::Migration),
            Box::new(m20260509_000002_game_invites::Migration),
        ]
    }
}
