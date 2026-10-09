// async-trait generates a redundant `#[must_use]` on `Pin<Box<dyn Future>>` returns.
#![allow(clippy::double_must_use)]

pub mod cache;
pub mod game;
pub mod messaging;
pub mod room;
pub mod scheduler;
