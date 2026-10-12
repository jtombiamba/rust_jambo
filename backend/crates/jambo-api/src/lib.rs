// async-trait generates a redundant `#[must_use]` on `Pin<Box<dyn Future>>` returns.
#![allow(clippy::double_must_use)]

pub mod api;
pub mod auth;
pub mod websocket;
pub mod worker;

#[cfg(test)]
pub mod test_helpers;
