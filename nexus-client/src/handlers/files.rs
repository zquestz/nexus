//! Files panel handlers

use iced::Task;
use iced::widget::{Id, operation};
use nexus_common::protocol::ClientMessage;
use nexus_common::validators::{self, DirNameError};

use crate::NexusApp;
use crate::i18n::t;
use crate::types::{ActivePanel, InputId, Message, PendingRequests, ResponseRouting};

/// Convert a directory name validation error to a localized error message
fn dir_name_error_message(error: DirNameError) -> String {
    match error {
        DirNameError::Empty => t("err-dir-name-empty"),
        DirNameError::TooLong => crate::i18n::t_args(
            "err-dir-name-too-long",
            &[("max_length", &validators::MAX_DIR_NAME_LENGTH.to_string())],
        ),
        DirNameError::ContainsPathSeparator => t("err-dir-name-path-separator"),
        DirNameError::ContainsParentRef => t("err-dir-name-parent-ref"),
        DirNameError::ContainsNull | DirNameError::InvalidCharacters => t("err-dir-name-invalid"),
    }
}

impl NexusApp {
    // ==================== Panel Toggle ====================

    /// Toggle the files panel
    ///
    /// When opening, fetches the file list for the current path from the server.
    /// Remembers the last viewed directory.
    pub fn handle_toggle_files(&mut self) -> Task<Message> {
        if self.active_panel() == ActivePanel::Files {
            return Task::none();
        }

        self.set_active_panel(ActivePanel::Files);

        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        // Initialize show_hidden from config on first open
        conn.files_management.show_hidden = self.config.settings.show_hidden_files;

        // Remember the current path - don't reset it
        let current_path = conn.files_management.current_path.clone();
        let viewing_root = conn.files_management.viewing_root;
        let show_hidden = conn.files_management.show_hidden;

        // Clear entries and error to show loading state, but keep the path
        conn.files_management.entries = None;
        conn.files_management.error = None;

        // Fetch the file list for the current path (or home if first time)
        self.send_file_list_request(conn_id, current_path, viewing_root, show_hidden)
    }

    /// Handle cancel in files panel (close the panel)
    pub fn handle_cancel_files(&mut self) -> Task<Message> {
        self.handle_show_chat_view()
    }

    // ==================== Navigation ====================

    /// Navigate to a directory path (or refresh if same path)
    pub fn handle_file_navigate(&mut self, path: String) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        // Update the current path and clear entries to show loading state
        conn.files_management.navigate_to(path.clone());
        let viewing_root = conn.files_management.viewing_root;
        let show_hidden = conn.files_management.show_hidden;

