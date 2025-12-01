//! User connection/disconnection handlers

use crate::NexusApp;
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
            false
        } else {
            // New user - add to list
            conn.online_users.push(ClientUserInfo {
                username: user.username.clone(),
                is_admin: user.is_admin,
                session_ids: user.session_ids.clone(),
            });
            sort_user_list(&mut conn.online_users);
            true
        };

        // Only announce if this is their first session (new user) and notifications are enabled
        if is_new_user && self.config.show_connection_notifications {
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
            }
        }

        // Only announce if this was their last session (fully offline) and notifications are enabled
        if is_last_session && self.config.show_connection_notifications {
            self.add_chat_message(
                connection_id,
                ChatMessage::system(t_args("msg-user-disconnected", &[("username", &username)])),
            )
        } else {
            Task::none()
        }
    }
}
