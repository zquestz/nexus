//! Mutation methods for UserManager

use std::collections::HashSet;
use std::net::IpAddr;
use std::sync::atomic::Ordering;

use ipnet::IpNet;
use nexus_common::protocol::ServerMessage;

use super::UserManager;
use crate::db::Permission;
use crate::users::user::{NewSessionParams, UserSession};

/// A disconnected session, carried back so the caller can broadcast UserDisconnected.
pub struct DisconnectedSession {
    pub session_id: u32,
    pub nickname: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddUserError {
    /// The requested nickname is already in use by another session.
    NicknameInUse,
}

impl UserManager {
    /// Add a new user and return their assigned session ID.
    ///
    /// For shared accounts, atomically rechecks nickname uniqueness while holding
    /// the write lock. The login handler's non-atomic `is_nickname_in_use()` pre-check
    /// catches most conflicts cheaply; this lock-held check closes the race where two
    /// simultaneous logins both pass the pre-check.
    pub async fn add_user(&self, mut params: NewSessionParams) -> Result<u32, AddUserError> {
        let mut users = self.users.write().await;

        if params.is_shared {
            let nickname_lower = params.nickname.to_lowercase();

            for user in users.values() {
                if user.nickname.to_lowercase() == nickname_lower {
                    return Err(AddUserError::NicknameInUse);
                }
            }
        }

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

    /// Remove a user and broadcast `UserDisconnected` to all users with `user_list`.
    /// For normal disconnects, kicks, account deletion, and disable.
    ///
    /// For ban disconnects use `disconnect_sessions_by_ip()` /
    /// `disconnect_sessions_in_range()` instead — those send a custom message to the
    /// disconnected user before removing them.
    pub async fn remove_user_and_broadcast(&self, session_id: u32) -> Option<UserSession> {
        if let Some(user) = self.remove_user(session_id).await {
            self.broadcast_user_event(
                ServerMessage::UserDisconnected {
                    session_id,
                    nickname: user.nickname.clone(),
                },
                Some(session_id),
            )
            .await;

            // Regular account with sessions still online: rebroadcast aggregated info
            // (latest login for avatar/locale, most recently active for away/status).
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

    /// Update username for all sessions of a user, returning the count updated.
    ///
    /// Regular accounts also get their nickname updated (nickname == username);
    /// shared accounts keep their independent nickname.
    pub async fn update_username(&self, user_id: i64, new_username: String) -> usize {
        let mut users = self.users.write().await;
        let mut count = 0;

        for user in users.values_mut() {
            if user.user_id == user_id {
                if !user.is_shared {
                    user.nickname = new_username.clone();
                }
                user.username = new_username.clone();
                count += 1;
            }
        }

        count
    }

    /// Update cached permissions for all sessions of a user, returning the count updated.
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

    /// Atomically flip `is_admin` and cached permissions for every session of
    /// `user_id` under one write-lock acquisition, returning the count touched.
    /// `has_permission` short-circuits on `is_admin`, so splitting the two writes
    /// would let a demoted admin keep passing privileged checks until both landed.
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

    /// Writes override + resolved weight under one write-lock acquisition so a
    /// concurrent reader (the group cascade) sees the old pair or the new pair,
    /// never a half-update where the weight reflects a new override but the marker doesn't.
    pub async fn update_bandwidth_state(
        &self,
        user_id: i64,
        override_value: Option<u16>,
        resolved: u16,
    ) -> usize {
        let mut users = self.users.write().await;
        let mut count = 0;

        for user in users.values_mut() {
            if user.user_id == user_id {
                user.bandwidth_weight_override = override_value;
                user.bandwidth_weight.store(resolved, Ordering::Relaxed);
                count += 1;
            }
        }

        count
    }

    /// Applies a group's weight only to members without a per-user override, and
    /// returns the touched `user_id` set so callers scope `UserUpdated` broadcasts
    /// to users whose visible weight actually changed.
    pub async fn update_bandwidth_weight_for_group_inheritors(
        &self,
        group_id: i64,
        weight: u16,
    ) -> HashSet<i64> {
        let mut users = self.users.write().await;
        let mut touched: HashSet<i64> = HashSet::new();

        for user in users.values_mut() {
            if user.group_id == Some(group_id) && user.bandwidth_weight_override.is_none() {
                user.bandwidth_weight.store(weight, Ordering::Relaxed);
                touched.insert(user.user_id);
            }
        }

        touched
    }

    /// Update cached group_id/group_name for all sessions of a user, returning the count updated.
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

    /// Refresh cached `group_name` on all sessions in a group (on group rename), so
    /// later UserInfo broadcasts reflect the new name. Returns the count updated.
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

    /// Set status and away flag for a session, returning the updated session if found.
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

    /// Stamp a session's `last_activity` for idle tracking; called on every
    /// non-passive ClientMessage.
    pub async fn update_last_activity(&self, session_id: u32) {
        let mut users = self.users.write().await;
        if let Some(user) = users.get_mut(&session_id) {
            user.last_activity = std::time::Instant::now();
        }
    }

    /// Disconnect all sessions from an IP (ban system). `build_message` is given each
    /// user's locale for a localized goodbye. `skip_ip` returning true exempts an IP
    /// (e.g. trusted). Returns the disconnected sessions so the caller can broadcast
    /// UserDisconnected.
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
        if let Ok(parsed_ip) = ip.parse::<IpAddr>()
            && skip_ip(&parsed_ip)
        {
            return Vec::new();
        }

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

        let mut users = self.users.write().await;
        let mut disconnected = Vec::new();

        for session_id in session_ids {
            if let Some(user) = users.remove(&session_id) {
                // Ignore send errors — the channel may already be closed.
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

    /// Disconnect all sessions whose IP falls in a CIDR range (ban system). Same
    /// `build_message` / `skip_ip` semantics as `disconnect_sessions_by_ip`; a skipped
    /// IP is exempt even when inside the range. Returns the disconnected sessions for
    /// UserDisconnected broadcasts.
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

        let mut users = self.users.write().await;
        let mut disconnected = Vec::new();

        for session_id in session_ids {
            if let Some(user) = users.remove(&session_id) {
                // Ignore send errors — the channel may already be closed.
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
            bandwidth_weight_override: None,
            last_activity: std::time::Instant::now(),
            bandwidth_weight: initial_weight,
        }
    }

    /// `update_bandwidth_state` must fan out both fields (override + resolved) to
    /// every session of the user. `build_aggregated_user_info` reads one arbitrary
    /// session and trusts all sessions agree; a break would emit stale/fabricated
    /// weights on multi-connection accounts.
    #[tokio::test]
    async fn test_update_bandwidth_state_fans_out_to_all_sessions_of_user() {
        let manager = UserManager::new();
        const SHARED_USER_ID: i64 = 42;

        for nick in ["alice", "bob", "carol"] {
            manager
                .add_user(shared_session_params(SHARED_USER_ID, nick, 1))
                .await
                .expect("add_user should succeed");
        }

        let sessions_before = manager.get_sessions_by_username("shared_acct").await;
        assert_eq!(sessions_before.len(), 3);
        for session in &sessions_before {
            assert_eq!(session.bandwidth_weight.load(Ordering::Relaxed), 1);
            assert_eq!(session.bandwidth_weight_override, None);
        }

        let touched = manager
            .update_bandwidth_state(SHARED_USER_ID, Some(99), 99)
            .await;
        assert_eq!(touched, 3, "all three sessions must be touched");

        let sessions_after = manager.get_sessions_by_username("shared_acct").await;
        assert_eq!(sessions_after.len(), 3);
        for session in &sessions_after {
            assert_eq!(session.bandwidth_weight.load(Ordering::Relaxed), 99);
            assert_eq!(
                session.bandwidth_weight_override,
                Some(99),
                "override field must move in lockstep with resolved"
            );
        }
    }

    /// `update_bandwidth_state` must not bleed across users: changing user A's
    /// weight leaves user B untouched within the same UserManager.
    #[tokio::test]
    async fn test_update_bandwidth_state_does_not_affect_other_users() {
        let manager = UserManager::new();

        manager
            .add_user(shared_session_params(1, "alice", 1))
            .await
            .unwrap();
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
                bandwidth_weight_override: None,
                last_activity: std::time::Instant::now(),
                bandwidth_weight: 1,
            })
            .await
            .unwrap();

        manager.update_bandwidth_state(1, Some(50), 50).await;

        let alice_sessions = manager.get_sessions_by_username("shared_acct").await;
        assert_eq!(alice_sessions.len(), 1);
        assert_eq!(
            alice_sessions[0].bandwidth_weight.load(Ordering::Relaxed),
            50
        );
        assert_eq!(alice_sessions[0].bandwidth_weight_override, Some(50));

        let bob_sessions = manager.get_sessions_by_username("bob").await;
        assert_eq!(bob_sessions.len(), 1);
        assert_eq!(
            bob_sessions[0].bandwidth_weight.load(Ordering::Relaxed),
            1,
            "weight update for user_id=1 must not touch user_id=2"
        );
        assert_eq!(bob_sessions[0].bandwidth_weight_override, None);
    }

    /// Helper: regular (non-shared) session with explicit group + override.
    fn regular_session_params(
        user_id: i64,
        username: &str,
        group_id: Option<i64>,
        bandwidth_weight_override: Option<u16>,
        resolved_weight: u16,
    ) -> NewSessionParams {
        let (tx, _rx) = mpsc::unbounded_channel();
        NewSessionParams {
            session_id: 0,
            user_id,
            username: username.to_string(),
            is_admin: false,
            is_shared: false,
            permissions: HashSet::new(),
            address: "127.0.0.1:12345".parse::<SocketAddr>().unwrap(),
            created_at: 0,
            tx,
            features: vec![],
            locale: "en".to_string(),
            avatar: None,
            nickname: username.to_string(),
            is_away: false,
            status: None,
            group_id,
            group_name: None,
            bandwidth_weight_override,
            last_activity: std::time::Instant::now(),
            bandwidth_weight: resolved_weight,
        }
    }

    /// Cascade invariant: sessions with an override are skipped even when their
    /// group_id matches — closes the cascade-vs-update race where a group fan-out
    /// could clobber a freshly-set per-user override.
    #[tokio::test]
    async fn test_update_bandwidth_weight_for_group_inheritors_skips_override_holders() {
        let manager = UserManager::new();

        // alice (user 1) inherits from group 7; bob (user 2) is in
        // group 7 but has an override; carol (user 3) is in a
        // different group.
        manager
            .add_user(regular_session_params(1, "alice", Some(7), None, 10))
            .await
            .unwrap();
        manager
            .add_user(regular_session_params(2, "bob", Some(7), Some(200), 200))
            .await
            .unwrap();
        manager
            .add_user(regular_session_params(3, "carol", Some(8), None, 5))
            .await
            .unwrap();

        let touched = manager
            .update_bandwidth_weight_for_group_inheritors(7, 50)
            .await;

        assert_eq!(
            touched,
            HashSet::from([1]),
            "only alice (inheritor of group 7) should be touched"
        );

        let alice = &manager.get_sessions_by_username("alice").await[0];
        assert_eq!(
            alice.bandwidth_weight.load(Ordering::Relaxed),
            50,
            "alice inherits from group 7, gets the new group weight"
        );

        let bob = &manager.get_sessions_by_username("bob").await[0];
        assert_eq!(
            bob.bandwidth_weight.load(Ordering::Relaxed),
            200,
            "bob has an override, must be skipped by group cascade"
        );

        let carol = &manager.get_sessions_by_username("carol").await[0];
        assert_eq!(
            carol.bandwidth_weight.load(Ordering::Relaxed),
            5,
            "carol is in a different group, must be untouched"
        );
    }

    /// All inheritors get touched, returned set matches.
    #[tokio::test]
    async fn test_update_bandwidth_weight_for_group_inheritors_returns_touched_set() {
        let manager = UserManager::new();

        manager
            .add_user(regular_session_params(1, "alice", Some(7), None, 10))
            .await
            .unwrap();
        manager
            .add_user(regular_session_params(2, "bob", Some(7), None, 10))
            .await
            .unwrap();
        manager
            .add_user(regular_session_params(3, "carol", Some(8), None, 5))
            .await
            .unwrap();

        let touched = manager
            .update_bandwidth_weight_for_group_inheritors(7, 30)
            .await;

        assert_eq!(touched, HashSet::from([1, 2]));
    }

    /// Empty result for a group with no online inheritors.
    #[tokio::test]
    async fn test_update_bandwidth_weight_for_group_inheritors_empty_for_unrelated_group() {
        let manager = UserManager::new();

        manager
            .add_user(regular_session_params(1, "alice", Some(7), None, 10))
            .await
            .unwrap();

        let touched = manager
            .update_bandwidth_weight_for_group_inheritors(99, 50)
            .await;

        assert!(touched.is_empty());

        let alice = &manager.get_sessions_by_username("alice").await[0];
        assert_eq!(alice.bandwidth_weight.load(Ordering::Relaxed), 10);
    }

    /// `get_sessions_by_user_id` returns exactly that user's sessions regardless of
    /// username — the cleanup primitive for `user_delete`, where username may be stale
    /// if another admin renamed the target between pre-tx fetch and cleanup sweep.
    #[tokio::test]
    async fn test_get_sessions_by_user_id() {
        let manager = UserManager::new();

        // user_id=1 (shared, two sessions), user_id=2 (regular, "bob").
        manager
            .add_user(shared_session_params(1, "alice1", 1))
            .await
            .unwrap();
        manager
            .add_user(shared_session_params(1, "alice2", 1))
            .await
            .unwrap();
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
                bandwidth_weight_override: None,
                last_activity: std::time::Instant::now(),
                bandwidth_weight: 1,
            })
            .await
            .unwrap();

        assert_eq!(manager.get_sessions_by_user_id(1).await.len(), 2);
        assert_eq!(manager.get_sessions_by_user_id(2).await.len(), 1);
        assert!(manager.get_sessions_by_user_id(999).await.is_empty());
    }
}
