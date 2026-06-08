//! Helper methods for UserManager

use std::collections::HashMap;
use std::sync::atomic::Ordering;

use nexus_common::protocol::{ServerMessage, UserInfo};

use super::UserManager;
use crate::constants::ERR_SESSIONS_NOT_EMPTY;
use crate::db::Permission;
use crate::users::user::UserSession;

impl UserManager {
    /// Remove sessions whose channels have closed, then notify remaining
    /// `user_list` clients: a `UserDisconnected` per removed session, plus one
    /// re-aggregated `UserUpdated` per affected regular account (so a surviving
    /// session's aggregate doesn't go stale — the same fix as
    /// `broadcast_disconnections`). Called by the broadcast methods; sends
    /// directly rather than via `broadcast_user_event()` to break the type-level
    /// recursion (broadcast → remove_disconnected → broadcast).
    pub(super) async fn remove_disconnected(&self, session_ids: Vec<u32>) {
        if session_ids.is_empty() {
            return;
        }

        // Remove and capture in one write-lock pass: `remove` yields the session only
        // if it was still in the map, so we broadcast exactly what *this* call removed.
        // A session another path already swept (and announced) returns None here and is
        // skipped — no duplicate UserDisconnected. The re-aggregate below needs the full
        // session (username, is_shared, avatar), not just the id.
        let removed_sessions: Vec<UserSession> = {
            let mut users = self.users.write().await;
            session_ids
                .iter()
                .filter_map(|session_id| users.remove(session_id))
                .collect()
        };

        // Notify user_list holders of each disconnect.
        for user_session in &removed_sessions {
            let message = ServerMessage::UserDisconnected {
                session_id: user_session.session_id,
                nickname: user_session.nickname.clone(),
            };
            self.direct_send_to_user_list(&message).await;
        }

        // Re-aggregate each affected regular account once (same as
        // broadcast_disconnections), direct-sent for the recursion reason above.
        // Shared sessions never re-aggregate — each is its own user-list entry.
        let mut by_user_id: HashMap<i64, Vec<UserSession>> = HashMap::new();
        for user_session in removed_sessions.iter().filter(|u| !u.is_shared) {
            by_user_id
                .entry(user_session.user_id)
                .or_default()
                .push(user_session.clone());
        }

        for account_removed_sessions in by_user_id.into_values() {
            if let Some(message) = self
                .build_account_reaggregate(
                    account_removed_sessions[0].user_id,
                    &account_removed_sessions,
                )
                .await
            {
                self.direct_send_to_user_list(&message).await;
            }
        }
    }

    /// Direct-send `message` to every connected `user_list` holder, ignoring send
    /// errors. The cleanup sweep uses this instead of `broadcast_user_event` to
    /// avoid the broadcast → remove_disconnected → broadcast recursion.
    async fn direct_send_to_user_list(&self, message: &ServerMessage) {
        let users = self.users.read().await;
        for user_session in users.values() {
            // Cached permissions, admin bypass.
            if user_session.has_permission(Permission::UserList) {
                // Ignore send errors: a closed channel here is cleaned up on the
                // next broadcast. We don't recurse.
                let _ = user_session.tx.send_message(message.clone(), None);
            }
        }
    }

    /// Build UserInfo from a single session (shared accounts). Each session has
    /// a unique nickname, so they're broadcast separately without aggregation.
    pub fn build_user_info_from_session(session: &UserSession) -> UserInfo {
        UserInfo {
            id: session.user_id,
            username: session.username.clone(),
            nickname: session.nickname.clone(),
            login_time: session.login_time,
            is_admin: session.is_admin,
            is_shared: session.is_shared,
            session_ids: vec![session.session_id],
            locale: session.locale.clone(),
            // Avatar left None; the caller sets it (None for UserUpdated, the
            // session's own avatar for UserListResponse).
            avatar: None,
            is_away: session.is_away,
            status: session.status.clone(),
            group_id: session.group_id,
            group_name: session.group_name.clone(),
            bandwidth_weight: session.bandwidth_weight.load(Ordering::Relaxed),
        }
    }

