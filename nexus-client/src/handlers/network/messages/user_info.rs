//! User info response handlers

use crate::NexusApp;
use crate::avatar::{compute_avatar_hash, get_or_create_avatar};
use crate::handlers::network::constants::DATETIME_FORMAT;
use crate::handlers::network::helpers::{format_duration, sort_user_list};
use crate::i18n::{t, t_args};
use crate::types::{
    ActivePanel, ChatMessage, ChatTab, Message, ResponseRouting, UserInfo as ClientUserInfo,
};
use chrono::Local;
use iced::Task;
use nexus_common::framing::MessageId;
use nexus_common::protocol::{UserInfo as ProtocolUserInfo, UserInfoDetailed};

/// Indentation for user info display lines
const INFO_INDENT: &str = "  ";

impl NexusApp {
    /// Handle user info response
    ///
    /// Routes response based on message_id tracking:
    /// - If tracked as UserInfoPanel(username) → populate panel if username matches
    /// - If tracked as UserInfoChat → display in chat (from `/info` command)
    /// - Otherwise → discard (stale/untracked response)
    pub fn handle_user_info_response(
        &mut self,
        connection_id: usize,
        message_id: MessageId,
        success: bool,
        error: Option<String>,
        user: Option<UserInfoDetailed>,
    ) -> Task<Message> {
        // Check if this response corresponds to a tracked request
        let routing = self
            .connections
            .get_mut(&connection_id)
            .and_then(|conn| conn.pending_requests.remove(&message_id));

        // Determine how to route this response
        let is_chat_request = matches!(routing, Some(ResponseRouting::DisplayUserInfoInChat));

        // If this was a panel request, only populate if panel is still open, waiting,
        // AND the username matches (to handle rapid clicks on different users)
        if let Some(ResponseRouting::PopulateUserInfoPanel(requested_username)) = routing {
            let panel_waiting = self.connections.get(&connection_id).is_some_and(|conn| {
                conn.active_panel == ActivePanel::UserInfo && conn.user_info_data.is_none()
            });

            // Check if username matches (case-insensitive)
            let username_matches = match &user {
                Some(u) => requested_username.to_lowercase() == u.username.to_lowercase(),
                None => true, // Error responses don't have user data, accept them
            };

            if panel_waiting && username_matches {
                // Populate the panel with the response
                if let Some(conn) = self.connections.get_mut(&connection_id) {
                    if success {
                        if let Some(user_data) = user {
                            conn.user_info_data = Some(Ok(user_data));
                        }
                    } else {
                        conn.user_info_data = Some(Err(error.unwrap_or_default()));
                    }
                }
            }
            // Panel request but panel closed, has data, or username mismatch - discard silently
            return Task::none();
        }

        // If not a chat request, discard (unknown/stale response)
        if !is_chat_request {
            return Task::none();
        }

        // Show in chat (from /info command)
        if !success {
            return self
                .add_chat_message(connection_id, ChatMessage::error(error.unwrap_or_default()));
        }

        let Some(user) = user else {
            return Task::none();
        };

        // Calculate session duration
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("System time should be after UNIX epoch")
            .as_secs() as i64;
        let session_duration_secs = now.saturating_sub(user.login_time) as u64;
        let duration_str = format_duration(session_duration_secs);

        // Format account creation time (ISO 8601)
        let created = chrono::DateTime::from_timestamp(user.created_at, 0)
            .map(|dt| dt.format(DATETIME_FORMAT).to_string())
            .unwrap_or_else(|| t("user-info-unknown"));

        // Build multi-line IRC WHOIS-style output
        let mut lines = Vec::new();

        // Username header
        lines.push(format!("[{}]", user.username));

        // Role (only visible to admins)
        if let Some(is_admin) = user.is_admin {
            let role_value = if is_admin {
                t("user-info-role-admin")
            } else {
                t("user-info-role-user")
            };
            lines.push(format!(
                "{INFO_INDENT}{} {}",
                t("user-info-role").to_lowercase(),
                role_value
            ));
        }

        // Sessions
        let session_count = user.session_ids.len();
        let connected_value = if session_count == 1 {
            t_args("user-info-connected-value", &[("duration", &duration_str)])
        } else {
            t_args(
                "user-info-connected-value-sessions",
                &[
                    ("duration", &duration_str),
                    ("count", &session_count.to_string()),
                ],
            )
        };
        lines.push(format!(
            "{INFO_INDENT}{} {}",
            t("user-info-connected").to_lowercase(),
            connected_value
        ));

        // Features
        let features_value = if user.features.is_empty() {
            t("user-info-features-none").to_lowercase()
        } else {
            t_args(
                "user-info-features-value",
                &[("features", &user.features.join(", "))],
            )
        };
        lines.push(format!(
            "{INFO_INDENT}{} {}",
            t("user-info-features").to_lowercase(),
            features_value
        ));

        // Locale
        lines.push(format!(
            "{INFO_INDENT}{} {}",
            t("user-info-locale").to_lowercase(),
            user.locale
        ));

        // IP Addresses (only visible to admins)
        if let Some(addresses) = user.addresses
            && !addresses.is_empty()
        {
            if addresses.len() == 1 {
                lines.push(format!(
                    "{INFO_INDENT}{} {}",
                    t("user-info-address").to_lowercase(),
                    addresses[0]
                ));
            } else {
                lines.push(format!(
                    "{INFO_INDENT}{}",
                    t("user-info-addresses").to_lowercase()
                ));
                for addr in &addresses {
                    lines.push(format!("{INFO_INDENT}  - {}", addr));
                }
            }
        }

        // Account created (last field)
        lines.push(format!(
            "{INFO_INDENT}{} {}",
            t("user-info-created").to_lowercase(),
            created
        ));

        lines.push(format!("{INFO_INDENT}{}", t("user-info-end")));

        // Add each line as a separate chat message with shared timestamp
        let timestamp = Local::now();
        let mut task = Task::none();
        for line in lines {
            task = self.add_chat_message(
                connection_id,
                ChatMessage::info_with_timestamp(line, timestamp),
            );
        }
        // Last add_chat_message will handle auto-scroll
        task
    }

