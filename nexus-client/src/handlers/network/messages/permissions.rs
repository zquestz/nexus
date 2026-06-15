//! Permissions update handler

use iced::Task;
use nexus_common::protocol::{ClientMessage, ServerInfo};
use nexus_common::validators::{ImageDecodeProfile, PasswordStrength};

use crate::NexusApp;
use crate::config::events::EventType;
use crate::events::{EventContext, emit_event};
use crate::i18n::t;
use crate::image::decode_data_uri_max_width;
use crate::style::SERVER_IMAGE_MAX_CACHE_WIDTH;
use crate::types::{ChatMessage, Message};
use crate::views::constants::{
    PERMISSION_USER_LIST, PERMISSION_VOICE_LISTEN, PERMISSION_VOICE_TALK,
};

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
        let had_voice_listen = conn.has_permission(PERMISSION_VOICE_LISTEN);
        let had_voice_talk = conn.has_permission(PERMISSION_VOICE_TALK);

        let has_user_list = is_admin || permissions.iter().any(|p| p == PERMISSION_USER_LIST);
        let has_voice_listen = is_admin || permissions.iter().any(|p| p == PERMISSION_VOICE_LISTEN);
        let has_voice_talk = is_admin || permissions.iter().any(|p| p == PERMISSION_VOICE_TALK);

        conn.is_admin = is_admin;
        conn.permissions = permissions;

        if had_voice_listen && !has_voice_listen {
            conn.channel_voiced.clear();
            conn.user_message_voiced.clear();
        }

        if had_user_list && !has_user_list {
            conn.online_users.clear();
            conn.expanded_user = None;
        }

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
            conn.max_outbound_rate = info.max_outbound_rate;
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
                    decode_data_uri_max_width(
                        &image,
                        SERVER_IMAGE_MAX_CACHE_WIDTH,
                        ImageDecodeProfile::Server,
                    )
                };
                conn.server_image = image;
            }
            if let Some(score) = info.min_password_strength {
                conn.min_password_strength = PasswordStrength::from(score);
            }
            conn.log_level = info.log_level;
            conn.scheduler_chunk_size = info.scheduler_chunk_size;
        }

        // If user just gained user_list permission, refresh the list
        // (it may be stale from missed join/leave events while permission was revoked)
        let user_list_error = if !had_user_list && has_user_list {
            conn.send(ClientMessage::UserList { all: false })
                .err()
                .map(|e| format!("{}: {}", t("err-userlist-failed"), e))
        } else {
            None
        };

        let voice_task = if had_voice_talk != has_voice_talk {
            self.set_active_voice_transmit_allowed(connection_id, has_voice_talk)
        } else {
            Task::none()
        };

        if let Some(error_msg) = user_list_error {
            // Channel send failed - add error to chat
            return Task::batch([
                voice_task,
                self.add_console_message(connection_id, ChatMessage::error(error_msg)),
            ]);
        }

        // Show notification message
        let notice_task = self.add_console_message(
            connection_id,
            ChatMessage::system(t("msg-permissions-updated")),
        );

        Task::batch([voice_task, notice_task])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use nexus_common::names::fold_name;
    use tokio::sync::{Mutex, mpsc};

    use crate::types::{ConnectionInfo, ServerConnection, ServerConnectionParams, UserInfo};

    fn test_connection(connection_id: usize) -> ServerConnection {
        let (tx, _rx) = mpsc::unbounded_channel();
        ServerConnection::new(ServerConnectionParams {
            bookmark_id: None,
            user_id: None,
            nickname: "me".to_string(),
            connection_info: ConnectionInfo {
                server_name: String::new(),
                address: String::new(),
                port: 0,
                transfer_port: 0,
                certificate_fingerprint: String::new(),
                username: "me".to_string(),
                password: String::new(),
                nickname: "me".to_string(),
            },
            display_name: String::new(),
            connection_id,
            is_admin: false,
            permissions: Vec::new(),
            features: Vec::new(),
            server_name: None,
            server_description: None,
            public_address: None,
            server_version: None,
            server_image: String::new(),
            cached_server_image: None,
            chat_burst_limit: None,
            chat_rate_limit: None,
            max_connections_per_ip: None,
            max_outbound_rate: None,
            max_transfers_per_ip: None,
            file_reindex_interval: None,
            persistent_channels: None,
            auto_join_channels: None,
            min_password_strength: PasswordStrength::Weak,
            log_level: None,
            scheduler_chunk_size: None,
            tx,
            shutdown_handle: Arc::new(Mutex::new(None)),
        })
    }

    #[test]
    fn removing_voice_listen_clears_voice_presence_caches() {
        let mut app = NexusApp {
            active_connection: Some(1),
            ..NexusApp::default()
        };
        let mut conn = test_connection(1);
        conn.permissions.push(PERMISSION_VOICE_LISTEN.to_string());
        conn.channel_voiced
            .entry(fold_name("#general"))
            .or_default()
            .insert(fold_name("Alice"));
        conn.user_message_voiced.insert(fold_name("Bob"));
        app.connections.insert(1, conn);

        let _ = app.handle_permissions_updated(1, false, Vec::new(), None);

        let conn = app.connections.get(&1).expect("connection remains");
        assert!(conn.channel_voiced.is_empty());
        assert!(conn.user_message_voiced.is_empty());
    }

    #[test]
    fn removing_user_list_clears_cached_user_list_state() {
        let mut app = NexusApp {
            active_connection: Some(1),
            ..NexusApp::default()
        };
        let mut conn = test_connection(1);
        conn.permissions.push(PERMISSION_USER_LIST.to_string());
        conn.expanded_user = Some("Alice".to_string());
        conn.online_users.push(UserInfo {
            id: 2,
            username: "alice".to_string(),
            nickname: "Alice".to_string(),
            is_admin: false,
            is_shared: false,
            session_ids: vec![22],
            avatar_hash: None,
            is_away: false,
            status: None,
        });
        app.connections.insert(1, conn);

        let _ = app.handle_permissions_updated(1, false, Vec::new(), None);

        let conn = app.connections.get(&1).expect("connection remains");
        assert!(conn.online_users.is_empty());
        assert!(conn.expanded_user.is_none());
    }
}
