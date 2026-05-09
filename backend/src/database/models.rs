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
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {
        #[sea_orm(
            belongs_to = "super::player::Entity",
            from = "Column::WinnerId",
            to = "super::player::Column::Id"
        )]
        Winner,
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

pub mod round {
    use super::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
    #[sea_orm(table_name = "rounds")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: Uuid,
        pub game_id: Uuid,
        pub round_number: i32,
        pub winner_position: Option<i32>,
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
        pub latitude: Option<f64>,
        pub longitude: Option<f64>,
        pub country_code: Option<String>,
        pub city: Option<String>,
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

// Re-export the Model types as the original names for convenience
pub use game::Model as Game;
pub use game_card::Model as GameCard;
pub use player::Model as Player;
pub use player_profile::Model as PlayerProfile;
#[allow(unused_imports)]
pub use round::Model as Round;
pub use user::Model as User;
