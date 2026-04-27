//! Network event handlers
//!
//! This module handles all network-related events including:
//! - Connection results (manual and bookmark connections)
//! - Message routing
//! - Server message handling

mod chat;
mod connection;
mod messages;

pub mod constants;
pub mod helpers;

// Re-export connection types for use by URI handler
pub use connection::{ConnectionContext, ConnectionSource};
