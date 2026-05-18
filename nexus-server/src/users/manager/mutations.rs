//! Mutation methods for UserManager

use std::collections::HashSet;
use std::net::IpAddr;
use std::sync::atomic::Ordering;

use ipnet::IpNet;
use nexus_common::protocol::ServerMessage;

use super::UserManager;
use crate::db::Permission;
use crate::users::user::{NewSessionParams, UserSession};

/// Information about a disconnected session, used for broadcasting UserDisconnected
pub struct DisconnectedSession {
    pub session_id: u32,
    pub nickname: String,
}

/// Error returned when adding a user fails
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddUserError {
    /// The requested nickname is already in use by another session
    NicknameInUse,
}

impl UserManager {
    /// Add a new user and return their assigned session ID
    ///
    /// For shared accounts with nicknames, this performs an atomic check to ensure
    /// the nickname is not already in use by another session or matching a logged-in
    /// username. This prevents race conditions where two users could claim the same
    /// nickname simultaneously.
    ///
    /// # Defense in Depth
    ///
    /// The login handler performs a non-atomic pre-check via `is_nickname_in_use()`
    /// before calling this method. This provides two benefits:
    ///
    /// 1. **Early rejection**: Most conflicts are caught without acquiring the write lock,
    ///    reducing contention for legitimate requests.
    ///
    /// 2. **Atomic guarantee**: This method's check while holding the write lock prevents
    ///    race conditions where two simultaneous logins could both pass the pre-check
    ///    but only one should succeed.
    ///
    /// Both checks are necessary: the pre-check for performance, the atomic check for correctness.
    ///
    /// # Errors
    ///
    /// Returns `AddUserError::NicknameInUse` if the nickname is already taken by
    /// another session (shared or regular).
    pub async fn add_user(&self, mut params: NewSessionParams) -> Result<u32, AddUserError> {
        // Acquire write lock first to ensure atomicity of nickname check + insert
        let mut users = self.users.write().await;

        // For shared accounts, check nickname uniqueness while holding the lock
        // (Regular accounts have nickname == username, so this check is redundant for them,
        // but we do it anyway for consistency)
        if params.is_shared {
            let nickname_lower = params.nickname.to_lowercase();

            for user in users.values() {
                // Check against existing nicknames (all sessions have nicknames now)
                if user.nickname.to_lowercase() == nickname_lower {
                    return Err(AddUserError::NicknameInUse);
                }
            }
        }

        // Nickname is unique (or not a shared account), proceed with adding
        let session_id = self.next_session_id();
        params.session_id = session_id;
        let user = UserSession::new(params);
        users.insert(session_id, user);

        Ok(session_id)
    }

    /// Remove a user by session ID
    pub async fn remove_user(&self, session_id: u32) -> Option<UserSession> {
        let mut users = self.users.write().await;
        users.remove(&session_id)
    }

    /// Remove a user and broadcast UserDisconnected to other clients
    ///
    /// This is a convenience method that combines `remove_user()` with broadcasting
    /// `UserDisconnected` to all users with the `user_list` permission. Use this
    /// for normal disconnects, kicks, account deletion, and account disable.
    ///
    /// For regular accounts with multiple sessions, this also broadcasts `UserUpdated`
    /// with the newest remaining session's info (e.g., avatar) so clients can update.
    ///
    /// For ban disconnects, use `disconnect_sessions_by_ip()` or
    /// `disconnect_sessions_in_range()` instead, as those need to send a custom
    /// message to the disconnected user before removing them.
    pub async fn remove_user_and_broadcast(&self, session_id: u32) -> Option<UserSession> {
        if let Some(user) = self.remove_user(session_id).await {
            // Broadcast UserDisconnected
            self.broadcast_user_event(
                ServerMessage::UserDisconnected {
                    session_id,
                    nickname: user.nickname.clone(),
                },
                Some(session_id),
            )
            .await;

            // For regular accounts, check if there are remaining sessions
            // If so, broadcast UserUpdated with aggregated info (split selection:
            // latest login for avatar/locale, most recently active for away/status)
            if !user.is_shared {
                let remaining_sessions = self.get_sessions_by_username(&user.username).await;
                if let Some(user_info) = Self::build_aggregated_user_info(&remaining_sessions) {
                    self.broadcast_user_event(
                        ServerMessage::UserUpdated {
                            previous_username: user.username.clone(),
                            user: user_info,
                        },
                        Some(session_id),
                    )
                    .await;
                }
            }

            Some(user)
        } else {
            None
        }
    }

