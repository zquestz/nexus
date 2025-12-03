//! Query methods for UserManager

use super::UserManager;
use crate::users::user::UserSession;

impl UserManager {
    /// Get all connected users
    pub async fn get_all_users(&self) -> Vec<UserSession> {
        let users = self.users.read().await;
        users.values().cloned().collect()
    }

    /// Get a user by session ID
    pub async fn get_user_by_session_id(&self, session_id: u32) -> Option<UserSession> {
        let users = self.users.read().await;
        users.get(&session_id).cloned()
    }

    /// Get a session by username (case-insensitive)
    ///
    /// Returns the first matching session if the user has multiple sessions.
    /// For all sessions of a user, use `get_sessions_by_username()`.
    pub async fn get_session_by_username(&self, username: &str) -> Option<UserSession> {
        let users = self.users.read().await;
        let username_lower = username.to_lowercase();
        users
            .values()
            .find(|u| u.username.to_lowercase() == username_lower)
            .cloned()
    }

    /// Get all sessions for a username (case-insensitive)
    ///
    /// Returns all sessions for a user who may be logged in from multiple devices.
    pub async fn get_sessions_by_username(&self, username: &str) -> Vec<UserSession> {
        let users = self.users.read().await;
        let username_lower = username.to_lowercase();
        users
            .values()
            .filter(|u| u.username.to_lowercase() == username_lower)
            .cloned()
            .collect()
    }

    /// Get all session IDs for a given username (case-insensitive)
    pub async fn get_session_ids_for_user(&self, username: &str) -> Vec<u32> {
        let users = self.users.read().await;
        let username_lower = username.to_lowercase();
        users
            .iter()
            .filter(|(_, user)| user.username.to_lowercase() == username_lower)
            .map(|(session_id, _)| *session_id)
            .collect()
    }
}
