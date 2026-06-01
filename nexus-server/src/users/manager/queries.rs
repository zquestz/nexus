//! Query methods for UserManager

use ipnet::IpNet;
use nexus_common::names::fold_name;

use super::{NicknameIpSnapshot, UserManager};
use crate::db::Permission;
use crate::users::user::UserSession;

#[cfg(test)]
use std::collections::HashSet;
#[cfg(test)]
use std::net::SocketAddr;

impl UserManager {
    pub async fn get_all_users(&self) -> Vec<UserSession> {
        let users = self.users.read().await;
        users.values().cloned().collect()
    }

    /// Distinct online users for the protocol `user_count` (`UserList`,
    /// tracker registration). Regular accounts collapse to one (single
    /// nickname); shared/guest sessions count individually (each has its own
    /// nickname); pre-login connections aren't in `UserManager`.
    #[must_use]
    pub async fn user_count(&self) -> u32 {
        let users = self.users.read().await;
        let mut nicknames = std::collections::HashSet::new();
        for u in users.values() {
            nicknames.insert(fold_name(&u.nickname));
        }
        u32::try_from(nicknames.len()).unwrap_or(u32::MAX)
    }

    pub async fn get_user_by_session_id(&self, session_id: u32) -> Option<UserSession> {
        let users = self.users.read().await;
        users.get(&session_id).cloned()
    }

    /// Permission check without cloning the session, for hot paths like voice
    /// packet relay. `None` if the user is not found.
    pub async fn has_permission(&self, session_id: u32, permission: Permission) -> Option<bool> {
        let users = self.users.read().await;
        users.get(&session_id).map(|u| u.has_permission(permission))
    }

    /// All sessions for a username (case-insensitive); a user may be logged in
    /// from multiple devices.
    pub async fn get_sessions_by_username(&self, username: &str) -> Vec<UserSession> {
        let users = self.users.read().await;
        let username_lower = fold_name(username);
        users
            .values()
            .filter(|u| fold_name(&u.username) == username_lower)
            .cloned()
            .collect()
    }

    /// All sessions owned by `user_id`. Stable across username
    /// changes, so safer than `get_sessions_by_username` for
    /// cleanup paths keyed on the DB row's identity.
    pub async fn get_sessions_by_user_id(&self, user_id: i64) -> Vec<UserSession> {
        let users = self.users.read().await;
        users
            .values()
            .filter(|u| u.user_id == user_id)
            .cloned()
            .collect()
    }

    /// All online sessions whose `group_id` matches, for the group edit cascade.
    pub async fn get_sessions_by_group_id(&self, group_id: i64) -> Vec<UserSession> {
        let users = self.users.read().await;
        users
            .values()
            .filter(|u| u.group_id == Some(group_id))
            .cloned()
            .collect()
    }

    /// Whether a nickname is in use by an active session (case-insensitive), to
    /// enforce shared-account nickname uniqueness at login. Nickname is always
    /// populated (equals username for regular accounts), so this checks against
    /// all logged-in display names.
    pub async fn is_nickname_in_use(&self, nickname: &str) -> bool {
        let users = self.users.read().await;
        let nickname_lower = fold_name(nickname);
        users
            .values()
            .any(|u| fold_name(&u.nickname) == nickname_lower)
    }

    /// Find a session by display nickname (case-insensitive). Nickname is always
    /// populated (equals username for regular accounts).
    pub async fn get_session_by_nickname(&self, nickname: &str) -> Option<UserSession> {
        let users = self.users.read().await;
        let nickname_lower = fold_name(nickname);
        users
            .values()
            .find(|u| fold_name(&u.nickname) == nickname_lower)
            .cloned()
    }

