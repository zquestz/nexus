//! Query methods for UserManager

use ipnet::IpNet;

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

    /// Get all session IDs for a given nickname (case-insensitive)
    ///
    /// This works correctly for both regular and shared accounts:
    /// - Regular accounts: nickname == username, so returns all sessions of the user
    /// - Shared accounts: each session has a unique nickname, so returns just that session
    pub async fn get_session_ids_for_nickname(&self, nickname: &str) -> Vec<u32> {
        let users = self.users.read().await;
        let nickname_lower = nickname.to_lowercase();
        users
            .iter()
            .filter(|(_, user)| user.nickname.to_lowercase() == nickname_lower)
            .map(|(session_id, _)| *session_id)
            .collect()
    }

    /// Check if a nickname is already in use by an active session (case-insensitive)
    ///
    /// Used during login to ensure nickname uniqueness for shared accounts.
    /// Since nickname is always populated (equals username for regular accounts),
    /// this effectively checks against all display names of logged-in users.
    pub async fn is_nickname_in_use(&self, nickname: &str) -> bool {
        let users = self.users.read().await;
        let nickname_lower = nickname.to_lowercase();
        users
            .values()
            .any(|u| u.nickname.to_lowercase() == nickname_lower)
    }

    /// Get a session by nickname (case-insensitive)
    ///
    /// Since nickname is always populated (equals username for regular accounts),
    /// this finds any user by their display name.
    /// Returns None if no session with that nickname is found.
    pub async fn get_session_by_nickname(&self, nickname: &str) -> Option<UserSession> {
        let users = self.users.read().await;
        let nickname_lower = nickname.to_lowercase();
        users
            .values()
            .find(|u| u.nickname.to_lowercase() == nickname_lower)
            .cloned()
    }

    /// Get all sessions with a specific nickname (case-insensitive)
    ///
    /// This works correctly for both regular and shared accounts:
    /// - Regular accounts: nickname == username, so returns all sessions of the user
    /// - Shared accounts: each session has a unique nickname, so returns just that session
    ///
    /// This is useful for operations that need to affect all sessions of a "user"
    /// as identified by their display name (e.g., kicking, disconnecting).
    pub async fn get_sessions_by_nickname(&self, nickname: &str) -> Vec<UserSession> {
        let users = self.users.read().await;
        let nickname_lower = nickname.to_lowercase();
        users
            .values()
            .filter(|u| u.nickname.to_lowercase() == nickname_lower)
            .cloned()
            .collect()
    }

    /// Check if any admin is connected from a given IP address
    ///
    /// Used by the ban system to prevent banning an IP that has an admin connected.
    pub async fn is_admin_connected_from_ip(&self, ip: &str) -> bool {
        let users = self.users.read().await;
        users
            .values()
            .any(|u| u.is_admin && u.address.ip().to_string() == ip)
    }

    /// Check if any admin is connected from an IP within a given CIDR range
    ///
    /// Used by the ban system to prevent banning a CIDR range that contains an admin's IP.
    pub async fn is_admin_connected_in_range(&self, range: &IpNet) -> bool {
        let users = self.users.read().await;
        users
            .values()
            .any(|u| u.is_admin && range.contains(&u.address.ip()))
    }

    /// Get sorted nicknames for a list of session IDs
    ///
    /// Looks up the nickname for each session ID and returns them sorted
    /// alphabetically (case-insensitive). Sessions that don't exist are skipped.
    ///
    /// Used by channel join handlers to build member lists.
    pub async fn get_nicknames_for_sessions(&self, session_ids: &[u32]) -> Vec<String> {
        let users = self.users.read().await;
        let mut nicknames: Vec<String> = session_ids
            .iter()
            .filter_map(|&session_id| users.get(&session_id).map(|u| u.nickname.clone()))
            .collect();
        nicknames.sort_by_key(|n| n.to_lowercase());
        nicknames
    }

    /// Get all unique IP addresses for sessions with a given nickname
    ///
    /// Used by the ban system to get IPs when banning by nickname.
    pub async fn get_ips_for_nickname(&self, nickname: &str) -> Vec<String> {
        let users = self.users.read().await;
        let nickname_lower = nickname.to_lowercase();
        let mut ips: Vec<String> = users
            .values()
            .filter(|u| u.nickname.to_lowercase() == nickname_lower)
            .map(|u| u.address.ip().to_string())
            .collect();
        ips.sort();
        ips.dedup();
        ips
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::users::user::NewSessionParams;

    /// Create a test session params with the given username and nickname
    fn test_session_params(username: &str, nickname: &str, is_shared: bool) -> NewSessionParams {
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
            nickname: nickname.to_string(),
            is_away: false,
            status: None,
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
        let params = test_session_params("shared_acct", "Nick1", true);
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
    }

    #[tokio::test]
    async fn test_is_nickname_in_use_matches_username() {
        let manager = UserManager::new();

        // Add a regular user (nickname == username) with username "alice"
        let params = test_session_params("alice", "alice", false);
        manager
            .add_user(params)
            .await
            .expect("add_user should succeed");

        // The nickname (which equals username for regular users) should be in use
        // This prevents a shared account from using a nickname that matches
        // a logged-in regular user's username/nickname
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

        // Add a regular user "alice" (nickname == username)
        let params1 = test_session_params("alice", "alice", false);
        manager
            .add_user(params1)
            .await
            .expect("add_user should succeed");

        // Add a shared account user with nickname "Nick1"
        let params2 = test_session_params("shared_acct", "Nick1", true);
        manager
            .add_user(params2)
            .await
            .expect("add_user should succeed");

        // Both nicknames should be detected
        assert!(manager.is_nickname_in_use("alice").await);
        assert!(manager.is_nickname_in_use("Nick1").await);

        // The shared account's username is NOT the nickname, so not detected
        // (the nickname "Nick1" is what's checked, not "shared_acct")
        assert!(!manager.is_nickname_in_use("shared_acct").await);

        // Other names should not be in use
        assert!(!manager.is_nickname_in_use("bob").await);
    }

    #[tokio::test]
    async fn test_is_nickname_in_use_shared_account_username_not_blocked() {
        let manager = UserManager::new();

        // Add a shared account user with nickname "Nick1"
        // The shared account's username is "shared_acct"
        let params = test_session_params("shared_acct", "Nick1", true);
        manager
            .add_user(params)
            .await
            .expect("add_user should succeed");

        // The nickname "Nick1" should be in use
        assert!(manager.is_nickname_in_use("Nick1").await);

        // The shared account's username is NOT checked - only nicknames are.
        // For shared accounts, nickname != username, so shared_acct is available.
        // (The DB username uniqueness check prevents collisions with account names)
        assert!(!manager.is_nickname_in_use("shared_acct").await);
    }

    // =========================================================================
    // get_session_by_nickname tests
    // =========================================================================

    #[tokio::test]
    async fn test_get_session_by_nickname_not_found() {
        let manager = UserManager::new();

        // Empty manager
        assert!(manager.get_session_by_nickname("alice").await.is_none());

        // Add a regular user (nickname == username)
        let params = test_session_params("alice", "alice", false);
        manager
            .add_user(params)
            .await
            .expect("add_user should succeed");

        // Regular users have nickname == username, so this WILL find them
        assert!(manager.get_session_by_nickname("alice").await.is_some());

        // But searching for a different name won't find them
        assert!(manager.get_session_by_nickname("bob").await.is_none());
    }

    #[tokio::test]
    async fn test_get_session_by_nickname_found() {
        let manager = UserManager::new();

        // Add a shared account user with nickname "Nick1"
        let params = test_session_params("shared_acct", "Nick1", true);
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
        assert_eq!(session.nickname, "Nick1");
    }

    #[tokio::test]
    async fn test_get_session_by_nickname_case_insensitive() {
        let manager = UserManager::new();

        // Add a shared account user with nickname "Nick1"
        let params = test_session_params("shared_acct", "Nick1", true);
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
        let params = test_session_params("shared_acct", "Nick1", true);
        manager
            .add_user(params)
            .await
            .expect("add_user should succeed");

        // Should NOT find by the account's username (nickname is "Nick1", not "shared_acct")
        assert!(
            manager
                .get_session_by_nickname("shared_acct")
                .await
                .is_none()
        );

        // Should find by nickname
        assert!(manager.get_session_by_nickname("Nick1").await.is_some());
    }

    #[tokio::test]
    async fn test_get_session_by_nickname_multiple_shared_users() {
        let manager = UserManager::new();

        // Add two shared account users with different nicknames
        let params1 = test_session_params("shared_acct", "Nick1", true);
        let session_id1 = manager
            .add_user(params1)
            .await
            .expect("add_user should succeed");

        let params2 = test_session_params("shared_acct", "Nick2", true);
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
