//! User representation for logged-in users

use nexus_common::protocol::ServerMessage;
use std::net::SocketAddr;
use tokio::sync::mpsc;

/// Parameters for creating a new user
pub struct NewUserParams {
    pub session_id: u32,
    pub db_user_id: i64,
    pub username: String,
    pub address: SocketAddr,
    pub created_at: i64,
    pub tx: mpsc::UnboundedSender<ServerMessage>,
    pub features: Vec<String>,
    pub locale: String,
}

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
    ///
    /// This field is stored for potential future features like account age display,
    /// statistics, or audit logging.
    #[allow(dead_code)]
    pub created_at: i64,
    /// When the user logged in (Unix timestamp)
    pub login_time: u64,
    /// Channel sender for sending messages to this user
    pub tx: mpsc::UnboundedSender<ServerMessage>,
    /// Features enabled for this user
    pub features: Vec<String>,
    /// User's preferred locale (e.g., "en", "en-US", "zh-CN")
    pub locale: String,
}

impl User {
    /// Create a new user
    pub fn new(params: NewUserParams) -> Self {
        Self {
            session_id: params.session_id,
            db_user_id: params.db_user_id,
            username: params.username,
            address: params.address,
            created_at: params.created_at,
            login_time: current_timestamp(),
            tx: params.tx,
            features: params.features,
            locale: params.locale,
        }
    }

    /// Check if user has a specific feature enabled
    pub fn has_feature(&self, feature: &str) -> bool {
        self.features.iter().any(|f| f == feature)
    }
}

/// Get current Unix timestamp in seconds
///
/// # Panics
///
/// Panics if system time is set before Unix epoch (January 1, 1970).
/// This should never happen on properly configured systems.
fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("System time is before Unix epoch - check system clock configuration")
        .as_secs()
}
