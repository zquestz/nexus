//! Helper methods for UserManager

use super::UserManager;
use crate::db::{Permission, UserDb};
use nexus_common::protocol::ServerMessage;

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
    pub(super) async fn remove_disconnected(&self, session_ids: Vec<u32>, _user_db: &UserDb) {
        if session_ids.is_empty() {
            return;
        }

        // Collect user info before removing them
        let users_to_remove: Vec<(u32, String)> = {
            let users = self.users.read().await;
            session_ids
                .iter()
                .filter_map(|&session_id| {
                    users
                        .get(&session_id)
                        .map(|user| (session_id, user.username.clone()))
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
        for (session_id, username) in users_to_remove {
            let message = ServerMessage::UserDisconnected {
                session_id,
                username,
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
                    let _ = user.tx.send(message.clone());
                }
            }
        }
    }
}