    /// The avatar shown for a regular account: the most recent login (max
    /// `login_time`) among sessions that actually carry one, or `None` if none
    /// do. A no-avatar login never clears an existing avatar; a newer login
    /// with one wins. Single source of truth for snapshots and the disconnect
    /// `UserUpdated` override. Not for shared accounts (those are per-session).
    pub fn aggregate_avatar<'a>(sessions: impl Iterator<Item = &'a UserSession>) -> Option<String> {
        sessions
            .filter(|s| s.avatar.is_some())
            .max_by_key(|s| (s.login_time, s.session_id))
            .and_then(|s| s.avatar.clone())
    }

    /// Aggregate the multiple sessions of one regular account into a single
    /// UserInfo. `avatar` is left `None`; the **caller** sets it — `None` for a
    /// `UserUpdated` delta (clients keep their cache), or `aggregate_avatar` for
    /// a snapshot (`UserListResponse`) or the disconnect override. Other field
    /// sources: identity (username/locale/group) from the latest login (stable,
    /// no flicker); login_time = earliest (for "connected since"); is_away/status
    /// = most recently active (accurate presence).
    /// Not for shared accounts — each of their sessions is its own entry.
    pub fn build_aggregated_user_info(sessions: &[UserSession]) -> Option<UserInfo> {
        if sessions.is_empty() {
            return None;
        }

        let latest_login = sessions
            .iter()
            .max_by_key(|s| (s.login_time, s.session_id))
            .expect(ERR_SESSIONS_NOT_EMPTY);

        let most_active = sessions
            .iter()
            .max_by_key(|s| s.last_activity)
            .expect(ERR_SESSIONS_NOT_EMPTY);

        let earliest_login_time = sessions
            .iter()
            .map(|s| s.login_time)
            .min()
            .expect(ERR_SESSIONS_NOT_EMPTY);

        let session_ids: Vec<u32> = sessions.iter().map(|s| s.session_id).collect();

        Some(UserInfo {
            id: latest_login.user_id,
            username: latest_login.username.clone(),
            nickname: latest_login.nickname.clone(), // == username for regular accounts
            login_time: earliest_login_time,
            is_admin: latest_login.is_admin,
            is_shared: latest_login.is_shared,
            session_ids,
            locale: latest_login.locale.clone(),
            // Avatar left None; the caller sets it (None for UserUpdated,
            // aggregate_avatar for snapshots / the disconnect override).
            avatar: None,
            is_away: most_active.is_away,
            status: most_active.status.clone(),
            group_id: latest_login.group_id,
            group_name: latest_login.group_name.clone(),
            // All sessions of one regular user share the same cached weight; reading
            // from latest_login is canonical.
            bandwidth_weight: latest_login.bandwidth_weight.load(Ordering::Relaxed),
        })
    }

    /// Build the post-removal aggregated `UserUpdated` for a regular account, or
    /// `None` if no sessions remain. `removed_sessions` is every session just
    /// removed for this account, so the avatar delta's "old" reflects all
    /// departing avatar sources, not just one.
    ///
    /// Build-only: the caller delivers the message. The normal remove paths
    /// broadcast it; the dead-channel cleanup sweep direct-sends it (broadcasting
    /// from inside a broadcast would re-enter the broadcast → cleanup → broadcast
    /// recursion). Regular accounts only — shared sessions are per-session
    /// user-list entries, not aggregated.
    pub(super) async fn build_account_reaggregate(
        &self,
        user_id: i64,
        removed_sessions: &[UserSession],
    ) -> Option<ServerMessage> {
        let remaining_sessions = self.get_sessions_by_user_id(user_id).await;
        let mut user_info = Self::build_aggregated_user_info(&remaining_sessions)?;

        // UserUpdated normally omits the avatar (unchanged). A disconnect is the one
        // case where the aggregate can change (the avatar-bearing latest session
        // left), so carry the new value when it differs; `Some("")` signals "now no
        // avatar". "old" spans every removed session so a multi-session removal
        // can't miss the change.
        let new_avatar = Self::aggregate_avatar(remaining_sessions.iter());
        let old_avatar =
            Self::aggregate_avatar(remaining_sessions.iter().chain(removed_sessions.iter()));
        if new_avatar != old_avatar {
            user_info.avatar = Some(new_avatar.unwrap_or_default());
        }

        Some(ServerMessage::UserUpdated {
            // Not a rename: survivors keep their current name, so previous == new
            // and the client's rename bookkeeping is a no-op. (The client matches
            // regular accounts by immutable id, not this field.)
            previous_username: user_info.username.clone(),
            user: user_info,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::net::SocketAddr;

    use super::*;
    use crate::users::user::{ConnectionWriter, NewSessionParams};

    fn session_params(
        user_id: i64,
        username: &str,
        nickname: &str,
        is_shared: bool,
        is_admin: bool,
        avatar: Option<String>,
        tx: ConnectionWriter,
    ) -> NewSessionParams {
        NewSessionParams {
            session_id: 0,
            user_id,
            username: username.to_string(),
            is_admin,
            is_shared,
            permissions: HashSet::new(),
            address: "127.0.0.1:12345".parse::<SocketAddr>().unwrap(),
            created_at: 0,
            tx,
            features: vec![],
            locale: "en".to_string(),
            avatar,
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

    /// A dead-channel sweep that removes one session of a multi-session regular
    /// account direct-sends UserDisconnected for it and a re-aggregated
    /// UserUpdated for the survivor (the avatar source left → clears to Some("")).
    #[tokio::test]
    async fn test_remove_disconnected_reaggregates_surviving_session() {
        let manager = UserManager::new();

        // Observer (admin → has user_list) to capture the direct sends.
        let (obs_tx, mut obs_rx) = ConnectionWriter::channel();
        manager
            .add_user(session_params(
                100, "observer", "observer", false, true, None, obs_tx,
            ))
            .await
            .unwrap();

        // alice: s1 carries the avatar and is swept; s2 (no avatar) survives.
        let (a1_tx, _a1_rx) = ConnectionWriter::channel();
        let s1 = manager
            .add_user(session_params(
                1,
                "alice",
                "alice",
                false,
                false,
                Some("data:avatar-x".to_string()),
                a1_tx,
            ))
            .await
            .unwrap();
        let (a2_tx, _a2_rx) = ConnectionWriter::channel();
        let s2 = manager
            .add_user(session_params(
                1, "alice", "alice", false, false, None, a2_tx,
            ))
            .await
            .unwrap();

        manager.remove_disconnected(vec![s1]).await;

        let (msg1, _) = obs_rx
            .try_recv()
            .expect("observer should receive UserDisconnected")
            .expect_message();
        match msg1 {
            ServerMessage::UserDisconnected { session_id, .. } => assert_eq!(session_id, s1),
            other => panic!("expected UserDisconnected, got {other:?}"),
        }

        let (msg2, _) = obs_rx
            .try_recv()
            .expect("observer should receive re-aggregated UserUpdated")
            .expect_message();
        match msg2 {
            ServerMessage::UserUpdated {
                previous_username,
                user,
            } => {
                assert_eq!(previous_username, "alice");
                assert_eq!(user.session_ids, vec![s2]);
                assert_eq!(user.avatar, Some(String::new()));
            }
            other => panic!("expected UserUpdated, got {other:?}"),
        }

        assert!(obs_rx.try_recv().is_err(), "no further messages");
        assert_eq!(manager.get_sessions_by_username("alice").await.len(), 1);
    }

    /// Shared sessions never re-aggregate: the sweep emits only UserDisconnected.
    #[tokio::test]
    async fn test_remove_disconnected_shared_no_user_updated() {
        let manager = UserManager::new();

        let (obs_tx, mut obs_rx) = ConnectionWriter::channel();
        manager
            .add_user(session_params(
                100, "observer", "observer", false, true, None, obs_tx,
            ))
            .await
            .unwrap();

        // Shared account: two sessions, distinct nicknames, one bears an avatar.
        let (g1_tx, _g1_rx) = ConnectionWriter::channel();
        let g1 = manager
            .add_user(session_params(
                2,
                "shared_acct",
                "guest1",
                true,
                false,
                Some("data:avatar-x".to_string()),
                g1_tx,
            ))
            .await
            .unwrap();
        let (g2_tx, _g2_rx) = ConnectionWriter::channel();
        manager
            .add_user(session_params(
                2,
                "shared_acct",
                "guest2",
                true,
                false,
                None,
                g2_tx,
            ))
            .await
            .unwrap();

        manager.remove_disconnected(vec![g1]).await;

        let (msg, _) = obs_rx
            .try_recv()
            .expect("observer should receive UserDisconnected")
            .expect_message();
        match msg {
            ServerMessage::UserDisconnected {
                session_id,
                nickname,
            } => {
                assert_eq!(session_id, g1);
                assert_eq!(nickname, "guest1");
            }
            other => panic!("expected UserDisconnected, got {other:?}"),
        }
        assert!(
            obs_rx.try_recv().is_err(),
            "shared cleanup must not emit UserUpdated"
        );
    }

    /// `login_time` is second-resolution, so two sessions can tie; the aggregate
    /// must pick a stable winner (higher `session_id`) rather than letting HashMap
    /// iteration order decide which avatar/identity wins.
    #[tokio::test]
    async fn test_aggregate_avatar_breaks_login_time_ties_by_session_id() {
        use crate::users::user::UserSession;

        let (tx_a, _rx_a) = ConnectionWriter::channel();
        let mut lower = UserSession::new(session_params(
            1,
            "alice",
            "alice",
            false,
            false,
            Some("data:lower".to_string()),
            tx_a,
        ));
        lower.session_id = 10;
        lower.login_time = 100;

        let (tx_b, _rx_b) = ConnectionWriter::channel();
        let mut higher = UserSession::new(session_params(
            1,
            "alice",
            "alice",
            false,
            false,
            Some("data:higher".to_string()),
            tx_b,
        ));
        higher.session_id = 20;
        higher.login_time = 100; // same second as `lower` → tie on login_time alone

        // Higher session_id wins the tie, regardless of iteration order.
        let forward = [lower.clone(), higher.clone()];
        assert_eq!(
            UserManager::aggregate_avatar(forward.iter()),
            Some("data:higher".to_string())
        );
        let reverse = [higher, lower];
        assert_eq!(
            UserManager::aggregate_avatar(reverse.iter()),
            Some("data:higher".to_string())
        );
    }
}
