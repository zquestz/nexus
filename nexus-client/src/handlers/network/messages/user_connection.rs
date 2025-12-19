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
    ///
    /// For regular accounts: Multiple sessions are merged into one entry (by username)
    /// For shared accounts: Each session is a separate entry (by nickname)
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

        // Display name is nickname for shared accounts, username for regular
        let display_name = user
            .nickname
            .as_deref()
            .unwrap_or(&user.username)
            .to_string();

        // For shared accounts, each session is a separate entry (don't merge)
        // For regular accounts, merge sessions by username
        let is_new_user = if user.is_shared {
            // Shared account: Check if this specific nickname already exists
            // (shouldn't happen since nicknames are unique, but handle gracefully)
            if let Some(existing_user) = conn
                .online_users
                .iter_mut()
                .find(|u| u.is_shared && u.nickname.as_deref() == user.nickname.as_deref())
            {
                // Nickname already exists - merge session_ids (edge case)
                for session_id in &user.session_ids {
                    if !existing_user.session_ids.contains(session_id) {
                        existing_user.session_ids.push(*session_id);
                    }
                }

                // Update avatar if it changed
                if existing_user.avatar_hash != new_avatar_hash {
                    existing_user.avatar_hash = new_avatar_hash;
                    conn.avatar_cache.remove(&display_name);
                    get_or_create_avatar(
                        &mut conn.avatar_cache,
                        &display_name,
                        user.avatar.as_deref(),
                    );
                }

                false
            } else {
                // New shared account session - add as separate entry
                conn.online_users.push(ClientUserInfo {
                    username: user.username.clone(),
                    nickname: user.nickname.clone(),
                    is_admin: user.is_admin,
                    is_shared: user.is_shared,
                    session_ids: user.session_ids.clone(),
                    avatar_hash: new_avatar_hash,
                });
                sort_user_list(&mut conn.online_users);

                // Cache avatar by display name (nickname)
                get_or_create_avatar(
                    &mut conn.avatar_cache,
                    &display_name,
                    user.avatar.as_deref(),
                );

                true
            }
        } else {
            // Regular account: Check if user already exists (multi-device connection)
            if let Some(existing_user) = conn
                .online_users
                .iter_mut()
                .find(|u| !u.is_shared && u.username == user.username)
            {
                // User already exists - merge session_ids
                for session_id in &user.session_ids {
                    if !existing_user.session_ids.contains(session_id) {
                        existing_user.session_ids.push(*session_id);
                    }
                }

                // Update avatar if it changed (latest login wins)
                if existing_user.avatar_hash != new_avatar_hash {
                    existing_user.avatar_hash = new_avatar_hash;
                    conn.avatar_cache.remove(&display_name);
                    get_or_create_avatar(
                        &mut conn.avatar_cache,
                        &display_name,
                        user.avatar.as_deref(),
                    );
                }

                false
            } else {
                // New user - add to list
                conn.online_users.push(ClientUserInfo {
                    username: user.username.clone(),
                    nickname: user.nickname.clone(),
                    is_admin: user.is_admin,
                    is_shared: user.is_shared,
                    session_ids: user.session_ids.clone(),
                    avatar_hash: new_avatar_hash,
                });
                sort_user_list(&mut conn.online_users);

                // Cache avatar by display name (username for regular accounts)
                get_or_create_avatar(
                    &mut conn.avatar_cache,
                    &display_name,
                    user.avatar.as_deref(),
                );

                true
            }
        };

        // Only announce if this is their first session (new user) and notifications are enabled
        if is_new_user && self.config.settings.show_connection_notifications {
            self.add_chat_message(
                connection_id,
                ChatMessage::system(t_args(
                    "msg-user-connected",
                    &[("username", &display_name)],
                )),
            )
        } else {
            Task::none()
        }
    }

    /// Handle user disconnected notification
    ///
    /// The `username` parameter is the display name (nickname for shared accounts,
    /// actual username for regular accounts).
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
        // We look up by session_id since username is the display name which may differ
        let mut is_last_session = false;
        let mut display_name_to_remove: Option<String> = None;

        if let Some(user) = conn
            .online_users
            .iter_mut()
            .find(|u| u.session_ids.contains(&session_id))
        {
            user.session_ids.retain(|&sid| sid != session_id);

            // If user has no more sessions, remove them entirely
            if user.session_ids.is_empty() {
                // Get display name for avatar cache removal
                display_name_to_remove = Some(user.display_name().to_string());
                is_last_session = true;
            }
        }

        // Remove user from list and cache if this was their last session
        if let Some(ref display_name) = display_name_to_remove {
            // For shared accounts, remove by nickname match; for regular, by username
            conn.online_users
                .retain(|u| u.display_name() != display_name);

            // Clear expanded_user if the disconnected user was expanded
            // (expanded_user stores username, not display_name, so check both)
            if conn.expanded_user.as_ref().is_some_and(|expanded| {
                conn.online_users
                    .iter()
                    .all(|u| &u.username != expanded && u.nickname.as_ref() != Some(expanded))
            }) {
                conn.expanded_user = None;
            }

            // Remove from avatar cache (keyed by display name)
            conn.avatar_cache.remove(display_name);
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