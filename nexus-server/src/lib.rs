//! Nexus BBS Server Library
//!
//! The whole server lives in this library; the `nexusd` binary is a thin
//! shell over it, and integration tests consume it directly. Keeping every
//! module here (rather than re-declared in `main.rs`) means the crate — and
//! its unit tests — compile exactly once.

pub mod args;
pub mod channels;
pub mod connection;
mod connection_io;
pub mod connection_tracker;
pub mod constants;
pub mod db;
pub mod egress;
pub mod files;
pub mod flood;
pub mod handlers;
pub mod i18n;
pub mod ip_rule_cache;
pub mod paths;
pub mod scheduler;
pub mod tracker;
pub mod transfers;
pub mod upnp;
pub mod users;
pub mod voice;
pub mod websocket;
