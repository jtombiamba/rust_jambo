// async-trait generates a redundant `#[must_use]` on `Pin<Box<dyn Future>>` returns.
#![allow(clippy::double_must_use)]

pub mod config;
pub mod error;
pub mod i18n;
pub mod observability;
pub mod password;
pub mod payment;
