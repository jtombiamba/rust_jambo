use chrono::{DateTime, Utc};
use sea_orm::entity::prelude::*;
use sea_orm::DeriveActiveEnum;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

pub mod game {
    use super::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
    #[sea_orm(table_name = "games")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: Uuid,
        pub status: super::GameStatus,
        pub bet: i32,
        pub created_at: DateTime<Utc>,
        pub updated_at: DateTime<Utc>,
        pub finished_at: Option<DateTime<Utc>>,
        pub rank: Option<i32>,
        pub roll: i32,
        pub auto: bool,
        pub winner_id: Option<Uuid>,
        #[sea_orm(column_type = "JsonBinary")]
        pub player_positions: Value,
        pub current_winning_card: Option<i32>,
        pub current_winning_player_position: Option<i32>,
        pub creator_id: Option<Uuid>,
        pub game_mode: super::GameMode,
        pub max_players: i16,
        pub invite_expires_at: Option<DateTime<Utc>>,
        pub stall_warning_sent_at: Option<DateTime<Utc>>,
        pub game_run_id: Option<Uuid>,
        pub step_by_step: bool,
        #[sea_orm(column_type = "JsonBinary")]
        pub kicked_players: Value,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {
        #[sea_orm(
            belongs_to = "super::player::Entity",
            from = "Column::WinnerId",
            to = "super::player::Column::Id"
        )]
        Winner,
        #[sea_orm(
            belongs_to = "super::user::Entity",
            from = "Column::CreatorId",
            to = "super::user::Column::Id"
        )]
        Creator,
    }

    impl ActiveModelBehavior for ActiveModel {}
}

pub mod player {
    use super::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
    #[sea_orm(table_name = "players")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: Uuid,
        pub game_id: Uuid,
        pub player_type: super::PlayerType,
        pub name: String,
        pub position: i32,
        pub credits: i32,
        pub created_at: DateTime<Utc>,
        pub user_id: Option<Uuid>,
        pub kicked: bool,
        pub kicked_at: Option<DateTime<Utc>>,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {
        #[sea_orm(
            belongs_to = "super::user::Entity",
            from = "Column::UserId",
            to = "super::user::Column::Id"
        )]
        User,
        #[sea_orm(
            belongs_to = "super::game::Entity",
            from = "Column::GameId",
            to = "super::game::Column::Id"
        )]
        Game,
    }

    impl Related<super::user::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::User.def()
        }
    }

    impl Related<super::game::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::Game.def()
        }
    }

    impl ActiveModelBehavior for ActiveModel {}
}

pub mod game_card {
    use super::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
    #[sea_orm(table_name = "game_cards")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: Uuid,
        pub game_id: Uuid,
        pub player_id: Option<Uuid>,
        pub card_index: i32,
        pub played: bool,
        pub played_at: Option<DateTime<Utc>>,
        pub round: Option<i32>,
        pub created_at: DateTime<Utc>,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "Enum", enum_name = "player_type")]
pub enum PlayerType {
    #[sea_orm(string_value = "human")]
    Human,
    #[sea_orm(string_value = "bot")]
    Bot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "Enum", enum_name = "game_status")]
pub enum GameStatus {
    #[sea_orm(string_value = "pending")]
    Pending,
    #[sea_orm(string_value = "active")]
    Active,
    #[sea_orm(string_value = "finished")]
    Finished,
    #[sea_orm(string_value = "cancelled")]
    Cancelled,
    #[sea_orm(string_value = "kora")]
    Kora,
    #[sea_orm(string_value = "double_kora")]
    DoubleKora,
    #[sea_orm(string_value = "ready")]
    Ready,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "Enum", enum_name = "game_mode")]
pub enum GameMode {
    #[sea_orm(string_value = "solo")]
    Solo,
    #[sea_orm(string_value = "multiplayer")]
    Multiplayer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "Enum", enum_name = "invite_status")]
pub enum InviteStatus {
    #[sea_orm(string_value = "pending")]
    Pending,
    #[sea_orm(string_value = "accepted")]
    Accepted,
    #[sea_orm(string_value = "declined")]
    Declined,
}

impl std::fmt::Display for GameStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            GameStatus::Pending => "pending",
            GameStatus::Active => "active",
            GameStatus::Finished => "finished",
            GameStatus::Cancelled => "cancelled",
            GameStatus::Kora => "kora",
            GameStatus::DoubleKora => "double_kora",
            GameStatus::Ready => "ready",
        };
        write!(f, "{}", s)
    }
}

pub mod user {
    use super::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
    #[sea_orm(table_name = "users")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: Uuid,
        pub pseudo: String,
        pub email: String,
        pub password_hash: String,
        pub last_ip_hash: Option<String>,
        pub language: String,
        pub created_at: DateTime<Utc>,
        pub updated_at: DateTime<Utc>,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

pub mod player_profile {
    use super::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
    #[sea_orm(table_name = "player_profiles")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: Uuid,
        pub user_id: Uuid,
        pub player_type: super::PlayerType,
        pub credit: i32,
        pub game_played: i32,
        pub wins: i32,
        pub kora_wins: i32,
        pub winning_streak: i32,
        pub latitude: Option<f64>,
        pub longitude: Option<f64>,
        pub country_code: Option<String>,
        pub city: Option<String>,
        pub frozen_until: Option<DateTime<Utc>>,
        pub created_at: DateTime<Utc>,
        pub updated_at: DateTime<Utc>,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {
        #[sea_orm(
            belongs_to = "super::user::Entity",
            from = "Column::UserId",
            to = "super::user::Column::Id"
        )]
        User,
    }

    impl Related<super::user::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::User.def()
        }
    }

    impl ActiveModelBehavior for ActiveModel {}
}

