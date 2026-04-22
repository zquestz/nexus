//! Permissions update handler

use iced::Task;
use nexus_common::protocol::{ClientMessage, ServerInfo};
use nexus_common::validators::PasswordStrength;

use crate::NexusApp;
use crate::config::events::EventType;
use crate::events::{EventContext, emit_event};
use crate::i18n::t;
use crate::image::decode_data_uri_max_width;
use crate::style::SERVER_IMAGE_MAX_CACHE_WIDTH;
use crate::types::{ChatMessage, Message};
use crate::views::constants::PERMISSION_USER_LIST;

impl NexusApp {
    /// Handle permissions updated notification
    ///
    /// Note: Previously, this also updated chat_topic when ChatTopic permission changed.
    /// With multi-channel support, topics are now per-channel and included in LoginResponse.channels
    /// or ChatUpdated messages. When a user gains ChatTopic permission mid-session, they
    /// won't see existing topics until they reconnect or topics are changed. This is acceptable
    /// since the multi-channel client UI will handle topic visibility per-channel.
    pub fn handle_permissions_updated(
        &mut self,
        connection_id: usize,
        is_admin: bool,
        permissions: Vec<String>,
        server_info: Option<ServerInfo>,
    ) -> Task<Message> {
        // Get server name before mutable borrow for notification
        let server_name = self
            .connections
            .get(&connection_id)
            .map(|c| c.connection_info.server_name.clone())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| t("unknown-server"));

        // Emit permissions changed notification
        emit_event(
            self,
            EventType::PermissionsChanged,
            EventContext::new()
                .with_connection_id(connection_id)
                .with_server_name(&server_name),
        );

        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        let had_user_list = conn.has_permission(PERMISSION_USER_LIST);

        let has_user_list = is_admin || permissions.iter().any(|p| p == PERMISSION_USER_LIST);

        conn.is_admin = is_admin;
        conn.permissions = permissions;

        // Update server info fields unconditionally (None means "cleared/not set").
        // Exception: image uses a guard because it's not sent in PermissionsUpdated,
        // so None means "not included" rather than "cleared".
        if let Some(info) = server_info {
            conn.server_name = info.name;
            conn.server_description = info.description;
            // Normalize empty → None so `Some` always carries a non-empty address.
            conn.public_address = info.public_address.filter(|s| !s.is_empty());
            conn.server_version = info.version;
            conn.max_connections_per_ip = info.max_connections_per_ip;
            conn.max_transfers_per_ip = info.max_transfers_per_ip;
            conn.file_reindex_interval = info.file_reindex_interval;
            conn.persistent_channels = info.persistent_channels;
            conn.auto_join_channels = info.auto_join_channels;
            conn.chat_burst_limit = info.chat_burst_limit;
            conn.chat_rate_limit = info.chat_rate_limit;
            if let Some(image) = info.image {
                // Decode first using reference, then move (avoids clone)
                conn.cached_server_image = if image.is_empty() {
                    None
                } else {
                    decode_data_uri_max_width(&image, SERVER_IMAGE_MAX_CACHE_WIDTH)
                };
                conn.server_image = image;
            }
            if let Some(score) = info.min_password_strength {
                conn.min_password_strength = PasswordStrength::from(score);
            }
            conn.log_level = info.log_level;
        }

        // If user just gained user_list permission, refresh the list
        // (it may be stale from missed join/leave events while permission was revoked)
        if !had_user_list
            && has_user_list
            && let Err(e) = conn.send(ClientMessage::UserList { all: false })
        {
            // Channel send failed - add error to chat
            let error_msg = format!("{}: {}", t("err-userlist-failed"), e);
            return self.add_console_message(connection_id, ChatMessage::error(error_msg));
        }

        // Show notification message
        self.add_console_message(
            connection_id,
            ChatMessage::system(t("msg-permissions-updated")),
        )
    }
}
