//! Files panel handlers

use iced::Task;
use iced::widget::{Id, operation};
use nexus_common::protocol::ClientMessage;
use nexus_common::validators::{self, DirNameError};

use crate::NexusApp;
use crate::i18n::t;
use crate::types::{
    ActivePanel, ClipboardItem, ClipboardOperation, InputId, Message, PendingRequests,
    ResponseRouting,
};
use crate::views::files::build_navigate_path;

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

        // Set pending delete to show confirmation dialog, clear any previous error
        conn.files_management.pending_delete = Some(path);
        conn.files_management.delete_error = None;

        Task::none()
    }

    /// Handle confirm delete button in modal
    ///
    /// Sends the FileDelete request to the server.
    /// Keeps the dialog open until we get a response (success closes it, error shows in dialog).
    pub fn handle_file_confirm_delete(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        // Get the path to delete (don't take - keep dialog open until response)
        let Some(path) = conn.files_management.pending_delete.clone() else {
            return Task::none();
        };

        let root = conn.files_management.viewing_root;

        // Clear any previous error before sending
        conn.files_management.delete_error = None;

        match conn.send(ClientMessage::FileDelete { path, root }) {
            Ok(message_id) => {
                conn.pending_requests
                    .track(message_id, ResponseRouting::FileDeleteResult);
            }
            Err(e) => {
                // Show send error in the delete dialog
                conn.files_management.delete_error =
                    Some(format!("{}: {}", t("err-send-failed"), e));
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

        // Clear pending delete and any error to close the dialog
        conn.files_management.pending_delete = None;
        conn.files_management.delete_error = None;

        Task::none()
    }

    // ==================== File Info ====================

    /// Handle info clicked from context menu
    ///
    /// Sends a FileInfo request to the server to get detailed information.
    pub fn handle_file_info_clicked(&mut self, name: String) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        // Build the full path for this entry
        let current_path = &conn.files_management.current_path;
        let path = build_navigate_path(current_path, &name);
        let root = conn.files_management.viewing_root;

        match conn.send(ClientMessage::FileInfo { path, root }) {
            Ok(message_id) => {
                conn.pending_requests
                    .track(message_id, ResponseRouting::FileInfoResult);
            }
            Err(e) => {
                conn.files_management.error = Some(format!("{}: {}", t("err-send-failed"), e));
            }
        }

        Task::none()
    }

    /// Handle close file info dialog
    pub fn handle_close_file_info(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        // Clear pending info to close the dialog
        conn.files_management.pending_info = None;

        Task::none()
    }

    // ==================== File Rename ====================

    /// Handle rename clicked from context menu
    ///
    /// Opens a rename dialog with the current name pre-populated.
    pub fn handle_file_rename_clicked(&mut self, name: String) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        // Build the full path for this entry
        let current_path = &conn.files_management.current_path;
        let path = build_navigate_path(current_path, &name);

        // Set pending rename to show dialog, pre-populate with actual filesystem name
        // (including any suffixes like [NEXUS-UL] so admin can edit them)
        conn.files_management.pending_rename = Some(path);
        conn.files_management.rename_name = name;
        conn.files_management.rename_error = None;

        // Focus the name input field
        operation::focus(Id::from(InputId::RenameName))
    }

    /// Handle rename name input change
    pub fn handle_file_rename_name_changed(&mut self, name: String) -> Task<Message> {
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

        conn.files_management.rename_name = name;
        conn.files_management.rename_error = validation_error;

        Task::none()
    }

    /// Handle rename submit button
    pub fn handle_file_rename_submit(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        let new_name = &conn.files_management.rename_name;

        // Validate before sending
        if new_name.is_empty() {
            conn.files_management.rename_error = Some(t("err-dir-name-empty"));
            return Task::none();
        }

        if let Err(e) = validators::validate_dir_name(new_name) {
            conn.files_management.rename_error = Some(dir_name_error_message(e));
            return Task::none();
        }

        // Get the path to rename (don't take - keep dialog open until response)
        let Some(path) = conn.files_management.pending_rename.clone() else {
            return Task::none();
        };

        let new_name = conn.files_management.rename_name.clone();
        let root = conn.files_management.viewing_root;

        // Clear any previous error before sending
        conn.files_management.rename_error = None;

        match conn.send(ClientMessage::FileRename {
            path,
            new_name,
            root,
        }) {
            Ok(message_id) => {
                conn.pending_requests
                    .track(message_id, ResponseRouting::FileRenameResult);
            }
            Err(e) => {
                // Show send error in the rename dialog
                conn.files_management.rename_error =
                    Some(format!("{}: {}", t("err-send-failed"), e));
            }
        }

        Task::none()
    }

    /// Handle rename cancel button (close dialog)
    pub fn handle_file_rename_cancel(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        // Clear pending rename and any error to close the dialog
        conn.files_management.pending_rename = None;
        conn.files_management.rename_name = String::new();
        conn.files_management.rename_error = None;

        Task::none()
    }

    // ==================== Clipboard Operations ====================

    /// Handle cut action from context menu
    ///
    /// Stores the file/directory in clipboard for later move operation.
    pub fn handle_file_cut(
        &mut self,
        path: String,
        name: String,
        is_directory: bool,
    ) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        conn.files_management.clipboard = Some(ClipboardItem {
            path,
            name,
            is_directory,
            operation: ClipboardOperation::Cut,
            root: conn.files_management.viewing_root,
        });

        Task::none()
    }

    /// Handle copy action from context menu
    ///
    /// Stores the file/directory in clipboard for later copy operation.
    pub fn handle_file_copy_to_clipboard(
        &mut self,
        path: String,
        name: String,
        is_directory: bool,
    ) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        conn.files_management.clipboard = Some(ClipboardItem {
            path,
            name,
            is_directory,
            operation: ClipboardOperation::Copy,
            root: conn.files_management.viewing_root,
        });

        Task::none()
    }

    /// Handle paste action (to current directory)
    ///
    /// Sends FileMove or FileCopy request based on clipboard operation.
    pub fn handle_file_paste(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        let Some(clipboard) = conn.files_management.clipboard.clone() else {
            return Task::none();
        };

        let destination_dir = conn.files_management.current_path.clone();
        let source_root = clipboard.root;
        let destination_root = conn.files_management.viewing_root;

        let message = match clipboard.operation {
            ClipboardOperation::Cut => ClientMessage::FileMove {
                source_path: clipboard.path,
                destination_dir: destination_dir.clone(),
                overwrite: false,
                source_root,
                destination_root,
            },
            ClipboardOperation::Copy => ClientMessage::FileCopy {
                source_path: clipboard.path,
                destination_dir: destination_dir.clone(),
                overwrite: false,
                source_root,
                destination_root,
            },
        };

        let Ok(message_id) = conn.send(message) else {
            return Task::none();
        };

        let routing = match clipboard.operation {
            ClipboardOperation::Cut => ResponseRouting::FileMoveResult { destination_dir },
            ClipboardOperation::Copy => ResponseRouting::FileCopyResult { destination_dir },
        };
        conn.pending_requests.track(message_id, routing);

        Task::none()
    }

    /// Handle paste into specific directory (from context menu on folder)
    ///
    /// Sends FileMove or FileCopy request to the specified directory.
    pub fn handle_file_paste_into(&mut self, destination_dir: String) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        let Some(clipboard) = conn.files_management.clipboard.clone() else {
            return Task::none();
        };

        let source_root = clipboard.root;
        let destination_root = conn.files_management.viewing_root;

        let message = match clipboard.operation {
            ClipboardOperation::Cut => ClientMessage::FileMove {
                source_path: clipboard.path,
                destination_dir: destination_dir.clone(),
                overwrite: false,
                source_root,
                destination_root,
            },
            ClipboardOperation::Copy => ClientMessage::FileCopy {
                source_path: clipboard.path,
                destination_dir: destination_dir.clone(),
                overwrite: false,
                source_root,
                destination_root,
            },
        };

        let Ok(message_id) = conn.send(message) else {
            return Task::none();
        };

        let routing = match clipboard.operation {
            ClipboardOperation::Cut => ResponseRouting::FileMoveResult { destination_dir },
            ClipboardOperation::Copy => ResponseRouting::FileCopyResult { destination_dir },
        };
        conn.pending_requests.track(message_id, routing);

        Task::none()
    }

    /// Handle clear clipboard action (Escape key)
    pub fn handle_file_clear_clipboard(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        conn.files_management.clipboard = None;

        Task::none()
    }

    /// Handle overwrite confirm button in dialog
    ///
    /// Resends the move/copy request with overwrite: true.
    pub fn handle_file_overwrite_confirm(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        let Some(pending) = conn.files_management.pending_overwrite.take() else {
            return Task::none();
        };

        // Clone destination_dir for routing before moving into message
        let destination_dir_for_routing = pending.destination_dir.clone();

        let message = if pending.is_move {
            ClientMessage::FileMove {
                source_path: pending.source_path,
                destination_dir: pending.destination_dir,
                overwrite: true,
                source_root: pending.source_root,
                destination_root: pending.destination_root,
            }
        } else {
            ClientMessage::FileCopy {
                source_path: pending.source_path,
                destination_dir: pending.destination_dir,
                overwrite: true,
                source_root: pending.source_root,
                destination_root: pending.destination_root,
            }
        };

        let Ok(message_id) = conn.send(message) else {
            return Task::none();
        };

        let routing = if pending.is_move {
            ResponseRouting::FileMoveResult {
                destination_dir: destination_dir_for_routing,
            }
        } else {
            ResponseRouting::FileCopyResult {
                destination_dir: destination_dir_for_routing,
            }
        };
        conn.pending_requests.track(message_id, routing);

        Task::none()
    }

    /// Handle overwrite cancel button in dialog
    pub fn handle_file_overwrite_cancel(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        conn.files_management.pending_overwrite = None;

        Task::none()
    }

    /// Handle sort by column click
    ///
    /// Clicking the active column toggles ascending/descending.
    /// Clicking a different column switches to that column (ascending).
    pub fn handle_file_sort_by(&mut self, column: crate::types::FileSortColumn) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        if conn.files_management.sort_column == column {
            // Toggle direction
            conn.files_management.sort_ascending = !conn.files_management.sort_ascending;
        } else {
            // Switch to new column, ascending
            conn.files_management.sort_column = column;
            conn.files_management.sort_ascending = true;
        }

        // Rebuild sorted entries cache
        conn.files_management.update_sorted_entries();

        Task::none()
    }
}