        // Fetch the file list for the path
        self.send_file_list_request(conn_id, path, viewing_root, show_hidden)
    }

    /// Navigate up one directory level
    pub fn handle_file_navigate_up(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        // Navigate up and clear entries to show loading state
        conn.files_management.navigate_up();
        let new_path = conn.files_management.current_path.clone();
        let viewing_root = conn.files_management.viewing_root;
        let show_hidden = conn.files_management.show_hidden;

        // Fetch the file list for the new path
        self.send_file_list_request(conn_id, new_path, viewing_root, show_hidden)
    }

    /// Navigate to the home directory (or refresh if already there)
    ///
    /// Preserves the current viewing_root state - home means root of current view.
    pub fn handle_file_navigate_home(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        // Navigate to home (preserves viewing_root state)
        conn.files_management.navigate_home();
        let viewing_root = conn.files_management.viewing_root;
        let show_hidden = conn.files_management.show_hidden;

        // Fetch the file list for home (respects current view mode)
        self.send_file_list_request(conn_id, String::new(), viewing_root, show_hidden)
    }

    /// Refresh the current directory listing
    pub fn handle_file_refresh(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        // Clear entries and error to show loading state
        let current_path = conn.files_management.current_path.clone();
        let viewing_root = conn.files_management.viewing_root;
        let show_hidden = conn.files_management.show_hidden;
        conn.files_management.entries = None;
        conn.files_management.error = None;

        // Re-fetch the file list for the current path
        self.send_file_list_request(conn_id, current_path, viewing_root, show_hidden)
    }

    /// Toggle between root view and user area view
    ///
    /// Requires file_root permission. Resets to root directory when toggling.
    pub fn handle_file_toggle_root(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        // Toggle the root view state (also resets path to root)
        conn.files_management.toggle_root();
        let viewing_root = conn.files_management.viewing_root;
        let show_hidden = conn.files_management.show_hidden;

        // Fetch the file list for the new view
        self.send_file_list_request(conn_id, String::new(), viewing_root, show_hidden)
    }

    /// Toggle showing hidden files (dotfiles)
    ///
    /// Toggles the show_hidden flag and refreshes the current directory.
    /// Also saves the preference to config.
    pub fn handle_file_toggle_hidden(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        // Toggle the show_hidden state
        conn.files_management.show_hidden = !conn.files_management.show_hidden;
        let show_hidden = conn.files_management.show_hidden;

        // Save preference to config
        self.config.settings.show_hidden_files = show_hidden;
        let _ = self.config.save();

        // Get current path and root state
        let current_path = conn.files_management.current_path.clone();
        let viewing_root = conn.files_management.viewing_root;

        // Clear entries to show loading state
        conn.files_management.entries = None;
        conn.files_management.error = None;

        // Refresh the file list with new show_hidden setting
        self.send_file_list_request(conn_id, current_path, viewing_root, show_hidden)
    }

    // ==================== New Directory ====================

    /// Handle new directory button click (open dialog)
    pub fn handle_file_new_directory_clicked(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        conn.files_management.open_new_directory_dialog();

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

        conn.files_management.new_directory_name = name;
        conn.files_management.new_directory_error = validation_error;

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

        let name = &conn.files_management.new_directory_name;

        // Validate before sending
        if name.is_empty() {
            conn.files_management.new_directory_error = Some(t("err-dir-name-empty"));
            return Task::none();
        }

        if let Err(e) = validators::validate_dir_name(name) {
            conn.files_management.new_directory_error = Some(dir_name_error_message(e));
            return Task::none();
        }

        // Clone values needed for the request after validation passes
        let name = conn.files_management.new_directory_name.clone();
        let path = conn.files_management.current_path.clone();
        let root = conn.files_management.viewing_root;

        match conn.send(ClientMessage::FileCreateDir { path, name, root }) {
            Ok(message_id) => {
                conn.pending_requests
                    .track(message_id, ResponseRouting::FileCreateDirResult);
            }
            Err(e) => {
                conn.files_management.new_directory_error =
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

        conn.files_management.close_new_directory_dialog();

        Task::none()
    }

    // ==================== Helper Functions ====================

    /// Send a FileList request to the server
    pub fn send_file_list_request(
        &mut self,
        conn_id: usize,
        path: String,
        root: bool,
        show_hidden: bool,
    ) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        match conn.send(ClientMessage::FileList {
            path,
            root,
            show_hidden,
        }) {
            Ok(message_id) => {
                conn.pending_requests
                    .track(message_id, ResponseRouting::PopulateFileList);
            }
            Err(e) => {
                conn.files_management.error = Some(format!("{}: {}", t("err-send-failed"), e));
            }
        }

        Task::none()
    }

    // ==================== File Delete ====================

    /// Handle delete clicked from context menu
    ///
    /// Opens a confirmation dialog with the path to delete.
    pub fn handle_file_delete_clicked(&mut self, path: String) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        // Set pending delete to show confirmation dialog
        conn.files_management.pending_delete = Some(path);

        Task::none()
    }

    /// Handle confirm delete button in modal
    ///
    /// Sends the FileDelete request to the server.
    pub fn handle_file_confirm_delete(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        // Get the path to delete
        let Some(path) = conn.files_management.pending_delete.take() else {
            return Task::none();
        };

        let root = conn.files_management.viewing_root;

        match conn.send(ClientMessage::FileDelete { path, root }) {
            Ok(message_id) => {
                conn.pending_requests
                    .track(message_id, ResponseRouting::FileDeleteResult);
            }
            Err(e) => {
                conn.files_management.error = Some(format!("{}: {}", t("err-send-failed"), e));
            }
        }

        Task::none()
    }

    /// Handle cancel delete (close modal)
    pub fn handle_file_cancel_delete(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        // Clear pending delete to close the dialog
        conn.files_management.pending_delete = None;

        Task::none()
    }
}
