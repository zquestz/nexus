//! User representation for logged-in users

use nexus_common::protocol::ServerMessage;
use std::net::SocketAddr;
use tokio::sync::mpsc;

/// Represents a logged-in user
#[derive(Debug, Clone)]
pub struct User {
    /// Unique user ID for this session
    pub id: u32,
    /// Database user ID
    pub db_user_id: i64,
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
    /// Features enabled for this user
    pub features: Vec<String>,
}

impl User {
    /// Create a new user
    pub fn new(
        id: u32,
        db_user_id: i64,
        username: String,
        session_id: String,
        address: SocketAddr,
        tx: mpsc::UnboundedSender<ServerMessage>,
        features: Vec<String>,
    ) -> Self {
        Self {
            id,
            db_user_id,
            username,
            session_id,
            address,
            login_time: current_timestamp(),
            tx,
            features,
        }
    }

    /// Check if user has a specific feature enabled
    pub fn has_feature(&self, feature: &str) -> bool {
        self.features.iter().any(|f| f == feature)
    }
}

/// Get current Unix timestamp in seconds
fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
