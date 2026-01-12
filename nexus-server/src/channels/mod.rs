//! Channel management module for multi-channel chat

mod manager;
mod types;

pub use manager::ChannelManager;
pub use types::{Channel, JoinError};
