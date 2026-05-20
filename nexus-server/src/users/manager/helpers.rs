//! Helper methods for UserManager

use std::sync::atomic::Ordering;

use nexus_common::protocol::{ServerMessage, UserInfo};

use super::UserManager;
use crate::constants::ERR_SESSIONS_NOT_EMPTY;
use crate::db::Permission;
use crate::users::user::UserSession;

impl UserManager {
    /// Remove sessions whose channels have closed, then notify remaining
    /// user_list clients with UserDisconnected so their lists stay in sync.
    /// Called by the broadcast methods. Sends directly rather than via
    /// `broadcast_user_event()` to break the type-level recursion (broadcast →
    /// remove_disconnected → broadcast).
    pub(super) async fn remove_disconnected(&self, session_ids: Vec<u32>) {
        if session_ids.is_empty() {
            return;
        }

        // Capture nicknames before removal (needed for the notification).
        let users_to_remove: Vec<(u32, String)> = {
            let users = self.users.read().await;
            session_ids
                .iter()
                .filter_map(|&session_id| {
                    users
                        .get(&session_id)
                        .map(|user| (session_id, user.nickname.clone()))
                })
                .collect()
        };

        {
            let mut users = self.users.write().await;
            for session_id in &session_ids {
                users.remove(session_id);
            }
        }

        for (session_id, nickname) in users_to_remove {
            let message = ServerMessage::UserDisconnected {
                session_id,
                nickname,
            };

            let users = self.users.read().await;
            for user in users.values() {
                if user.session_id == session_id {
                    continue;
                }

                // Cached permissions, admin bypass.
                if user.has_permission(Permission::UserList) {
                    // Ignore send errors: a closed channel here is cleaned up on
                    // the next broadcast. We don't recurse.
                    let _ = user.tx.send((message.clone(), None));
                }
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
            avatar: session.avatar.clone(),
            is_away: session.is_away,
            status: session.status.clone(),
            group_id: session.group_id,
            group_name: session.group_name.clone(),
            bandwidth_weight: Some(session.bandwidth_weight.load(Ordering::Relaxed)),
        }
    }

    /// Aggregate the multiple sessions of one regular account into a single
    /// UserInfo. Field sources: identity (username/avatar/locale/group) from the
    /// latest login (stable, no flicker); login_time = earliest (for "connected
    /// since"); is_away/status = most recently active (accurate presence).
    /// Not for shared accounts — each of their sessions is its own entry.
    pub fn build_aggregated_user_info(sessions: &[UserSession]) -> Option<UserInfo> {
        if sessions.is_empty() {
            return None;
        }

        let latest_login = sessions
            .iter()
            .max_by_key(|s| s.login_time)
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
            avatar: latest_login.avatar.clone(),
            is_away: most_active.is_away,
            status: most_active.status.clone(),
            group_id: latest_login.group_id,
            group_name: latest_login.group_name.clone(),
            // All sessions of one regular user share the same cached weight; reading
            // from latest_login is canonical.
            bandwidth_weight: Some(latest_login.bandwidth_weight.load(Ordering::Relaxed)),
        })
    }
}
