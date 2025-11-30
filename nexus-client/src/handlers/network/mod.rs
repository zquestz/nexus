//! Network event handlers
//!
//! This module handles all network-related events including:
//! - Connection results (manual and bookmark connections)
//! - Message routing
//! - Server message handling
//! - Certificate fingerprint verification

mod chat;
mod connection;
mod fingerprint;
mod messages;

pub mod constants;
pub mod helpers;