    /// Representative session plus unique IPs for a display nickname, captured
    /// from one user-map snapshot. This avoids lookup/IP races in handlers that
    /// turn nickname targets into IP-keyed rules.
    pub async fn get_nickname_ip_snapshot(&self, nickname: &str) -> Option<NicknameIpSnapshot> {
        let users = self.users.read().await;
        let nickname_lower = fold_name(nickname);
        let mut representative = None;
        let mut ips = Vec::new();

        for user in users
            .values()
            .filter(|u| fold_name(&u.nickname) == nickname_lower)
        {
            if representative.is_none() {
                representative = Some(user.clone());
            }
            ips.push(user.address.ip().to_string());
        }

        let representative = representative?;
        ips.sort();
        ips.dedup();

        Some(NicknameIpSnapshot {
            representative,
            ips,
        })
    }

    /// All sessions with a given nickname (case-insensitive) — every session of
    /// a "user" identified by display name, for kick/disconnect. Regular
    /// accounts (nickname == username) return all of the user's sessions; shared
    /// accounts have a unique nickname per session, so this returns just one.
    pub async fn get_sessions_by_nickname(&self, nickname: &str) -> Vec<UserSession> {
        let users = self.users.read().await;
        let nickname_lower = fold_name(nickname);
        users
            .values()
            .filter(|u| fold_name(&u.nickname) == nickname_lower)
            .cloned()
            .collect()
    }

    /// Whether any admin is connected from `ip`, so the ban system won't ban it.
    pub async fn is_admin_connected_from_ip(&self, ip: &str) -> bool {
        let users = self.users.read().await;
        users
            .values()
            .any(|u| u.is_admin && u.address.ip().to_string() == ip)
    }

    /// Whether any admin's IP falls within `range`, so the ban system won't ban it.
    pub async fn is_admin_connected_in_range(&self, range: &IpNet) -> bool {
        let users = self.users.read().await;
        users
            .values()
            .any(|u| u.is_admin && range.contains(&u.address.ip()))
    }

    /// Nicknames for the given session IDs, sorted alphabetically
    /// (case-insensitive). Nonexistent sessions are skipped.
    pub async fn get_nicknames_for_sessions(&self, session_ids: &[u32]) -> Vec<String> {
        let users = self.users.read().await;
        let mut nicknames: Vec<String> = session_ids
            .iter()
            .filter_map(|&session_id| users.get(&session_id).map(|u| u.nickname.clone()))
            .collect();
        nicknames.sort_by_key(|n| fold_name(n));
        nicknames
    }

    /// One entry per visible nickname (e.g. channel member lists): sorted
    /// alphabetically and deduplicated, both case-insensitive. Nonexistent
    /// sessions are skipped.
    pub async fn get_unique_nicknames_for_sessions(&self, session_ids: &[u32]) -> Vec<String> {
        let mut nicknames = self.get_nicknames_for_sessions(session_ids).await;

        // `dedup_by_key` only removes adjacent items, but the source is already
        // sorted case-insensitively, so duplicates are adjacent.
        nicknames.dedup_by_key(|n| fold_name(n));

        nicknames
    }