    /// Update username for a user by database user ID
    /// Returns the number of sessions updated
    ///
    /// For regular accounts, also updates nickname (since nickname == username).
    /// For shared accounts, nickname is independent and unchanged.
    pub async fn update_username(&self, user_id: i64, new_username: String) -> usize {
        let mut users = self.users.write().await;
        let mut count = 0;

        for user in users.values_mut() {
            if user.user_id == user_id {
                // For regular accounts, nickname == username, so update both
                if !user.is_shared {
                    user.nickname = new_username.clone();
                }
                user.username = new_username.clone();
                count += 1;
            }
        }

        count
    }

    /// Update cached permissions for a user by database user ID
    /// Returns the number of sessions updated
    pub async fn update_permissions(
        &self,
        user_id: i64,
        permissions: HashSet<Permission>,
    ) -> usize {
        let mut users = self.users.write().await;
        let mut count = 0;

        for user in users.values_mut() {
            if user.user_id == user_id {
                user.permissions = permissions.clone();
                count += 1;
            }
        }

        count
    }

    /// Atomically flip both `is_admin` and cached permissions for
    /// every session of `user_id` under one write-lock acquisition.
    /// `UserSession::has_permission` short-circuits on `is_admin`, so
    /// splitting the two writes would let a demoted admin keep
    /// passing privileged checks until both landed. Returns the
    /// number of sessions touched.
    pub async fn update_auth_state(
        &self,
        user_id: i64,
        is_admin: bool,
        permissions: HashSet<Permission>,
    ) -> usize {
        let mut users = self.users.write().await;
        let mut count = 0;

        for user in users.values_mut() {
            if user.user_id == user_id {
                user.is_admin = is_admin;
                user.permissions = permissions.clone();
                count += 1;
            }
        }

        count
    }

    /// Refresh the cached `bandwidth_weight` atomic for all sessions of a
    /// user. Called after `UserUpdate` (own user). For the `GroupUpdate`
    /// cascade, prefer [`Self::update_bandwidth_weight_for_user_ids`] —
    /// it batches the per-user fan-out into a single pass. Returns the
    /// number of sessions touched.
    ///
    /// `Relaxed` is sufficient — the scheduler reads this advisorily for
    /// fairness, not as a correctness invariant.
    pub async fn update_bandwidth_weight(&self, user_id: i64, weight: u16) -> usize {
        let users = self.users.read().await;
        let mut count = 0;

        for user in users.values() {
            if user.user_id == user_id {
                user.bandwidth_weight.store(weight, Ordering::Relaxed);
                count += 1;
            }
        }

        count
    }

    /// Batched companion to [`Self::update_bandwidth_weight`] —
    /// refreshes every session whose `user_id` is in `user_ids` in one
    /// pass over sessions. Used by the `GroupUpdate` cascade to avoid
    /// O(N·M) per-member scans. Returns the number of sessions
    /// touched. Read lock suffices because `bandwidth_weight` is an
    /// `AtomicU16`.
    pub async fn update_bandwidth_weight_for_user_ids(
        &self,
        user_ids: &HashSet<i64>,
        weight: u16,
    ) -> usize {
        if user_ids.is_empty() {
            return 0;
        }
        let users = self.users.read().await;
        let mut count = 0;

        for user in users.values() {
            if user_ids.contains(&user.user_id) {
                user.bandwidth_weight.store(weight, Ordering::Relaxed);
                count += 1;
            }
        }

        count
    }

    /// Update group info for all sessions of a user by database user ID
    /// Returns the number of sessions updated
    pub async fn update_group(
        &self,
        user_id: i64,
        group_id: Option<i64>,
        group_name: Option<String>,
    ) -> usize {
        let mut users = self.users.write().await;
        let mut count = 0;

        for user in users.values_mut() {
            if user.user_id == user_id {
                user.group_id = group_id;
                user.group_name = group_name.clone();
                count += 1;
            }
        }

        count
    }