pub mod game_invite {
    use super::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
    #[sea_orm(table_name = "game_invites")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: Uuid,
        pub game_id: Uuid,
        pub invited_user_id: Uuid,
        pub status: super::InviteStatus,
        pub created_at: DateTime<Utc>,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {
        #[sea_orm(
            belongs_to = "super::game::Entity",
            from = "Column::GameId",
            to = "super::game::Column::Id"
        )]
        Game,
        #[sea_orm(
            belongs_to = "super::user::Entity",
            from = "Column::InvitedUserId",
            to = "super::user::Column::Id"
        )]
        User,
    }

    impl Related<super::game::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::Game.def()
        }
    }

    impl Related<super::user::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::User.def()
        }
    }

    impl ActiveModelBehavior for ActiveModel {}
}

pub mod room {
    use super::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
    #[sea_orm(table_name = "rooms")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: Uuid,
        pub name: String,
        pub creator_id: Uuid,
        pub invitation_code: String,
        pub created_at: DateTime<Utc>,
        pub updated_at: DateTime<Utc>,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {
        #[sea_orm(
            belongs_to = "super::user::Entity",
            from = "Column::CreatorId",
            to = "super::user::Column::Id"
        )]
        Creator,
    }

    impl ActiveModelBehavior for ActiveModel {}
}

pub mod room_member {
    use super::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
    #[sea_orm(table_name = "room_members")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: Uuid,
        pub room_id: Uuid,
        pub user_id: Uuid,
        pub joined_at: DateTime<Utc>,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {
        #[sea_orm(
            belongs_to = "super::room::Entity",
            from = "Column::RoomId",
            to = "super::room::Column::Id"
        )]
        Room,
        #[sea_orm(
            belongs_to = "super::user::Entity",
            from = "Column::UserId",
            to = "super::user::Column::Id"
        )]
        User,
    }

    impl ActiveModelBehavior for ActiveModel {}
}

pub mod game_run {
    use super::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
    #[sea_orm(table_name = "game_runs")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: Uuid,
        pub room_id: Uuid,
        pub num_games: i32,
        pub bet_per_game: i32,
        pub num_players: i32,
        pub current_game_index: i32,
        pub status: String,
        pub created_by: Uuid,
        pub next_game_auto_start_at: Option<DateTime<Utc>>,
        pub stall_warning_sent_at: Option<DateTime<Utc>>,
        pub stall_cancelled_at: Option<DateTime<Utc>>,
        pub created_at: DateTime<Utc>,
        pub updated_at: DateTime<Utc>,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {
        #[sea_orm(
            belongs_to = "super::room::Entity",
            from = "Column::RoomId",
            to = "super::room::Column::Id"
        )]
        Room,
        #[sea_orm(
            belongs_to = "super::user::Entity",
            from = "Column::CreatedBy",
            to = "super::user::Column::Id"
        )]
        Creator,
    }

    impl ActiveModelBehavior for ActiveModel {}
}

pub mod game_run_player {
    use super::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
    #[sea_orm(table_name = "game_run_players")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: Uuid,
        pub game_run_id: Uuid,
        pub user_id: Uuid,
        pub position: i32,
        pub provisioned_credits: i32,
        pub kicked: bool,
        pub joined_at: DateTime<Utc>,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {
        #[sea_orm(
            belongs_to = "super::game_run::Entity",
            from = "Column::GameRunId",
            to = "super::game_run::Column::Id"
        )]
        GameRun,
        #[sea_orm(
            belongs_to = "super::user::Entity",
            from = "Column::UserId",
            to = "super::user::Column::Id"
        )]
        User,
    }

    impl ActiveModelBehavior for ActiveModel {}
}

pub mod game_run_game {
    use super::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
    #[sea_orm(table_name = "game_run_games")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: Uuid,
        pub game_run_id: Uuid,
        pub game_id: Uuid,
        pub game_index: i32,
        pub status: String,
        pub created_at: DateTime<Utc>,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {
        #[sea_orm(
            belongs_to = "super::game_run::Entity",
            from = "Column::GameRunId",
            to = "super::game_run::Column::Id"
        )]
        GameRun,
        #[sea_orm(
            belongs_to = "super::game::Entity",
            from = "Column::GameId",
            to = "super::game::Column::Id"
        )]
        Game,
    }

    impl ActiveModelBehavior for ActiveModel {}
}

pub mod game_run_event {
    use super::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
    #[sea_orm(table_name = "game_run_events")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: Uuid,
        pub game_run_id: Uuid,
        pub user_id: Option<Uuid>,
        pub event_type: String,
        pub data: Option<String>,
        pub created_at: DateTime<Utc>,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {
        #[sea_orm(
            belongs_to = "super::game_run::Entity",
            from = "Column::GameRunId",
            to = "super::game_run::Column::Id"
        )]
        GameRun,
    }

    impl ActiveModelBehavior for ActiveModel {}
}

// Re-export the Model types as the original names for convenience
pub use game::Model as Game;
pub use game_card::Model as GameCard;
pub use game_run::Model as GameRun;
pub use game_run_event::Model as GameRunEvent;
pub use game_run_game::Model as GameRunGame;
pub use game_run_player::Model as GameRunPlayer;
pub use player::Model as Player;
pub use player_profile::Model as PlayerProfile;
pub use room::Model as Room;
pub use room_member::Model as RoomMember;
pub use user::Model as User;
