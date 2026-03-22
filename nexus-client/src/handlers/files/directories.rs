//! Directory creation handlers

use iced::Task;
use iced::widget::{Id, operation};
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
        operation::focus(Id::from(InputId::NewDirectoryName))
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

        conn.files_management
            .active_tab_mut()
            .is_create_dir_submitting = true;

        let tab_id = conn.files_management.active_tab_id();
        match conn.send(ClientMessage::FileCreateDir { path, name, root }) {
            Ok(message_id) => {
                conn.pending_requests
                    .track(message_id, ResponseRouting::FileCreateDirResult { tab_id });
            }
            Err(e) => {
                let tab = conn.files_management.active_tab_mut();
                tab.is_create_dir_submitting = false;
                tab.new_directory_error = Some(format!("{}: {}", t("err-send-failed"), e));
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
