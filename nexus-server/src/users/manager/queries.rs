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

    /// Get sorted unique nicknames for a list of session IDs (case-insensitive dedup)
    ///
    /// This is useful when multiple sessions map to the same visible nickname and you
    /// want one entry per nickname (e.g., channel member lists).
    ///
    /// Sessions that don't exist are skipped. The output is sorted alphabetically
    /// (case-insensitive) and deduplicated case-insensitively.
    pub async fn get_unique_nicknames_for_sessions(&self, session_ids: &[u32]) -> Vec<String> {
        let mut nicknames = self.get_nicknames_for_sessions(session_ids).await;

        // `dedup_by_key` only removes adjacent items.
        // `get_nicknames_for_sessions()` already sorts case-insensitively (by `to_lowercase()`),
        // so duplicates will be adjacent and dedup is safe here.
        nicknames.dedup_by_key(|n| n.to_lowercase());

        nicknames
    }

    /// Return true if any session in `session_ids` has a nickname equal to `nickname`
    /// (case-insensitive), excluding an optional `skip_session_id`.
    ///
    /// This is useful for nickname-based channel presence gating when channel membership
    /// is tracked by session id.
    pub async fn sessions_contain_nickname(
        &self,
        session_ids: &[u32],
        nickname: &str,
        skip_session_id: Option<u32>,
    ) -> bool {
        let users = self.users.read().await;
        let nickname_lower = nickname.to_lowercase();

        for &sid in session_ids {
            if skip_session_id.is_some_and(|skip| skip == sid) {
                continue;
            }

            let Some(user) = users.get(&sid) else {
                continue;
            };

            if user.nickname.to_lowercase() == nickname_lower {
                return true;
            }
        }

        false
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

    // =========================================================================
    // get_unique_nicknames_for_sessions tests
    // =========================================================================

    #[tokio::test]
    async fn test_get_unique_nicknames_empty_list() {
        let manager = UserManager::new();

        let nicknames = manager.get_unique_nicknames_for_sessions(&[]).await;
        assert!(nicknames.is_empty());
    }

    #[tokio::test]
    async fn test_get_unique_nicknames_single_session() {
        let manager = UserManager::new();

        let params = test_session_params("alice", "alice", false);
        let session_id = manager
            .add_user(params)
            .await
            .expect("add_user should succeed");

        let nicknames = manager
            .get_unique_nicknames_for_sessions(&[session_id])
            .await;
        assert_eq!(nicknames, vec!["alice"]);
    }

    #[tokio::test]
    async fn test_get_unique_nicknames_multiple_different_users() {
        let manager = UserManager::new();

        let params1 = test_session_params("alice", "alice", false);
        let session1 = manager
            .add_user(params1)
            .await
            .expect("add_user should succeed");

        let params2 = test_session_params("bob", "bob", false);
        let session2 = manager
            .add_user(params2)
            .await
            .expect("add_user should succeed");

        let params3 = test_session_params("charlie", "charlie", false);
        let session3 = manager
            .add_user(params3)
            .await
            .expect("add_user should succeed");

        let nicknames = manager
            .get_unique_nicknames_for_sessions(&[session1, session2, session3])
            .await;

        // Should be sorted alphabetically
        assert_eq!(nicknames, vec!["alice", "bob", "charlie"]);
    }

    #[tokio::test]
    async fn test_get_unique_nicknames_deduplicates_same_nickname() {
        let manager = UserManager::new();

        // Regular user with two sessions (same username = same nickname)
        let params1 = test_session_params("alice", "alice", false);
        let session1 = manager
            .add_user(params1)
            .await
            .expect("add_user should succeed");

        let params2 = test_session_params("alice", "alice", false);
        let session2 = manager
            .add_user(params2)
            .await
            .expect("add_user should succeed");

        let nicknames = manager
            .get_unique_nicknames_for_sessions(&[session1, session2])
            .await;

        // Should deduplicate to single entry
        assert_eq!(nicknames, vec!["alice"]);
    }

    #[tokio::test]
    async fn test_get_unique_nicknames_case_insensitive_dedup() {
        let manager = UserManager::new();

        // Regular user with two sessions (same username = same nickname)
        // This tests case-insensitive dedup when the same user has multiple sessions
        let params1 = test_session_params("Alice", "Alice", false);
        let session1 = manager
            .add_user(params1)
            .await
            .expect("add_user should succeed");

        // Second session for same user - nickname uniqueness doesn't apply to same user
        let params2 = test_session_params("Alice", "Alice", false);
        let session2 = manager
            .add_user(params2)
            .await
            .expect("add_user should succeed");

        let nicknames = manager
            .get_unique_nicknames_for_sessions(&[session1, session2])
            .await;

        // Should deduplicate (both sessions have "Alice")
        assert_eq!(nicknames.len(), 1);
        assert_eq!(nicknames[0], "Alice");
    }

    #[tokio::test]
    async fn test_get_unique_nicknames_skips_nonexistent_sessions() {
        let manager = UserManager::new();

        let params = test_session_params("alice", "alice", false);
        let session_id = manager
            .add_user(params)
            .await
            .expect("add_user should succeed");

        // Include a nonexistent session ID
        let nicknames = manager
            .get_unique_nicknames_for_sessions(&[session_id, 99999])
            .await;

        assert_eq!(nicknames, vec!["alice"]);
    }

    #[tokio::test]
    async fn test_get_unique_nicknames_mixed_regular_and_shared() {
        let manager = UserManager::new();

        // Regular user
        let params1 = test_session_params("alice", "alice", false);
        let session1 = manager
            .add_user(params1)
            .await
            .expect("add_user should succeed");

        // Shared account with different nickname
        let params2 = test_session_params("shared_acct", "Guest123", true);
        let session2 = manager
            .add_user(params2)
            .await
            .expect("add_user should succeed");

        // Another shared account session
        let params3 = test_session_params("shared_acct", "Visitor", true);
        let session3 = manager
            .add_user(params3)
            .await
            .expect("add_user should succeed");

        let nicknames = manager
            .get_unique_nicknames_for_sessions(&[session1, session2, session3])
            .await;

        // Should be sorted alphabetically (case-insensitive)
        assert_eq!(nicknames, vec!["alice", "Guest123", "Visitor"]);
    }

    // =========================================================================
    // sessions_contain_nickname tests
    // =========================================================================

    #[tokio::test]
    async fn test_sessions_contain_nickname_empty_list() {
        let manager = UserManager::new();

        let result = manager.sessions_contain_nickname(&[], "alice", None).await;
        assert!(!result);
    }

    #[tokio::test]
    async fn test_sessions_contain_nickname_found() {
        let manager = UserManager::new();

        let params = test_session_params("alice", "alice", false);
        let session_id = manager
            .add_user(params)
            .await
            .expect("add_user should succeed");

        let result = manager
            .sessions_contain_nickname(&[session_id], "alice", None)
            .await;
        assert!(result);
    }

    #[tokio::test]
    async fn test_sessions_contain_nickname_not_found() {
        let manager = UserManager::new();

        let params = test_session_params("alice", "alice", false);
        let session_id = manager
            .add_user(params)
            .await
            .expect("add_user should succeed");

        let result = manager
            .sessions_contain_nickname(&[session_id], "bob", None)
            .await;
        assert!(!result);
    }

    #[tokio::test]
    async fn test_sessions_contain_nickname_case_insensitive() {
        let manager = UserManager::new();

        let params = test_session_params("alice", "Alice", false);
        let session_id = manager
            .add_user(params)
            .await
            .expect("add_user should succeed");

        // Should match regardless of case
        assert!(
            manager
                .sessions_contain_nickname(&[session_id], "alice", None)
                .await
        );
        assert!(
            manager
                .sessions_contain_nickname(&[session_id], "ALICE", None)
                .await
        );
        assert!(
            manager
                .sessions_contain_nickname(&[session_id], "Alice", None)
                .await
        );
    }

    #[tokio::test]
    async fn test_sessions_contain_nickname_with_skip() {
        let manager = UserManager::new();

        let params1 = test_session_params("alice", "alice", false);
        let session1 = manager
            .add_user(params1)
            .await
            .expect("add_user should succeed");

        let params2 = test_session_params("alice", "alice", false);
        let session2 = manager
            .add_user(params2)
            .await
            .expect("add_user should succeed");

        // Both sessions have nickname "alice"
        // If we skip session1, session2 still has "alice"
        let result = manager
            .sessions_contain_nickname(&[session1, session2], "alice", Some(session1))
            .await;
        assert!(result);

        // If we skip session2, session1 still has "alice"
        let result = manager
            .sessions_contain_nickname(&[session1, session2], "alice", Some(session2))
            .await;
        assert!(result);
    }

    #[tokio::test]
    async fn test_sessions_contain_nickname_skip_only_match() {
        let manager = UserManager::new();

        let params1 = test_session_params("alice", "alice", false);
        let session1 = manager
            .add_user(params1)
            .await
            .expect("add_user should succeed");

        let params2 = test_session_params("bob", "bob", false);
        let session2 = manager
            .add_user(params2)
            .await
            .expect("add_user should succeed");

        // Only session1 has "alice", if we skip session1, result should be false
        let result = manager
            .sessions_contain_nickname(&[session1, session2], "alice", Some(session1))
            .await;
        assert!(!result);
    }

    #[tokio::test]
    async fn test_sessions_contain_nickname_nonexistent_sessions() {
        let manager = UserManager::new();

        let params = test_session_params("alice", "alice", false);
        let session_id = manager
            .add_user(params)
            .await
            .expect("add_user should succeed");

        // Nonexistent session should be skipped
        let result = manager
            .sessions_contain_nickname(&[99999, session_id], "alice", None)
            .await;
        assert!(result);

        // All nonexistent sessions
        let result = manager
            .sessions_contain_nickname(&[99998, 99999], "alice", None)
            .await;
        assert!(!result);
    }

    #[tokio::test]
    async fn test_sessions_contain_nickname_shared_account() {
        let manager = UserManager::new();

        // Shared account with custom nickname
        let params = test_session_params("shared_acct", "Guest123", true);
        let session_id = manager
            .add_user(params)
            .await
            .expect("add_user should succeed");

        // Should match by nickname, not username
        assert!(
            manager
                .sessions_contain_nickname(&[session_id], "Guest123", None)
                .await
        );
        assert!(
            !manager
                .sessions_contain_nickname(&[session_id], "shared_acct", None)
                .await
        );
    }

    #[tokio::test]
    async fn test_sessions_contain_nickname_multiple_sessions_different_nicknames() {
        let manager = UserManager::new();

        // Multiple shared account sessions with different nicknames
        let params1 = test_session_params("shared_acct", "Guest1", true);
        let session1 = manager
            .add_user(params1)
            .await
            .expect("add_user should succeed");

        let params2 = test_session_params("shared_acct", "Guest2", true);
        let session2 = manager
            .add_user(params2)
            .await
            .expect("add_user should succeed");

        let params3 = test_session_params("shared_acct", "Guest3", true);
        let session3 = manager
            .add_user(params3)
            .await
            .expect("add_user should succeed");

        let sessions = [session1, session2, session3];

        assert!(
            manager
                .sessions_contain_nickname(&sessions, "Guest1", None)
                .await
        );
        assert!(
            manager
                .sessions_contain_nickname(&sessions, "Guest2", None)
                .await
        );
        assert!(
            manager
                .sessions_contain_nickname(&sessions, "Guest3", None)
                .await
        );
        assert!(
            !manager
                .sessions_contain_nickname(&sessions, "Guest4", None)
                .await
        );

        // Skip session1, Guest1 should not be found
        assert!(
            !manager
                .sessions_contain_nickname(&sessions, "Guest1", Some(session1))
                .await
        );
        // But Guest2 and Guest3 should still be found
        assert!(
            manager
                .sessions_contain_nickname(&sessions, "Guest2", Some(session1))
                .await
        );
        assert!(
            manager
                .sessions_contain_nickname(&sessions, "Guest3", Some(session1))
                .await
        );
    }
}