    /// Update cached group name for all sessions with a specific group ID
    /// Returns the number of sessions updated
    ///
    /// Used when a group is renamed — updates the cached `group_name` on all
    /// member sessions so UserInfo broadcasts reflect the new name.
    pub async fn update_group_name(&self, group_id: i64, new_group_name: &str) -> usize {
        let mut users = self.users.write().await;
        let mut count = 0;

        for user in users.values_mut() {
            if user.group_id == Some(group_id) {
                user.group_name = Some(new_group_name.to_string());
                count += 1;
            }
        }

        count
    }

    /// Set status and away flag for a session (by session_id)
    /// Returns the updated session if found
    pub async fn set_status(
        &self,
        session_id: u32,
        is_away: bool,
        status: Option<String>,
    ) -> Option<UserSession> {
        let mut users = self.users.write().await;

        if let Some(user) = users.get_mut(&session_id) {
            user.is_away = is_away;
            user.status = status;
            Some(user.clone())
        } else {
            None
        }
    }

    /// Update last_activity timestamp for a session (for idle tracking)
    ///
    /// Called on every non-passive ClientMessage to track when the user was last active.
    pub async fn update_last_activity(&self, session_id: u32) {
        let mut users = self.users.write().await;
        if let Some(user) = users.get_mut(&session_id) {
            user.last_activity = std::time::Instant::now();
        }
    }

    /// Disconnect all sessions from a given IP address
    ///
    /// Builds a disconnect message for each session using the provided function,
    /// which receives the user's locale to generate a properly localized message.
    /// Used by the ban system to disconnect users when their IP is banned.
    ///
    /// The `skip_ip` predicate can be used to skip certain IPs (e.g., trusted IPs).
    /// If `skip_ip` returns true for an IP, sessions from that IP will NOT be disconnected.
    ///
    /// Returns information about disconnected sessions so the caller can broadcast
    /// UserDisconnected messages to update other clients' user lists.
    pub async fn disconnect_sessions_by_ip<F, S>(
        &self,
        ip: &str,
        build_message: F,
        skip_ip: S,
    ) -> Vec<DisconnectedSession>
    where
        F: Fn(&str) -> ServerMessage,
        S: Fn(&IpAddr) -> bool,
    {
        // Check if this IP should be skipped (e.g., trusted)
        if let Ok(parsed_ip) = ip.parse::<IpAddr>()
            && skip_ip(&parsed_ip)
        {
            return Vec::new();
        }

        // First, collect session IDs to disconnect
        let session_ids: Vec<u32> = {
            let users = self.users.read().await;
            users
                .values()
                .filter(|u| u.address.ip().to_string() == ip)
                .map(|u| u.session_id)
                .collect()
        };

        if session_ids.is_empty() {
            return Vec::new();
        }

        // Send disconnect message to each session and remove them
        let mut users = self.users.write().await;
        let mut disconnected = Vec::new();

        for session_id in session_ids {
            if let Some(user) = users.remove(&session_id) {
                // Build message with user's locale and send
                // (ignore send errors - channel may already be closed)
                let message = build_message(&user.locale);
                let _ = user.tx.send((message, None));
                disconnected.push(DisconnectedSession {
                    session_id,
                    nickname: user.nickname.clone(),
                });
            }
        }

        disconnected
    }

    /// Disconnect all sessions from IPs within a given CIDR range
    ///
    /// Builds a disconnect message for each session using the provided function,
    /// which receives the user's locale to generate a properly localized message.
    /// Used by the ban system to disconnect users when a CIDR range is banned.
    ///
    /// The `skip_ip` predicate can be used to skip certain IPs (e.g., trusted IPs).
    /// If `skip_ip` returns true for an IP, sessions from that IP will NOT be disconnected,
    /// even if the IP falls within the banned range.
    ///
    /// Returns information about disconnected sessions so the caller can broadcast
    /// UserDisconnected messages to update other clients' user lists.
    pub async fn disconnect_sessions_in_range<F, S>(
        &self,
        range: &IpNet,
        build_message: F,
        skip_ip: S,
    ) -> Vec<DisconnectedSession>
    where
        F: Fn(&str) -> ServerMessage,
        S: Fn(&IpAddr) -> bool,
    {
        // First, collect session IDs to disconnect (excluding skipped IPs like trusted)
        let session_ids: Vec<u32> = {
            let users = self.users.read().await;
            users
                .values()
                .filter(|u| {
                    let ip = u.address.ip();
                    range.contains(&ip) && !skip_ip(&ip)
                })
                .map(|u| u.session_id)
                .collect()
        };

        if session_ids.is_empty() {
            return Vec::new();
        }

        // Send disconnect message to each session and remove them
        let mut users = self.users.write().await;
        let mut disconnected = Vec::new();

        for session_id in session_ids {
            if let Some(user) = users.remove(&session_id) {
                // Build message with user's locale and send
                // (ignore send errors - channel may already be closed)
                let message = build_message(&user.locale);
                let _ = user.tx.send((message, None));
                disconnected.push(DisconnectedSession {
                    session_id,
                    nickname: user.nickname.clone(),
                });
            }
        }

        disconnected
    }
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use tokio::sync::mpsc;