    /// Whether any session in `session_ids` (excluding `skip_session_id`) has
    /// `nickname` (case-insensitive). For nickname-based channel presence gating
    /// when membership is tracked by session id.
    pub async fn sessions_contain_nickname(
        &self,
        session_ids: &[u32],
        nickname: &str,
        skip_session_id: Option<u32>,
    ) -> bool {
        let users = self.users.read().await;
        let nickname_lower = fold_name(nickname);

        for &sid in session_ids {
            if skip_session_id.is_some_and(|skip| skip == sid) {
                continue;
            }

            let Some(user) = users.get(&sid) else {
                continue;
            };

            if fold_name(&user.nickname) == nickname_lower {
                return true;
            }
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::users::user::{ConnectionWriter, NewSessionParams};

    fn test_session_params(username: &str, nickname: &str, is_shared: bool) -> NewSessionParams {
        let (tx, _rx) = ConnectionWriter::channel();
        NewSessionParams {
            session_id: 0, // Will be assigned by add_user
            user_id: 1,
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
            group_id: None,
            group_name: None,
            bandwidth_weight_override: None,
            last_activity: std::time::Instant::now(),
            bandwidth_weight: 1,
        }
    }

    #[tokio::test]
    async fn test_is_nickname_in_use_empty_manager() {
        let manager = UserManager::new();

        assert!(!manager.is_nickname_in_use("alice").await);
        assert!(!manager.is_nickname_in_use("Bob").await);
    }

    #[tokio::test]
    async fn test_is_nickname_in_use_matches_nickname() {
        let manager = UserManager::new();

        let params = test_session_params("shared_acct", "Nick1", true);
        manager
            .add_user(params)
            .await
            .expect("add_user should succeed");

        assert!(manager.is_nickname_in_use("Nick1").await);
        // Case-insensitive
        assert!(manager.is_nickname_in_use("nick1").await);
        assert!(manager.is_nickname_in_use("NICK1").await);

        assert!(!manager.is_nickname_in_use("Nick2").await);
    }

    #[tokio::test]
    async fn test_is_nickname_in_use_unicode() {
        let manager = UserManager::new();

        let params = test_session_params("shared_acct", "Renée", true);
        manager
            .add_user(params)
            .await
            .expect("add_user should succeed");

        // Regression guard: nickname matching folds via fold_name (full Unicode),
        // so a non-ASCII case pair (uppercase É folds to é) is detected as in use.
        assert!(manager.is_nickname_in_use("Renée").await);
        assert!(manager.is_nickname_in_use("renée").await);
        assert!(manager.is_nickname_in_use("RENÉE").await);

        assert!(!manager.is_nickname_in_use("Renato").await);
    }

    #[tokio::test]
    async fn test_is_nickname_in_use_matches_username() {
        let manager = UserManager::new();

        let params = test_session_params("alice", "alice", false);
        manager
            .add_user(params)
            .await
            .expect("add_user should succeed");

        // Regular user's nickname == username, so a shared account can't reuse it
        assert!(manager.is_nickname_in_use("alice").await);
        // Case-insensitive
        assert!(manager.is_nickname_in_use("Alice").await);
        assert!(manager.is_nickname_in_use("ALICE").await);

        assert!(!manager.is_nickname_in_use("bob").await);
    }

    #[tokio::test]
    async fn test_is_nickname_in_use_multiple_users() {
        let manager = UserManager::new();

        let params1 = test_session_params("alice", "alice", false);
        manager
            .add_user(params1)
            .await
            .expect("add_user should succeed");

        let params2 = test_session_params("shared_acct", "Nick1", true);
        manager
            .add_user(params2)
            .await
            .expect("add_user should succeed");

        assert!(manager.is_nickname_in_use("alice").await);
        assert!(manager.is_nickname_in_use("Nick1").await);

        // Shared account's username is not its nickname, so it's not detected
        assert!(!manager.is_nickname_in_use("shared_acct").await);

        assert!(!manager.is_nickname_in_use("bob").await);
    }

    #[tokio::test]
    async fn test_is_nickname_in_use_shared_account_username_not_blocked() {
        let manager = UserManager::new();

        let params = test_session_params("shared_acct", "Nick1", true);
        manager
            .add_user(params)
            .await
            .expect("add_user should succeed");

        assert!(manager.is_nickname_in_use("Nick1").await);

        // Only nicknames are checked, not usernames; the DB's username uniqueness
        // check prevents collisions with account names.
        assert!(!manager.is_nickname_in_use("shared_acct").await);
    }

    #[tokio::test]
    async fn test_get_session_by_nickname_not_found() {
        let manager = UserManager::new();

        assert!(manager.get_session_by_nickname("alice").await.is_none());

        let params = test_session_params("alice", "alice", false);
        manager
            .add_user(params)
            .await
            .expect("add_user should succeed");

        // Regular users have nickname == username, so this finds them
        assert!(manager.get_session_by_nickname("alice").await.is_some());

        assert!(manager.get_session_by_nickname("bob").await.is_none());
    }

    #[tokio::test]
    async fn test_get_session_by_nickname_found() {
        let manager = UserManager::new();

        let params = test_session_params("shared_acct", "Nick1", true);
        let session_id = manager
            .add_user(params)
            .await
            .expect("add_user should succeed");

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

        let params = test_session_params("shared_acct", "Nick1", true);
        manager
            .add_user(params)
            .await
            .expect("add_user should succeed");

        // Not found by the account's username, only by nickname
        assert!(
            manager
                .get_session_by_nickname("shared_acct")
                .await
                .is_none()
        );

        assert!(manager.get_session_by_nickname("Nick1").await.is_some());
    }

    #[tokio::test]
    async fn test_get_session_by_nickname_multiple_shared_users() {
        let manager = UserManager::new();

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

        assert!(manager.get_session_by_nickname("Nick3").await.is_none());
    }

    #[tokio::test]
    async fn test_get_nickname_ip_snapshot_deduplicates_ips() {
        let manager = UserManager::new();

        let mut params1 = test_session_params("alice", "alice", false);
        params1.address = "192.168.1.50:12345".parse::<SocketAddr>().unwrap();
        manager
            .add_user(params1)
            .await
            .expect("add_user should succeed");

        let mut params2 = test_session_params("alice", "alice", false);
        params2.address = "192.168.1.51:12345".parse::<SocketAddr>().unwrap();
        manager
            .add_user(params2)
            .await
            .expect("add_user should succeed");

        let mut params3 = test_session_params("alice", "alice", false);
        params3.address = "192.168.1.50:54321".parse::<SocketAddr>().unwrap();
        manager
            .add_user(params3)
            .await
            .expect("add_user should succeed");

        let snapshot = manager
            .get_nickname_ip_snapshot("ALICE")
            .await
            .expect("snapshot should exist");

        assert_eq!(snapshot.representative.nickname, "alice");
        assert_eq!(snapshot.ips, vec!["192.168.1.50", "192.168.1.51"]);
    }

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

        assert_eq!(nicknames, vec!["alice"]);
    }

    #[tokio::test]
    async fn test_get_unique_nicknames_case_insensitive_dedup() {
        let manager = UserManager::new();

        // Same regular user, two sessions: case-insensitive dedup
        let params1 = test_session_params("Alice", "Alice", false);
        let session1 = manager
            .add_user(params1)
            .await
            .expect("add_user should succeed");

        let params2 = test_session_params("Alice", "Alice", false);
        let session2 = manager
            .add_user(params2)
            .await
            .expect("add_user should succeed");

        let nicknames = manager
            .get_unique_nicknames_for_sessions(&[session1, session2])
            .await;

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

        // Two shared-account sessions, each with its own nickname
        let params2 = test_session_params("shared_acct", "Guest123", true);
        let session2 = manager
            .add_user(params2)
            .await
            .expect("add_user should succeed");

        let params3 = test_session_params("shared_acct", "Visitor", true);
        let session3 = manager
            .add_user(params3)
            .await
            .expect("add_user should succeed");

        let nicknames = manager
            .get_unique_nicknames_for_sessions(&[session1, session2, session3])
            .await;

        // Sorted alphabetically (case-insensitive)
        assert_eq!(nicknames, vec!["alice", "Guest123", "Visitor"]);
    }

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

        // Both sessions are "alice"; skipping one still leaves the other
        let result = manager
            .sessions_contain_nickname(&[session1, session2], "alice", Some(session1))
            .await;
        assert!(result);

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

        // Only session1 is "alice"; skipping it leaves no match
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

        // Nonexistent sessions are skipped
        let result = manager
            .sessions_contain_nickname(&[99999, session_id], "alice", None)
            .await;
        assert!(result);

        let result = manager
            .sessions_contain_nickname(&[99998, 99999], "alice", None)
            .await;
        assert!(!result);
    }

    #[tokio::test]
    async fn test_sessions_contain_nickname_shared_account() {
        let manager = UserManager::new();

        let params = test_session_params("shared_acct", "Guest123", true);
        let session_id = manager
            .add_user(params)
            .await
            .expect("add_user should succeed");

        // Matches by nickname, not username
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

        // Multiple shared-account sessions, each with its own nickname
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

        // Skipping session1 hides Guest1 but not the other nicknames
        assert!(
            !manager
                .sessions_contain_nickname(&sessions, "Guest1", Some(session1))
                .await
        );
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
