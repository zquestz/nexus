//! Helper methods for UserManager

use nexus_common::protocol::{ServerMessage, UserInfo};

use super::UserManager;
use crate::db::Permission;
use crate::users::user::UserSession;

impl UserManager {
    /// Remove disconnected users from the manager with permission checking
    ///
    /// Takes a list of session IDs whose channels have closed and removes them
    /// from the UserManager. This is called by broadcast methods when they detect
    /// that a user's channel has been closed.
    ///
    /// This method broadcasts UserDisconnected messages to all remaining clients
    /// who have the user_list permission so their user lists stay in sync.
    /// We send messages directly to avoid infinite recursion (since broadcast() calls remove_disconnected()).
    pub(super) async fn remove_disconnected(&self, session_ids: Vec<u32>) {
        if session_ids.is_empty() {
            return;
        }

        // Collect user info before removing them (nickname for proper identification)
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

        // Remove users from the manager
        {
            let mut users = self.users.write().await;
            for session_id in &session_ids {
                users.remove(session_id);
            }
        }

        // Broadcast disconnection to all remaining clients who have user_list permission
        // We send directly instead of using broadcast_user_event() to avoid infinite recursion
        // at the type level (even though runtime would be safe since users are already removed)
        for (session_id, nickname) in users_to_remove {
            let message = ServerMessage::UserDisconnected {
                session_id,
                nickname,
            };

            // Send to users who have user_list permission (ignore send errors)
            let users = self.users.read().await;
            for user in users.values() {
                // Skip the disconnecting user (already removed, but be explicit)
                if user.session_id == session_id {
                    continue;
                }

                // Check if user has user_list permission (uses cached permissions, admin bypass)
                if user.has_permission(Permission::UserList) {
                    // Ignore send errors - if this user's channel is also closed, they'll be
                    // cleaned up on the next broadcast. We don't recurse here to avoid complexity.
                    let _ = user.tx.send((message.clone(), None));
                }
            }
        }
    }

    /// Build UserInfo from a single session (for shared accounts)
    ///
    /// Shared accounts have unique nicknames per session, so each session is
    /// broadcast separately without aggregation.
    pub fn build_user_info_from_session(session: &UserSession) -> UserInfo {
        UserInfo {
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
        }
    }

    /// Build aggregated UserInfo for a regular account using "latest login wins" for avatar/away/status
    ///
    /// For regular accounts with multiple sessions, we need to aggregate data:
    /// - username, is_admin, is_shared: same for all sessions
    /// - nickname: equals username for regular accounts
    /// - login_time: earliest session's login time (for "connected since" display)
    /// - session_ids: all session IDs
    /// - locale: from latest session
    /// - avatar, is_away, status: from latest session ("latest login wins")
    ///
    /// For shared accounts (is_shared=true), this method should NOT be used - each session
    /// is a separate entry with its own nickname.
    pub fn build_aggregated_user_info(sessions: &[UserSession]) -> Option<UserInfo> {
        if sessions.is_empty() {
            return None;
        }

        // Find the session with the latest login time
        let latest_session = sessions
            .iter()
            .max_by_key(|s| s.login_time)
            .expect("sessions is not empty");

        // Find the earliest login time for display
        let earliest_login_time = sessions
            .iter()
            .map(|s| s.login_time)
            .min()
            .expect("sessions is not empty");

        // Collect all session IDs
        let session_ids: Vec<u32> = sessions.iter().map(|s| s.session_id).collect();

        Some(UserInfo {
            username: latest_session.username.clone(),
            nickname: latest_session.nickname.clone(), // For regular accounts, nickname == username
            login_time: earliest_login_time,
            is_admin: latest_session.is_admin,
            is_shared: latest_session.is_shared,
            session_ids,
            locale: latest_session.locale.clone(),
            avatar: latest_session.avatar.clone(),
            is_away: latest_session.is_away,
            status: latest_session.status.clone(),
        })
    }
}
