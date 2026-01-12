//! User connection/disconnection handlers

use iced::Task;
use nexus_common::protocol::UserInfo as ProtocolUserInfo;

use crate::NexusApp;
use crate::avatar::{compute_avatar_hash, get_or_create_avatar};
use crate::config::events::EventType;
use crate::events::{EventContext, emit_event};
use crate::handlers::network::helpers::sort_user_list;
use crate::i18n::t_args;
use crate::types::{ChatMessage, Message, UserInfo as ClientUserInfo};

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

        // Nickname is always populated (equals username for regular accounts)
        let nickname = user.nickname.clone();

        // For shared accounts, each session is a separate entry (don't merge)
        // For regular accounts, merge sessions by username
        let is_new_user = if user.is_shared {
            // Shared account: Check if this specific nickname already exists
            // (shouldn't happen since nicknames are unique, but handle gracefully)
            if let Some(existing_user) = conn
                .online_users
                .iter_mut()
                .find(|u| u.is_shared && u.nickname == user.nickname)
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
                    conn.avatar_cache.remove(&nickname);
                    get_or_create_avatar(&mut conn.avatar_cache, &nickname, user.avatar.as_deref());
                }

                // Update away/status (latest login wins)
                existing_user.is_away = user.is_away;
                existing_user.status = user.status.clone();

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
                    is_away: user.is_away,
                    status: user.status.clone(),
                });
                sort_user_list(&mut conn.online_users);

                // Cache avatar by nickname
                get_or_create_avatar(&mut conn.avatar_cache, &nickname, user.avatar.as_deref());

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
                    conn.avatar_cache.remove(&nickname);
                    get_or_create_avatar(&mut conn.avatar_cache, &nickname, user.avatar.as_deref());
                }

                // Update away/status (latest login wins)
                existing_user.is_away = user.is_away;
                existing_user.status = user.status.clone();

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
                    is_away: user.is_away,
                    status: user.status.clone(),
                });
                sort_user_list(&mut conn.online_users);

                // Cache avatar by nickname (equals username for regular accounts)
                get_or_create_avatar(&mut conn.avatar_cache, &nickname, user.avatar.as_deref());

                true
            }
        };

        // Emit notification for new users
        if is_new_user {
            emit_event(
                self,
                EventType::UserConnected,
                EventContext::new()
                    .with_connection_id(connection_id)
                    .with_username(&nickname),
            );
        }

        // Only announce in chat if this is their first session (new user) and notifications are enabled
        if is_new_user && self.config.settings.show_connection_notifications {
            self.add_console_message(
                connection_id,
                ChatMessage::system(t_args("msg-user-connected", &[("nickname", &nickname)])),
            )
        } else {
            Task::none()
        }
    }

    /// Handle user disconnected notification
    ///
    /// The `nickname` parameter is the display name (nickname for shared accounts,
    /// username for regular accounts - since nickname always equals username for regular accounts).
    pub fn handle_user_disconnected(
        &mut self,
        connection_id: usize,
        session_id: u32,
        nickname: String,
    ) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        // Remove the specific session_id from the user's sessions
        // We look up by session_id since nickname may differ from username for shared accounts
        let mut is_last_session = false;
        let mut nickname_to_remove: Option<String> = None;

        if let Some(user) = conn
            .online_users
            .iter_mut()
            .find(|u| u.session_ids.contains(&session_id))
        {
            user.session_ids.retain(|&sid| sid != session_id);

            // If user has no more sessions, remove them entirely
            if user.session_ids.is_empty() {
                // Get nickname for avatar cache removal
                nickname_to_remove = Some(user.nickname.clone());
                is_last_session = true;
            }
        }

        // Remove user from list and cache if this was their last session
        if let Some(ref nickname) = nickname_to_remove {
            // Remove by nickname
            conn.online_users.retain(|u| u.nickname != *nickname);

            // Clear expanded_user if the disconnected user was expanded
            if conn.expanded_user.as_deref() == Some(nickname.as_str()) {
                conn.expanded_user = None;
            }

            // Remove from avatar cache (keyed by nickname)
            conn.avatar_cache.remove(nickname);
        }

        // Emit notification for users going offline
        if is_last_session {
            emit_event(
                self,
                EventType::UserDisconnected,
                EventContext::new()
                    .with_connection_id(connection_id)
                    .with_username(&nickname),
            );
        }

        // Only announce in chat if this was their last session (fully offline) and notifications are enabled
        if is_last_session && self.config.settings.show_connection_notifications {
            self.add_console_message(
                connection_id,
                ChatMessage::system(t_args("msg-user-disconnected", &[("nickname", &nickname)])),
            )
        } else {
            Task::none()
        }
    }
}
