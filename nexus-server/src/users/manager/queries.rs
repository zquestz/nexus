//! Query methods for UserManager

use super::UserManager;
use crate::users::user::UserSession;

#[cfg(test)]
use std::collections::HashSet;
#[cfg(test)]
use std::net::SocketAddr;
#[cfg(test)]
use tokio::sync::mpsc;

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

    /// Check if a nickname is already in use by an active session (case-insensitive)
    ///
    /// Used during login to ensure nickname uniqueness for shared accounts.
    /// This checks against both nicknames (for shared accounts) and usernames
    /// (for regular accounts that don't have nicknames).
    pub async fn is_nickname_in_use(&self, nickname: &str) -> bool {
        let users = self.users.read().await;
        let nickname_lower = nickname.to_lowercase();
        users.values().any(|u| {
            // Check against nickname if present (shared accounts)
            if let Some(ref user_nickname) = u.nickname
                && user_nickname.to_lowercase() == nickname_lower
            {
                return true;
            }
            // Also check against username for regular accounts
            // This ensures a shared account can't use a nickname that matches
            // a logged-in regular user's username
            u.username.to_lowercase() == nickname_lower
        })
    }

    /// Get a session by nickname (case-insensitive)
    ///
    /// Used for routing private messages to shared account users.
    /// Returns None if no session with that nickname is found.
    pub async fn get_session_by_nickname(&self, nickname: &str) -> Option<UserSession> {
        let users = self.users.read().await;
        let nickname_lower = nickname.to_lowercase();
        users
            .values()
            .find(|u| {
                u.nickname
                    .as_ref()
                    .is_some_and(|n| n.to_lowercase() == nickname_lower)
            })
            .cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::users::user::NewSessionParams;

    /// Create a test session params with the given username and optional nickname
    fn test_session_params(
        username: &str,
        nickname: Option<&str>,
        is_shared: bool,
    ) -> NewSessionParams {
        let (tx, _rx) = mpsc::unbounded_channel();
        NewSessionParams {
            session_id: 0, // Will be assigned by add_user
            db_user_id: 1,
            username: username.to_string(),
            is_admin: false,
            is_shared,
            permissions: HashSet::new(),
            address: "127.0.0.1:12345".parse::<SocketAddr>().unwrap(),
            created_at: 0,
            tx,
            features: vec![],
            locale: "en".to_string(),
            avatar: None,
            nickname: nickname.map(|s| s.to_string()),
        }
    }

    // =========================================================================
    // is_nickname_in_use tests
    // =========================================================================

    #[tokio::test]
    async fn test_is_nickname_in_use_empty_manager() {
        let manager = UserManager::new();

        // No users, so no nicknames in use
        assert!(!manager.is_nickname_in_use("alice").await);
        assert!(!manager.is_nickname_in_use("Bob").await);
    }

    #[tokio::test]
    async fn test_is_nickname_in_use_matches_nickname() {
        let manager = UserManager::new();

        // Add a shared account user with nickname "Nick1"
        let params = test_session_params("shared_acct", Some("Nick1"), true);
        manager
            .add_user(params)
            .await
            .expect("add_user should succeed");

        // The nickname should be in use
        assert!(manager.is_nickname_in_use("Nick1").await);
        // Case-insensitive
        assert!(manager.is_nickname_in_use("nick1").await);
        assert!(manager.is_nickname_in_use("NICK1").await);

        // Different nicknames should not be in use
        assert!(!manager.is_nickname_in_use("Nick2").await);
        assert!(!manager.is_nickname_in_use("alice").await);
    }

    #[tokio::test]
    async fn test_is_nickname_in_use_matches_username() {
        let manager = UserManager::new();

        // Add a regular user (no nickname) with username "alice"
        let params = test_session_params("alice", None, false);
        manager
            .add_user(params)
            .await
            .expect("add_user should succeed");

        // The username should be detected as "in use" for nickname purposes
        // This prevents a shared account from using a nickname that matches
        // a logged-in regular user's username
        assert!(manager.is_nickname_in_use("alice").await);
        // Case-insensitive
        assert!(manager.is_nickname_in_use("Alice").await);
        assert!(manager.is_nickname_in_use("ALICE").await);

        // Different names should not be in use
        assert!(!manager.is_nickname_in_use("bob").await);
    }

    #[tokio::test]
    async fn test_is_nickname_in_use_multiple_users() {
        let manager = UserManager::new();

        // Add a regular user "alice"
        let params1 = test_session_params("alice", None, false);
        manager
            .add_user(params1)
            .await
            .expect("add_user should succeed");

        // Add a shared account user with nickname "Nick1"
        let params2 = test_session_params("shared_acct", Some("Nick1"), true);
        manager
            .add_user(params2)
            .await
            .expect("add_user should succeed");

        // Both should be detected
        assert!(manager.is_nickname_in_use("alice").await);
        assert!(manager.is_nickname_in_use("Nick1").await);

        // The shared account's username IS also detected - this is intentional
        // defense in depth to prevent any collision with logged-in usernames
        assert!(manager.is_nickname_in_use("shared_acct").await);

        // Other names should not be in use
        assert!(!manager.is_nickname_in_use("bob").await);
    }

    #[tokio::test]
    async fn test_is_nickname_in_use_shared_account_username_also_blocked() {
        let manager = UserManager::new();

        // Add a shared account user with nickname "Nick1"
        // The shared account's username is "shared_acct"
        let params = test_session_params("shared_acct", Some("Nick1"), true);
        manager
            .add_user(params)
            .await
            .expect("add_user should succeed");

        // The nickname "Nick1" should be in use
        assert!(manager.is_nickname_in_use("Nick1").await);

        // The shared account's username is ALSO blocked - this is intentional.
        // The check is against ALL usernames of logged-in sessions, regardless
        // of whether they are shared or regular accounts. This provides defense
        // in depth against any edge cases where a nickname could collide with
        // a logged-in account name.
        assert!(manager.is_nickname_in_use("shared_acct").await);
    }

    // =========================================================================
    // get_session_by_nickname tests
    // =========================================================================

    #[tokio::test]
    async fn test_get_session_by_nickname_not_found() {
        let manager = UserManager::new();

        // Empty manager
        assert!(manager.get_session_by_nickname("alice").await.is_none());

        // Add a regular user (no nickname)
        let params = test_session_params("alice", None, false);
        manager
            .add_user(params)
            .await
            .expect("add_user should succeed");

        // Regular users don't have nicknames, so this should not find them
        assert!(manager.get_session_by_nickname("alice").await.is_none());
    }

    #[tokio::test]
    async fn test_get_session_by_nickname_found() {
        let manager = UserManager::new();

        // Add a shared account user with nickname "Nick1"
        let params = test_session_params("shared_acct", Some("Nick1"), true);
        let session_id = manager
            .add_user(params)
            .await
            .expect("add_user should succeed");

        // Should find by nickname
        let session = manager.get_session_by_nickname("Nick1").await;
        assert!(session.is_some());
        let session = session.unwrap();
        assert_eq!(session.session_id, session_id);
        assert_eq!(session.username, "shared_acct");
        assert_eq!(session.nickname, Some("Nick1".to_string()));
    }

    #[tokio::test]
    async fn test_get_session_by_nickname_case_insensitive() {
        let manager = UserManager::new();

        // Add a shared account user with nickname "Nick1"
        let params = test_session_params("shared_acct", Some("Nick1"), true);
        let session_id = manager
            .add_user(params)
            .await
            .expect("add_user should succeed");

        // Should find regardless of case
        assert_eq!(
            manager
                .get_session_by_nickname("Nick1")
                .await
                .unwrap()
                .session_id,
            session_id
        );
        assert_eq!(
            manager
                .get_session_by_nickname("nick1")
                .await
                .unwrap()
                .session_id,
            session_id
        );
        assert_eq!(
            manager
                .get_session_by_nickname("NICK1")
                .await
                .unwrap()
                .session_id,
            session_id
        );
    }

    #[tokio::test]
    async fn test_get_session_by_nickname_does_not_match_username() {
        let manager = UserManager::new();

        // Add a shared account user with nickname "Nick1"
        let params = test_session_params("shared_acct", Some("Nick1"), true);
        manager
            .add_user(params)
            .await
            .expect("add_user should succeed");

        // Should NOT find by the account's username
        assert!(
            manager
                .get_session_by_nickname("shared_acct")
                .await
                .is_none()
        );

        // Should only find by nickname
        assert!(manager.get_session_by_nickname("Nick1").await.is_some());
    }

    #[tokio::test]
    async fn test_get_session_by_nickname_multiple_shared_users() {
        let manager = UserManager::new();

        // Add two shared account users with different nicknames
        let params1 = test_session_params("shared_acct", Some("Nick1"), true);
        let session_id1 = manager
            .add_user(params1)
            .await
            .expect("add_user should succeed");

        let params2 = test_session_params("shared_acct", Some("Nick2"), true);
        let session_id2 = manager
            .add_user(params2)
            .await
            .expect("add_user should succeed");

        // Should find each by their own nickname
        assert_eq!(
            manager
                .get_session_by_nickname("Nick1")
                .await
                .unwrap()
                .session_id,
            session_id1
        );
        assert_eq!(
            manager
                .get_session_by_nickname("Nick2")
                .await
                .unwrap()
                .session_id,
            session_id2
        );

        // Should not find non-existent nicknames
        assert!(manager.get_session_by_nickname("Nick3").await.is_none());
    }
}
