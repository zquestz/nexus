//! User info response handlers

use crate::NexusApp;
use crate::handlers::network::constants::DATETIME_FORMAT;
use crate::handlers::network::helpers::{format_duration, sort_user_list};
use crate::i18n::{t, t_args};
use crate::types::{ActivePanel, ChatMessage, ChatTab, Message, UserInfo as ClientUserInfo};
use chrono::Local;
use iced::Task;
use nexus_common::protocol::{UserInfo as ProtocolUserInfo, UserInfoDetailed};

/// Indentation for user info display lines
const INFO_INDENT: &str = "  ";

impl NexusApp {
    /// Handle user info response
    ///
    /// If the UserInfo panel is open and waiting for data, populate it.
    /// Otherwise (e.g., from /info command), show the result in chat.
    pub fn handle_user_info_response(
        &mut self,
        connection_id: usize,
        success: bool,
        error: Option<String>,
        user: Option<UserInfoDetailed>,
    ) -> Task<Message> {
        // Check if UserInfo panel is open and waiting for data
        let panel_waiting = self.ui_state.active_panel == ActivePanel::UserInfo
            && self
                .connections
                .get(&connection_id)
                .is_some_and(|conn| conn.user_info_data.is_none());

        if panel_waiting {
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
            return Task::none();
        }

        // Panel not open - show in chat (from /info command)
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
        success: bool,
        users: Option<Vec<ProtocolUserInfo>>,
    ) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        if !success {
            return Task::none();
        }

        conn.online_users = users
            .unwrap_or_default()
            .into_iter()
            .map(|u| ClientUserInfo {
                username: u.username,
                is_admin: u.is_admin,
                session_ids: u.session_ids,
            })
            .collect();
        sort_user_list(&mut conn.online_users);

        // Clear expanded_user if the user is no longer in the list
        if let Some(expanded) = &conn.expanded_user
            && !conn.online_users.iter().any(|u| &u.username == expanded)
        {
            conn.expanded_user = None;
        }
        Task::none()
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

        // Update the user's info in the online_users list
        // Use previous_username to find the user (in case username changed)
        if let Some(existing_user) = conn
            .online_users
            .iter_mut()
            .find(|u| u.username == previous_username)
        {
            existing_user.username = new_username.clone();
            existing_user.is_admin = user.is_admin;
            existing_user.session_ids = user.session_ids;

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
