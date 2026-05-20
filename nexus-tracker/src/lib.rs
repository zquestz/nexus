//! Library face for the `nexus-trackerd` binary, exposing modules so
//! integration tests under `tests/` can drive the daemon's components
//! directly. No behavior of its own beyond re-exports.

pub mod args;
pub mod auth;
pub mod connection;
pub mod constants;
pub mod errors;
pub mod handlers;
pub mod i18n;
pub mod rate_limiter;
pub mod registry;
pub mod resolver;
pub mod state;
pub mod upnp;
pub mod websocket;
