//! User representation for logged-in users

use std::net::SocketAddr;
use tokio::sync::mpsc;
use nexus_common::protocol::ServerMessage;

/// Represents a logged-in user
#[derive(Debug, Clone)]
pub struct User {
    /// Unique user ID for this session
    pub id: u32,
    /// Username
    pub username: String,
    /// Session ID
    pub session_id: String,
    /// Remote address of the user's connection
    pub address: SocketAddr,
    /// When the user logged in (Unix timestamp)
    pub login_time: u64,
    /// Channel sender for sending messages to this user
    pub tx: mpsc::UnboundedSender<ServerMessage>,
}

impl User {
    /// Create a new user
    pub fn new(
        id: u32,
        username: String,
        session_id: String,
        address: SocketAddr,
        tx: mpsc::UnboundedSender<ServerMessage>,
    ) -> Self {
        Self {
            id,
            username,
            session_id,
            address,
            login_time: current_timestamp(),
            tx,
        }
    }
}

/// Get current Unix timestamp in seconds
fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
