//! User connection/disconnection handlers

use crate::NexusApp;
use crate::avatar::{compute_avatar_hash, get_or_create_avatar};
use crate::handlers::network::helpers::sort_user_list;
use crate::i18n::t_args;
use crate::types::{ChatMessage, Message, UserInfo as ClientUserInfo};
use iced::Task;
use nexus_common::protocol::UserInfo as ProtocolUserInfo;

impl NexusApp {
    /// Handle user connected notification
    pub fn handle_user_connected(
        &mut self,
        connection_id: usize,
        user: ProtocolUserInfo,
    ) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        // Compute hash of incoming avatar for comparison
        let new_avatar_hash = compute_avatar_hash(user.avatar.as_deref());

        // Check if user already exists (multi-device connection) and update accordingly
        let is_new_user = if let Some(existing_user) = conn
            .online_users
            .iter_mut()
            .find(|u| u.username == user.username)
        {
            // User already exists - merge session_ids
            for session_id in &user.session_ids {
                if !existing_user.session_ids.contains(session_id) {
                    existing_user.session_ids.push(*session_id);
                }
            }

            // Update avatar if it changed (latest login wins, including clearing avatar)
            if existing_user.avatar_hash != new_avatar_hash {
                existing_user.avatar_hash = new_avatar_hash;
                // Invalidate cache so new avatar (or identicon) is displayed
                conn.avatar_cache.remove(&user.username);
                get_or_create_avatar(
                    &mut conn.avatar_cache,
                    &user.username,
                    user.avatar.as_deref(),
                );
            }

            false
        } else {
            // New user - add to list
            conn.online_users.push(ClientUserInfo {
                username: user.username.clone(),
                is_admin: user.is_admin,
                session_ids: user.session_ids.clone(),
                avatar_hash: new_avatar_hash,
            });
            sort_user_list(&mut conn.online_users);

            // Pre-populate avatar cache for new user
            get_or_create_avatar(
                &mut conn.avatar_cache,
                &user.username,
                user.avatar.as_deref(),
            );

            true
        };

        // Only announce if this is their first session (new user) and notifications are enabled
        if is_new_user && self.config.settings.show_connection_notifications {
            self.add_chat_message(
                connection_id,
                ChatMessage::system(t_args(
                    "msg-user-connected",
                    &[("username", &user.username)],
                )),
            )
        } else {
            Task::none()
        }
    }

    /// Handle user disconnected notification
    pub fn handle_user_disconnected(
        &mut self,
        connection_id: usize,
        session_id: u32,
        username: String,
    ) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        // Remove the specific session_id from the user's sessions
        let mut is_last_session = false;
        if let Some(user) = conn
            .online_users
            .iter_mut()
            .find(|u| u.username == username)
        {
            user.session_ids.retain(|&sid| sid != session_id);

            // If user has no more sessions, remove them entirely
            if user.session_ids.is_empty() {
                conn.online_users.retain(|u| u.username != username);
                is_last_session = true;

                // Clear expanded_user if the disconnected user was expanded
                if conn.expanded_user.as_ref() == Some(&username) {
                    conn.expanded_user = None;
                }

                // Remove from avatar cache
                conn.avatar_cache.remove(&username);
            }
        }

        // Only announce if this was their last session (fully offline) and notifications are enabled
        if is_last_session && self.config.settings.show_connection_notifications {
            self.add_chat_message(
                connection_id,
                ChatMessage::system(t_args("msg-user-disconnected", &[("username", &username)])),
            )
        } else {
            Task::none()
        }
    }
}