    /// Handle user list response
    pub fn handle_user_list_response(
        &mut self,
        connection_id: usize,
        message_id: MessageId,
        success: bool,
        users: Option<Vec<ProtocolUserInfo>>,
    ) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        // Check if this response corresponds to a tracked request
        let routing = conn.pending_requests.remove(&message_id);

        let users_vec = users.unwrap_or_default();

        // If this was a user management list request, populate the panel
        if matches!(routing, Some(ResponseRouting::PopulateUserManagementList)) {
            if !success {
                if let Some(conn) = self.connections.get_mut(&connection_id) {
                    conn.user_management.all_users = Some(Err(t("err-userlist-failed")));
                }
                return Task::none();
            }

            if let Some(conn) = self.connections.get_mut(&connection_id) {
                conn.user_management.all_users = Some(Ok(users_vec));
            }
            return Task::none();
        }

        if !success {
            return Task::none();
        }

        // If this was a /list all request, display in chat instead of caching
        if matches!(routing, Some(ResponseRouting::DisplayListInChat)) {
            return self.display_all_users_list(connection_id, users_vec);
        }

        // Normal case: update the online_users cache
        let conn = self
            .connections
            .get_mut(&connection_id)
            .expect("connection exists");
        let user_list: Vec<ClientUserInfo> = users_vec
            .into_iter()
            .map(|u| {
                let avatar_hash = compute_avatar_hash(u.avatar.as_deref());
                // Pre-populate avatar cache for this user
                get_or_create_avatar(&mut conn.avatar_cache, &u.username, u.avatar.as_deref());
                ClientUserInfo {
                    username: u.username,
                    is_admin: u.is_admin,
                    session_ids: u.session_ids,
                    avatar_hash,
                }
            })
            .collect();

