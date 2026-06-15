//! Directory creation handlers

use iced::Task;
use nexus_common::protocol::ClientMessage;
use nexus_common::validators::{self};

use super::dir_name_error_message;
use crate::NexusApp;
use crate::i18n::t;
use crate::types::{InputId, Message, PendingRequests, ResponseRouting};

impl NexusApp {
    pub fn handle_file_new_directory_clicked(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        conn.files_management
            .active_tab_mut()
            .open_new_directory_dialog();

        // Focus the name input field
        self.focus_field(InputId::NewDirectoryName)
    }

    /// Handle new directory name input change
    pub fn handle_file_new_directory_name_changed(&mut self, name: String) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        // Validate the name in real-time (before storing to avoid clone)
        let validation_error = if name.is_empty() {
            None
        } else {
            validators::validate_dir_name(&name)
                .err()
                .map(dir_name_error_message)
        };

        let tab = conn.files_management.active_tab_mut();
        tab.new_directory_name = name;
        tab.new_directory_error = validation_error;

        // Track focus for tray-restore, matching the `*_changed`
        // convention used by every other form's input handlers.
        self.focused_field = InputId::NewDirectoryName;
        Task::none()
    }

    /// Handle new directory submit button
    pub fn handle_file_new_directory_submit(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        let tab = conn.files_management.active_tab_mut();

        if tab.is_create_dir_submitting {
            return Task::none();
        }

        let name = &tab.new_directory_name;

        // Validate first
        if name.is_empty() {
            tab.new_directory_error = Some(t("err-dir-name-empty"));
            return Task::none();
        }

        if let Err(e) = validators::validate_dir_name(name) {
            tab.new_directory_error = Some(dir_name_error_message(e));
            return Task::none();
        }

        let name = tab.new_directory_name.clone();
        let path = tab.current_path.clone();
        let root = tab.viewing_root;

        {
            let tab = conn.files_management.active_tab_mut();
            tab.new_directory_error = None;
            tab.new_directory_submission_error = None;
            tab.is_create_dir_submitting = true;
        }

        let tab_id = conn.files_management.active_tab_id();
        match conn.send(ClientMessage::FileCreateDir { path, name, root }) {
            Ok(message_id) => {
                conn.pending_requests
                    .track(message_id, ResponseRouting::FileCreateDirResult { tab_id });
            }
            Err(e) => {
                let tab = conn.files_management.active_tab_mut();
                tab.is_create_dir_submitting = false;
                tab.new_directory_submission_error =
                    Some(format!("{}: {}", t("err-send-failed"), e));
            }
        }

        Task::none()
    }

    /// Handle new directory cancel button (close dialog)
    pub fn handle_file_new_directory_cancel(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        conn.files_management
            .active_tab_mut()
            .close_new_directory_dialog();

        Task::none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use nexus_common::validators::PasswordStrength;
    use tokio::sync::{Mutex, mpsc};

    use crate::types::{ConnectionInfo, ServerConnection, ServerConnectionParams};

    fn closed_connection(connection_id: usize) -> ServerConnection {
        let (tx, rx) = mpsc::unbounded_channel();
        drop(rx);

        ServerConnection::new(ServerConnectionParams {
            bookmark_id: None,
            user_id: None,
            nickname: "me".to_string(),
            connection_info: ConnectionInfo {
                server_name: "Test".to_string(),
                address: "bbs.example".to_string(),
                port: 7500,
                transfer_port: 7501,
                certificate_fingerprint: String::new(),
                username: "me".to_string(),
                password: String::new(),
                nickname: String::new(),
            },
            display_name: "Test".to_string(),
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

    fn app_with_closed_connection() -> NexusApp {
        let connection_id = 1;
        let mut app = NexusApp {
            active_connection: Some(connection_id),
            ..NexusApp::default()
        };
        app.connections
            .insert(connection_id, closed_connection(connection_id));
        app
    }

    #[test]
    fn new_directory_valid_edit_preserves_submission_error() {
        let mut app = app_with_closed_connection();
        let conn = app.connections.get_mut(&1).expect("connection exists");
        let tab = conn.files_management.active_tab_mut();
        tab.creating_directory = true;
        tab.new_directory_name = "Old".to_string();
        tab.new_directory_submission_error = Some("server error".to_string());

        let task = app.handle_file_new_directory_name_changed("New".to_string());
        drop(task);

        let tab = app.connections[&1].files_management.active_tab();
        assert_eq!(tab.new_directory_name, "New");
        assert!(tab.new_directory_error.is_none());
        assert_eq!(
            tab.new_directory_submission_error.as_deref(),
            Some("server error")
        );
    }

    #[test]
    fn new_directory_invalid_edit_sets_validation_error_without_dropping_submission_error() {
        let mut app = app_with_closed_connection();
        let conn = app.connections.get_mut(&1).expect("connection exists");
        let tab = conn.files_management.active_tab_mut();
        tab.creating_directory = true;
        tab.new_directory_name = "Old".to_string();
        tab.new_directory_submission_error = Some("server error".to_string());

        let task = app.handle_file_new_directory_name_changed("bad/name".to_string());
        drop(task);

        let tab = app.connections[&1].files_management.active_tab();
        assert_eq!(tab.new_directory_name, "bad/name");
        assert!(tab.new_directory_error.is_some());
        assert_eq!(
            tab.new_directory_submission_error.as_deref(),
            Some("server error")
        );
    }

    #[test]
    fn new_directory_submit_with_submission_error_still_attempts_send() {
        let mut app = app_with_closed_connection();
        let conn = app.connections.get_mut(&1).expect("connection exists");
        let tab = conn.files_management.active_tab_mut();
        tab.creating_directory = true;
        tab.new_directory_name = "New".to_string();
        tab.new_directory_submission_error = Some("server error".to_string());

        let task = app.handle_file_new_directory_submit();
        drop(task);

        let tab = app.connections[&1].files_management.active_tab();
        assert!(!tab.is_create_dir_submitting);
        assert!(
            tab.new_directory_submission_error
                .as_deref()
                .is_some_and(|error| error.starts_with(&t("err-send-failed")))
        );
    }
}