    use super::*;

    fn shared_session_params(
        user_id: i64,
        nickname: &str,
        initial_weight: u16,
    ) -> NewSessionParams {
        let (tx, _rx) = mpsc::unbounded_channel();
        NewSessionParams {
            session_id: 0,
            user_id,
            username: "shared_acct".to_string(),
            is_admin: false,
            is_shared: true,
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
            last_activity: std::time::Instant::now(),
            bandwidth_weight: initial_weight,
        }
    }

    /// Invariant: `update_bandwidth_weight(user_id, w)` must fan out the new
    /// value to every session belonging to that user. `build_aggregated_user_info`
    /// reads the weight from a single arbitrary session ("latest_login") and
    /// trusts that all sessions of one user agree — if this invariant ever
    /// breaks, group/user-update broadcasts would emit a stale or fabricated
    /// weight on accounts with multiple connections.
    #[tokio::test]
    async fn test_update_bandwidth_weight_fans_out_to_all_sessions_of_user() {
        let manager = UserManager::new();
        const SHARED_USER_ID: i64 = 42;

        // Three sessions for the same user_id (shared account with three
        // distinct nicknames is the natural way to model this; the invariant
        // we're pinning is "same user_id ⇒ same cached weight" regardless of
        // shared/regular).
        for nick in ["alice", "bob", "carol"] {
            manager
                .add_user(shared_session_params(SHARED_USER_ID, nick, 1))
                .await
                .expect("add_user should succeed");
        }

        // Sanity: all three start at the initial weight.
        let sessions_before = manager.get_sessions_by_username("shared_acct").await;
        assert_eq!(sessions_before.len(), 3);
        for session in &sessions_before {
            assert_eq!(session.bandwidth_weight.load(Ordering::Relaxed), 1);
        }

        // One call updates every session of this user_id.
        let touched = manager.update_bandwidth_weight(SHARED_USER_ID, 99).await;
        assert_eq!(touched, 3, "all three sessions must be touched");

        let sessions_after = manager.get_sessions_by_username("shared_acct").await;
        assert_eq!(sessions_after.len(), 3);
        for session in &sessions_after {
            assert_eq!(
                session.bandwidth_weight.load(Ordering::Relaxed),
                99,
                "every session of one user_id must observe the new weight"
            );
        }
    }

    /// Companion invariant: `update_bandwidth_weight` must not bleed across
    /// users. A weight change for user A's sessions must leave user B
    /// untouched, even when both share the same UserManager.
    #[tokio::test]
    async fn test_update_bandwidth_weight_does_not_affect_other_users() {
        let manager = UserManager::new();

        manager
            .add_user(shared_session_params(1, "alice", 1))
            .await
            .unwrap();
        // Different user_id, different account — would need a fresh username
        // to satisfy is_nickname_in_use; use a regular account here.
        let (tx, _rx) = mpsc::unbounded_channel();
        manager
            .add_user(NewSessionParams {
                session_id: 0,
                user_id: 2,
                username: "bob".to_string(),
                is_admin: false,
                is_shared: false,
                permissions: HashSet::new(),
                address: "127.0.0.1:12345".parse::<SocketAddr>().unwrap(),
                created_at: 0,
                tx,
                features: vec![],
                locale: "en".to_string(),
                avatar: None,
                nickname: "bob".to_string(),
                is_away: false,
                status: None,
                group_id: None,
                group_name: None,
                last_activity: std::time::Instant::now(),
                bandwidth_weight: 1,
            })
            .await
            .unwrap();

        manager.update_bandwidth_weight(1, 50).await;

        let alice_sessions = manager.get_sessions_by_username("shared_acct").await;
        assert_eq!(alice_sessions.len(), 1);
        assert_eq!(
            alice_sessions[0].bandwidth_weight.load(Ordering::Relaxed),
            50
        );

        let bob_sessions = manager.get_sessions_by_username("bob").await;
        assert_eq!(bob_sessions.len(), 1);
        assert_eq!(
            bob_sessions[0].bandwidth_weight.load(Ordering::Relaxed),
            1,
            "weight update for user_id=1 must not touch user_id=2"
        );
    }

