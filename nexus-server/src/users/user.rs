//! User session representation for logged-in users

use std::collections::HashSet;
use std::net::SocketAddr;

use nexus_common::framing::MessageId;
use nexus_common::protocol::ServerMessage;
use tokio::sync::mpsc;

use crate::db::Permission;

/// Parameters for creating a new user session
pub struct NewSessionParams {
    pub session_id: u32,
    pub db_user_id: i64,
    pub username: String,
    pub is_admin: bool,
    pub permissions: HashSet<Permission>,
    pub address: SocketAddr,
    pub created_at: i64,
    pub tx: mpsc::UnboundedSender<(ServerMessage, Option<MessageId>)>,
    pub features: Vec<String>,
    pub locale: String,
    /// User's avatar as a data URI (ephemeral, not stored in DB)
    pub avatar: Option<String>,
}

/// Represents a logged-in user session
#[derive(Debug, Clone)]
pub struct UserSession {
    /// Session ID (unique identifier for this connection)
    pub session_id: u32,
    /// Database user ID
    pub db_user_id: i64,
    /// Username
    pub username: String,
    /// Whether the user is an admin
    pub is_admin: bool,
    /// Cached permissions for this user (admins bypass this check)
    pub permissions: HashSet<Permission>,
    /// Remote address of the user's connection
    pub address: SocketAddr,
    /// When the user account was created (Unix timestamp from database)
    ///
    /// This field is stored for potential future features like account age display,
    /// statistics, or audit logging.
    #[allow(dead_code)]
    pub created_at: i64,
    /// When the user logged in (Unix timestamp)
    pub login_time: i64,
    /// Channel sender for sending messages to this user
    pub tx: mpsc::UnboundedSender<(ServerMessage, Option<MessageId>)>,
    /// Features enabled for this user
    pub features: Vec<String>,
    /// User's preferred locale (e.g., "en", "en-US", "zh-CN")
    pub locale: String,
    /// User's avatar as a data URI (ephemeral, not stored in DB)
    pub avatar: Option<String>,
}

impl UserSession {
    /// Create a new user session
    pub fn new(params: NewSessionParams) -> Self {
        Self {
            session_id: params.session_id,
            db_user_id: params.db_user_id,
            username: params.username,
            is_admin: params.is_admin,
            permissions: params.permissions,
            address: params.address,
            created_at: params.created_at,
            login_time: current_timestamp(),
            tx: params.tx,
            features: params.features,
            locale: params.locale,
            avatar: params.avatar,
        }
    }

    /// Check if user has a specific feature enabled
    pub fn has_feature(&self, feature: &str) -> bool {
        self.features.iter().any(|f| f == feature)
    }

    /// Check if user has a specific permission (admins have all permissions)
    pub fn has_permission(&self, permission: Permission) -> bool {
        if self.is_admin {
            true
        } else {
            self.permissions.contains(&permission)
        }
    }
}

/// Get current Unix timestamp in seconds
///
/// # Panics
///
/// Panics if system time is set before Unix epoch (January 1, 1970).
/// This should never happen on properly configured systems.
fn current_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("System time is before Unix epoch - check system clock configuration")
        .as_secs() as i64
}
