pub use sea_orm_migration::prelude::*;

mod m20220101_000001_create_table;
mod m20260506_000001_create_users;
mod m20260509_000001_multiplayer_game;
mod m20260509_000002_game_invites;
mod m20260514_000001_winning_streak;
mod m20260514_000002_unique_player_game_user;
mod m20260518_000001_freeze_system;
mod m20260523_000001_user_language;
mod m20260527_000001_game_staleness;
mod m20260527_000002_rooms;
mod m20260528_000001_fix_unique_index;
mod m20260528_000002_run_stall_tracking;
mod m20260707_000001_step_by_step;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20220101_000001_create_table::Migration),
            Box::new(m20260506_000001_create_users::Migration),
            Box::new(m20260509_000001_multiplayer_game::Migration),
            Box::new(m20260509_000002_game_invites::Migration),
            Box::new(m20260514_000001_winning_streak::Migration),
            Box::new(m20260514_000002_unique_player_game_user::Migration),
            Box::new(m20260518_000001_freeze_system::Migration),
            Box::new(m20260523_000001_user_language::Migration),
            Box::new(m20260527_000001_game_staleness::Migration),
            Box::new(m20260527_000002_rooms::Migration),
            Box::new(m20260528_000001_fix_unique_index::Migration),
            Box::new(m20260528_000002_run_stall_tracking::Migration),
            Box::new(m20260707_000001_step_by_step::Migration),
        ]
    }
}