    /// Batched variant: a single call updates every session whose
    /// `user_id` is in the set, including multi-session users, and
    /// leaves out-of-set users alone. Empty set is a no-op.
    #[tokio::test]
    async fn test_update_bandwidth_weight_for_user_ids_batched() {
        let manager = UserManager::new();

        // user_id=1 (shared, two sessions), user_id=2 (regular, "bob"),
        // user_id=3 (regular, "carol"). Cascade set = {1, 3}.
        manager
            .add_user(shared_session_params(1, "alice1", 1))
            .await
            .unwrap();
        manager
            .add_user(shared_session_params(1, "alice2", 1))
            .await
            .unwrap();
        let (tx_b, _rx_b) = mpsc::unbounded_channel();
        manager
            .add_user(NewSessionParams {
                session_id: 0,
                user_id: 2,
                username: "bob".to_string(),
                is_admin: false,
                is_shared: false,
                permissions: HashSet::new(),
                address: "127.0.0.1:12345".parse::<SocketAddr>().unwrap(),
                created_at: 0,
                tx: tx_b,
                features: vec![],
                locale: "en".to_string(),
                avatar: None,
                nickname: "bob".to_string(),
                is_away: false,
                status: None,
                group_id: None,
                group_name: None,
                last_activity: std::time::Instant::now(),
                bandwidth_weight: 1,
            })
            .await
            .unwrap();
        let (tx_c, _rx_c) = mpsc::unbounded_channel();
        manager
            .add_user(NewSessionParams {
                session_id: 0,
                user_id: 3,
                username: "carol".to_string(),
                is_admin: false,
                is_shared: false,
                permissions: HashSet::new(),
                address: "127.0.0.1:12345".parse::<SocketAddr>().unwrap(),
                created_at: 0,
                tx: tx_c,
                features: vec![],
                locale: "en".to_string(),
                avatar: None,
                nickname: "carol".to_string(),
                is_away: false,
                status: None,
                group_id: None,
                group_name: None,
                last_activity: std::time::Instant::now(),
                bandwidth_weight: 1,
            })
            .await
            .unwrap();

        let set: HashSet<i64> = [1i64, 3].into_iter().collect();
        let touched = manager.update_bandwidth_weight_for_user_ids(&set, 77).await;
        assert_eq!(touched, 3, "two alice sessions + one carol session");

        let alice_sessions = manager.get_sessions_by_username("shared_acct").await;
        assert_eq!(alice_sessions.len(), 2);
        for s in &alice_sessions {
            assert_eq!(s.bandwidth_weight.load(Ordering::Relaxed), 77);
        }

        let carol_sessions = manager.get_sessions_by_username("carol").await;
        assert_eq!(carol_sessions.len(), 1);
        assert_eq!(
            carol_sessions[0].bandwidth_weight.load(Ordering::Relaxed),
            77
        );

        let bob_sessions = manager.get_sessions_by_username("bob").await;
        assert_eq!(bob_sessions.len(), 1);
        assert_eq!(
            bob_sessions[0].bandwidth_weight.load(Ordering::Relaxed),
            1,
            "user_id not in set must be untouched"
        );

        // Empty set is a no-op — no sessions changed, no lock contention worth taking.
        let touched_empty = manager
            .update_bandwidth_weight_for_user_ids(&HashSet::new(), 99)
            .await;
        assert_eq!(touched_empty, 0);
        let bob_after_empty = manager.get_sessions_by_username("bob").await;
        assert_eq!(
            bob_after_empty[0].bandwidth_weight.load(Ordering::Relaxed),
            1
        );
    }
}
