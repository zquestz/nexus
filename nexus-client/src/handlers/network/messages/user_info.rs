//! User info response handlers

use crate::handlers::network::constants::DATETIME_FORMAT;
use crate::handlers::network::helpers::{format_duration, sort_user_list};
use crate::i18n::{t, t_args};
use crate::types::{ChatMessage, ChatTab, Message, UserInfo as ClientUserInfo};
use crate::NexusApp;
use chrono::Local;
use iced::Task;
use nexus_common::protocol::{UserInfo as ProtocolUserInfo, UserInfoDetailed};

impl NexusApp {
    /// Handle user info response
    pub fn handle_user_info_response(
        &mut self,
        connection_id: usize,
        success: bool,
        error: Option<String>,
        user: Option<UserInfoDetailed>,
    ) -> Task<Message> {
        if !success {
            let err = error.unwrap_or_default();
            let message = ChatMessage {
                username: t("msg-username-info"),
                message: t_args("user-info-error", &[("error", &err)]),
                timestamp: Local::now(),
            };
            return self.add_chat_message(connection_id, message);
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

        // Header
        lines.push(t_args("user-info-header", &[("username", &user.username)]));

        // Admin status (only visible to admins)
        if let Some(is_admin) = user.is_admin
            && is_admin
        {
            lines.push(format!("  {}", t("user-info-is-admin")));
        }

        // Sessions
        let session_count = user.session_ids.len();
        if session_count == 1 {
            lines.push(format!(
                "  {}",
                t_args("user-info-connected-ago", &[("duration", &duration_str)])
            ));
        } else {
            lines.push(format!(
                "  {}",
                t_args(
                    "user-info-connected-sessions",
                    &[
                        ("duration", &duration_str),
                        ("count", &session_count.to_string())
                    ]
                )
            ));
        }

        // Features
        if !user.features.is_empty() {
            lines.push(format!(
                "  {}",
                t_args(
                    "user-info-features",
                    &[("features", &user.features.join(", "))]
                )
            ));
        }

        // Locale
        lines.push(format!(
            "  {}",
            t_args("user-info-locale", &[("locale", &user.locale)])
        ));

        // IP Addresses (only visible to admins)
        if let Some(addresses) = user.addresses
            && !addresses.is_empty()
        {
            if addresses.len() == 1 {
                lines.push(format!(
                    "  {}",
                    t_args("user-info-address", &[("address", &addresses[0])])
                ));
            } else {
                lines.push(format!("  {}", t("user-info-addresses")));
                for addr in &addresses {
                    lines.push(format!(
                        "  {}",
                        t_args("user-info-address-item", &[("address", addr)])
                    ));
                }
            }
        }

        // Account created (last field)
        lines.push(format!(
            "  {}",
            t_args("user-info-created", &[("created", &created)])
        ));

        lines.push(format!("  {}", t("user-info-end")));

        // Add each line as a separate chat message
        let timestamp = Local::now();
        let mut task = Task::none();
        for line in lines {
            task = self.add_chat_message(
                connection_id,
                ChatMessage {
                    username: t("msg-username-info"),
                    message: line,
                    timestamp,
                },
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
        // Sort to maintain alphabetical order
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

        // Update the user's info in the online_users list
        // Use previous_username to find the user (in case username changed)
        if let Some(existing_user) = conn
            .online_users
            .iter_mut()
            .find(|u| u.username == previous_username)
        {
            existing_user.username = user.username.clone();
            existing_user.is_admin = user.is_admin;
            existing_user.session_ids = user.session_ids;

            // Re-sort the list since username may have changed
            sort_user_list(&mut conn.online_users);
        }

        // If username changed, update user_messages HashMap and active tab
        if previous_username != user.username {
            // If this is our own username changing, update conn.username
            if conn.username == previous_username {
                conn.username = user.username.clone();
            }

            // Rename the user_messages entry
            if let Some(messages) = conn.user_messages.remove(&previous_username) {
                conn.user_messages.insert(user.username.clone(), messages);
            }

            // Update unread_tabs if present
            let old_tab = ChatTab::UserMessage(previous_username.clone());
            if conn.unread_tabs.remove(&old_tab) {
                conn.unread_tabs
                    .insert(ChatTab::UserMessage(user.username.clone()));
            }

            // Update active_chat_tab if it's for this user
            if conn.active_chat_tab == old_tab {
                conn.active_chat_tab = ChatTab::UserMessage(user.username.clone());
            }

            // Update expanded_user if it was set to the old username
            if conn.expanded_user.as_ref() == Some(&previous_username) {
                conn.expanded_user = Some(user.username.clone());
            }
        }

        Task::none()
    }
}