        conn.online_users = user_list;
        sort_user_list(&mut conn.online_users);

        // Clear expanded_user if the user is no longer in the list
        if let Some(expanded) = &conn.expanded_user
            && !conn.online_users.iter().any(|u| &u.username == expanded)
        {
            conn.expanded_user = None;
        }
        Task::none()
    }

    /// Display all users list in chat (for /list all command)
    fn display_all_users_list(
        &mut self,
        connection_id: usize,
        users: Vec<ProtocolUserInfo>,
    ) -> Task<Message> {
        if users.is_empty() {
            return self.add_chat_message(connection_id, ChatMessage::info(t("cmd-list-empty")));
        }

        // Build IRC-style user list: @admin user1 user2
        let user_count = users.len();
        let user_list: String = users
            .iter()
            .map(|user| {
                if user.is_admin {
                    format!("@{}", user.username)
                } else {
                    user.username.clone()
                }
            })
            .collect::<Vec<_>>()
            .join(" ");

        // Format: "All users: @alice bob charlie (3 users)"
        let message = t_args(
            "cmd-list-all-output",
            &[("users", &user_list), ("count", &user_count.to_string())],
        );

        self.add_chat_message(connection_id, ChatMessage::info(message))
    }

    /// Handle user updated notification
    pub fn handle_user_updated(
        &mut self,
        connection_id: usize,
        previous_username: String,
        user: ProtocolUserInfo,
    ) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        let new_username = user.username;
        let username_changed = previous_username != new_username;
        let new_avatar_hash = compute_avatar_hash(user.avatar.as_deref());

        // Update the user's info in the online_users list
        // Use previous_username to find the user (in case username changed)
        if let Some(existing_user) = conn
            .online_users
            .iter_mut()
            .find(|u| u.username == previous_username)
        {
            // Check if avatar changed (invalidate cache if so)
            let avatar_changed = existing_user.avatar_hash != new_avatar_hash;

            existing_user.username = new_username.clone();
            existing_user.is_admin = user.is_admin;
            existing_user.session_ids = user.session_ids;
            existing_user.avatar_hash = new_avatar_hash;

            // If username changed, remove old cache entry
            if username_changed {
                conn.avatar_cache.remove(&previous_username);
            } else if avatar_changed {
                // Same username but avatar changed - invalidate cache
                conn.avatar_cache.remove(&new_username);
            }

            // Pre-populate cache with new avatar
            get_or_create_avatar(
                &mut conn.avatar_cache,
                &new_username,
                user.avatar.as_deref(),
            );

            // Re-sort the list since username may have changed
            sort_user_list(&mut conn.online_users);
        }

        // If username changed, update user_messages HashMap and active tab
        if username_changed {
            // If this is our own username changing, update conn.username
            if conn.username == previous_username {
                conn.username = new_username.clone();
            }

            // Rename the user_messages entry
            if let Some(messages) = conn.user_messages.remove(&previous_username) {
                conn.user_messages.insert(new_username.clone(), messages);
            }

            // Rename the scroll_states entry
            let old_tab = ChatTab::UserMessage(previous_username.clone());
            let new_tab = ChatTab::UserMessage(new_username.clone());
            if let Some(scroll_state) = conn.scroll_states.remove(&old_tab) {
                conn.scroll_states.insert(new_tab.clone(), scroll_state);
            }

            // Update unread_tabs if present
            if conn.unread_tabs.remove(&old_tab) {
                conn.unread_tabs.insert(new_tab.clone());
            }

            // Update active_chat_tab if it's for this user
            if conn.active_chat_tab == old_tab {
                conn.active_chat_tab = new_tab;
            }

            // Update expanded_user if it was set to the old username
            if conn.expanded_user.as_ref() == Some(&previous_username) {
                conn.expanded_user = Some(new_username);
            }
        }

        Task::none()
    }
}
