//! Library entry point for `nexus-tracker`.
//!
//! The crate ships as a binary (`nexus-trackerd`); this library face
//! exists so integration tests under `tests/` can drive the daemon's
//! components (TLS, connection task, handlers) directly. Apart from
//! re-exposing modules, the library has no behavior of its own.

pub mod args;
pub mod auth;
pub mod connection;
pub mod constants;
pub mod errors;
pub mod handlers;
pub mod i18n;
pub mod logging;
pub mod registry;
pub mod state;
pub mod tls;
