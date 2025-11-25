//! User representation for logged-in users

use nexus_common::protocol::ServerMessage;
use std::net::SocketAddr;
use tokio::sync::mpsc;

/// Represents a logged-in user
#[derive(Debug, Clone)]
pub struct User {
    /// Session ID (unique identifier for this connection)
    pub session_id: u32,
    /// Database user ID
    pub db_user_id: i64,
    /// Username
    pub username: String,
    /// Remote address of the user's connection
    pub address: SocketAddr,
    /// When the user account was created (Unix timestamp from database)
    #[allow(dead_code)]
    pub created_at: i64,
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
        session_id: u32,
        db_user_id: i64,
        username: String,
        address: SocketAddr,
        created_at: i64,
        tx: mpsc::UnboundedSender<ServerMessage>,
        features: Vec<String>,
    ) -> Self {
        Self {
            session_id,
            db_user_id,
            username,
            address,
            created_at,
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
