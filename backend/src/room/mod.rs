pub mod error;
pub mod event_publisher;
pub mod service;
pub mod service_trait;
pub mod start_game_lock;
pub mod start_next_game;
mod trait_impl;
pub mod transaction_runner;

pub use service::RoomService;
pub use service_trait::RoomServiceTrait;

pub mod games;
pub mod runs;
pub mod stall;

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
