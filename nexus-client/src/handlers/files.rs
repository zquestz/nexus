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
        let show_hidden = self.config.settings.show_hidden_files;

        // Remember the current path - don't reset it
        let tab = conn.files_management.active_tab_mut();
        let current_path = tab.current_path.clone();
        let viewing_root = tab.viewing_root;

        // Clear entries and error to show loading state, but keep the path
        tab.entries = None;
        tab.error = None;

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
        let tab = conn.files_management.active_tab_mut();
        tab.navigate_to(path.clone());
        let viewing_root = tab.viewing_root;
        let show_hidden = self.config.settings.show_hidden_files;

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

        let tab = conn.files_management.active_tab_mut();
        tab.navigate_up();
        let new_path = tab.current_path.clone();
        let viewing_root = tab.viewing_root;
        let show_hidden = self.config.settings.show_hidden_files;

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

        let tab = conn.files_management.active_tab_mut();
        tab.navigate_home();
        let viewing_root = tab.viewing_root;
        let show_hidden = self.config.settings.show_hidden_files;

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

        let tab = conn.files_management.active_tab_mut();
        let current_path = tab.current_path.clone();
        let viewing_root = tab.viewing_root;
        let show_hidden = self.config.settings.show_hidden_files;
        tab.entries = None;
        tab.error = None;

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

        let tab = conn.files_management.active_tab_mut();
        tab.toggle_root();
        let viewing_root = tab.viewing_root;
        let show_hidden = self.config.settings.show_hidden_files;

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

        // Toggle the show_hidden state in config
        let show_hidden = !self.config.settings.show_hidden_files;
        self.config.settings.show_hidden_files = show_hidden;
        let _ = self.config.save();

        // Get current path and root state from active tab
        let tab = conn.files_management.active_tab_mut();
        let current_path = tab.current_path.clone();
        let viewing_root = tab.viewing_root;

        // Clear entries to show loading state
        tab.entries = None;
        tab.error = None;

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

        let tab_id = conn.files_management.active_tab_id();
        match conn.send(ClientMessage::FileCreateDir { path, name, root }) {
            Ok(message_id) => {
                conn.pending_requests
                    .track(message_id, ResponseRouting::FileCreateDirResult { tab_id });
            }
            Err(e) => {
                conn.files_management.active_tab_mut().new_directory_error =
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

    // ==================== Helper Functions ====================

    /// Send a FileList request to the server for the active tab
    ///
    /// This is used for user-initiated navigation (navigate, refresh, etc.)
    /// where we always want to update the currently active tab.
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

        let tab_id = conn.files_management.active_tab_id();
        self.send_file_list_request_for_tab(conn_id, tab_id, path, root, show_hidden)
    }

    /// Send a FileList request to the server for a specific tab
    ///
    /// This is used by response handlers that need to refresh a specific tab
    /// (identified by tab_id) rather than the currently active tab.
    pub fn send_file_list_request_for_tab(
        &mut self,
        conn_id: usize,
        tab_id: crate::types::TabId,
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
                    .track(message_id, ResponseRouting::PopulateFileList { tab_id });
            }
            Err(e) => {
                // Show error on the specific tab if it still exists, otherwise active tab
                if let Some(tab) = conn.files_management.tab_by_id_mut(tab_id) {
                    tab.error = Some(format!("{}: {}", t("err-send-failed"), e));
                } else {
                    conn.files_management.active_tab_mut().error =
                        Some(format!("{}: {}", t("err-send-failed"), e));
                }
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

        let tab = conn.files_management.active_tab_mut();
        tab.pending_delete = Some(path);
        tab.delete_error = None;

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

        let tab = conn.files_management.active_tab_mut();
        let Some(path) = tab.pending_delete.clone() else {
            return Task::none();
        };

        let root = tab.viewing_root;

        // Clear any previous error while the request is in flight
        tab.delete_error = None;

        let tab_id = conn.files_management.active_tab_id();
        match conn.send(ClientMessage::FileDelete { path, root }) {
            Ok(message_id) => {
                conn.pending_requests
                    .track(message_id, ResponseRouting::FileDeleteResult { tab_id });
            }
            Err(e) => {
                conn.files_management.active_tab_mut().delete_error =
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

        let tab = conn.files_management.active_tab_mut();
        tab.pending_delete = None;
        tab.delete_error = None;

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

        let tab = conn.files_management.active_tab();
        let current_path = &tab.current_path;
        let path = build_navigate_path(current_path, &name);
        let root = tab.viewing_root;

        let tab_id = conn.files_management.active_tab_id();
        match conn.send(ClientMessage::FileInfo { path, root }) {
            Ok(message_id) => {
                conn.pending_requests
                    .track(message_id, ResponseRouting::FileInfoResult { tab_id });
            }
            Err(e) => {
                conn.files_management.active_tab_mut().error =
                    Some(format!("{}: {}", t("err-send-failed"), e));
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

        conn.files_management.active_tab_mut().pending_info = None;

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
        let tab = conn.files_management.active_tab_mut();
        let current_path = &tab.current_path;
        let path = build_navigate_path(current_path, &name);

        // Set pending rename to show dialog, pre-populate with actual filesystem name
        // (including any suffixes like [NEXUS-UL] so admin can edit them)
        tab.pending_rename = Some(path);
        tab.rename_name = name;
        tab.rename_error = None;

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

        let tab = conn.files_management.active_tab_mut();
        tab.rename_name = name;
        tab.rename_error = validation_error;

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

        let tab = conn.files_management.active_tab_mut();
        let new_name = &tab.rename_name;

        // Validate before sending
        if new_name.is_empty() {
            tab.rename_error = Some(t("err-dir-name-empty"));
            return Task::none();
        }

        if let Err(e) = validators::validate_dir_name(new_name) {
            tab.rename_error = Some(dir_name_error_message(e));
            return Task::none();
        }

        // Get the path to rename (don't take - keep dialog open until response)
        let Some(path) = tab.pending_rename.clone() else {
            return Task::none();
        };

        let new_name = tab.rename_name.clone();
        let root = tab.viewing_root;

        // Clear any previous error before sending
        conn.files_management.active_tab_mut().rename_error = None;

        let tab_id = conn.files_management.active_tab_id();
        match conn.send(ClientMessage::FileRename {
            path,
            new_name,
            root,
        }) {
            Ok(message_id) => {
                conn.pending_requests
                    .track(message_id, ResponseRouting::FileRenameResult { tab_id });
            }
            Err(e) => {
                conn.files_management.active_tab_mut().rename_error =
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

        let tab = conn.files_management.active_tab_mut();
        tab.pending_rename = None;
        tab.rename_name = String::new();
        tab.rename_error = None;

        Task::none()
    }

    // ==================== Clipboard Operations ====================

    /// Handle cut action from context menu
    ///
    /// Stores the file/directory in clipboard for later move operation.
    pub fn handle_file_cut(&mut self, path: String, name: String) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        let root = conn.files_management.active_tab().viewing_root;
        conn.files_management.clipboard = Some(ClipboardItem {
            path,
            name,
            operation: ClipboardOperation::Cut,
            root,
        });

        Task::none()
    }

    /// Handle copy action from context menu
    ///
    /// Stores the file/directory in clipboard for later copy operation.
    pub fn handle_file_copy_to_clipboard(&mut self, path: String, name: String) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        let root = conn.files_management.active_tab().viewing_root;
        conn.files_management.clipboard = Some(ClipboardItem {
            path,
            name,
            operation: ClipboardOperation::Copy,
            root,
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

        let destination_dir = conn.files_management.active_tab().current_path.clone();
        self.send_paste_request(conn_id, destination_dir)
    }

    /// Handle paste into specific directory (from context menu on folder)
    ///
    /// Sends FileMove or FileCopy request to the specified directory.
    pub fn handle_file_paste_into(&mut self, destination_dir: String) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };

        self.send_paste_request(conn_id, destination_dir)
    }

    /// Send a paste (move/copy) request to the server
    ///
    /// Helper for handle_file_paste and handle_file_paste_into.
    fn send_paste_request(&mut self, conn_id: usize, destination_dir: String) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        let Some(clipboard) = conn.files_management.clipboard.clone() else {
            return Task::none();
        };

        let source_root = clipboard.root;
        let destination_root = conn.files_management.active_tab().viewing_root;
        let tab_id = conn.files_management.active_tab_id();
        let is_move = matches!(clipboard.operation, ClipboardOperation::Cut);

        let message = if is_move {
            ClientMessage::FileMove {
                source_path: clipboard.path,
                destination_dir: destination_dir.clone(),
                overwrite: false,
                source_root,
                destination_root,
            }
        } else {
            ClientMessage::FileCopy {
                source_path: clipboard.path,
                destination_dir: destination_dir.clone(),
                overwrite: false,
                source_root,
                destination_root,
            }
        };

        let Ok(message_id) = conn.send(message) else {
            return Task::none();
        };

        let routing = if is_move {
            ResponseRouting::FileMoveResult {
                tab_id,
                destination_dir,
            }
        } else {
            ResponseRouting::FileCopyResult {
                tab_id,
                destination_dir,
            }
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

        let Some(pending) = conn
            .files_management
            .active_tab_mut()
            .pending_overwrite
            .take()
        else {
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

        let tab_id = conn.files_management.active_tab_id();
        let routing = if pending.is_move {
            ResponseRouting::FileMoveResult {
                tab_id,
                destination_dir: destination_dir_for_routing,
            }
        } else {
            ResponseRouting::FileCopyResult {
                tab_id,
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

        conn.files_management.active_tab_mut().pending_overwrite = None;

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

        let tab = conn.files_management.active_tab_mut();
        if tab.sort_column == column {
            // Toggle direction
            tab.sort_ascending = !tab.sort_ascending;
        } else {
            // Switch to new column, ascending
            tab.sort_column = column;
            tab.sort_ascending = true;
        }

        // Rebuild sorted entries cache
        tab.update_sorted_entries();

        Task::none()
    }

    // ==================== Tab Management ====================

    /// Create a new file tab (clones current tab's location and settings)
    pub fn handle_file_tab_new(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        // Create new tab cloned from current (new_tab sets it as active)
        conn.files_management.new_tab();

        // Fetch file list for the new tab (now the active tab)
        let tab = conn.files_management.active_tab();
        let current_path = tab.current_path.clone();
        let viewing_root = tab.viewing_root;
        let show_hidden = self.config.settings.show_hidden_files;

        self.send_file_list_request(conn_id, current_path, viewing_root, show_hidden)
    }

    /// Switch to a file tab by ID
    pub fn handle_file_tab_switch(&mut self, tab_id: crate::types::TabId) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        conn.files_management.switch_to_tab_by_id(tab_id);
        Task::none()
    }

    /// Close a file tab by ID
    pub fn handle_file_tab_close(&mut self, tab_id: crate::types::TabId) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        conn.files_management.close_tab_by_id(tab_id);
        Task::none()
    }

    // ==================== Downloads ====================

    /// Handle file download request (single file)
    ///
    /// Creates a new transfer in the transfer manager and queues it for download.
    pub fn handle_file_download(&mut self, path: String) -> Task<Message> {
        self.queue_download(path, false)
    }

    /// Handle directory download request (recursive)
    ///
    /// Creates a new transfer in the transfer manager and queues it for download.
    pub fn handle_file_download_all(&mut self, path: String) -> Task<Message> {
        self.queue_download(path, true)
    }

    /// Queue a download transfer
    ///
    /// Creates a Transfer with Queued status and adds it to the transfer manager.
    fn queue_download(&mut self, remote_path: String, is_directory: bool) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get(&conn_id) else {
            return Task::none();
        };

        // Get the current viewing mode (root or user area)
        let remote_root = conn.files_management.active_tab().viewing_root;

        // Build local path from download directory + remote filename
        let download_dir = self
            .config
            .settings
            .download_path
            .clone()
            .or_else(crate::config::settings::default_download_path)
            .unwrap_or_else(|| ".".to_string());

        // Extract filename from remote path
        // For single files: use the filename
        // For directories: use the directory name as the containing folder
        // For root path ("/") downloads: use server name as the folder
        let trimmed_path = remote_path.trim_matches('/');
        let local_path = if is_directory && trimmed_path.is_empty() {
            // Root directory download - use server name as folder
            // Sanitize server name to be filesystem-safe, fall back to address
            let safe_name = sanitize_filename(
                &conn.connection_info.server_name,
                &conn.connection_info.address,
            );
            std::path::PathBuf::from(&download_dir).join(safe_name)
        } else {
            // Extract last path component for the local filename/folder
            // trimmed_path is guaranteed non-empty here, so rsplit will return a non-empty value
            let filename = trimmed_path.rsplit('/').next().expect("non-empty path");
            std::path::PathBuf::from(&download_dir).join(filename)
        };

        // Create the transfer
        let transfer = crate::transfers::Transfer::new_download(
            conn.connection_info.clone(),
            remote_path,
            remote_root,
            is_directory,
            local_path,
            conn.bookmark_id,
        );

        // Add to transfer manager
        self.transfer_manager.add(transfer);

        // Save transfers to disk
        let _ = self.transfer_manager.save();

        Task::none()
    }
}

/// Sanitize a string to be safe for use as a filename
///
/// Replaces characters that are invalid in filenames on various platforms.
/// Falls back to the provided fallback if the result would be empty.
fn sanitize_filename(name: &str, fallback: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|c| match c {
            // Invalid on Windows and/or problematic on Unix
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            // Control characters
            c if c.is_control() => '_',
            c => c,
        })
        .collect();

    // Trim whitespace and dots from ends (Windows doesn't like trailing dots/spaces)
    let trimmed = sanitized.trim().trim_end_matches('.');

    // If empty after sanitization, use the fallback (typically server address)
    if trimmed.is_empty() {
        return fallback.to_string();
    }

    // Check for Windows reserved names (case-insensitive)
    // These cannot be used as filenames on Windows, even with extensions
    let upper = trimmed.to_uppercase();
    let is_reserved = matches!(
        upper.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    );

    if is_reserved {
        // Prefix with underscore to make it safe
        format!("_{trimmed}")
    } else {
        trimmed.to_string()
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_filename_normal() {
        assert_eq!(sanitize_filename("My Server", "fallback"), "My Server");
        assert_eq!(sanitize_filename("test", "fallback"), "test");
        assert_eq!(sanitize_filename("server123", "fallback"), "server123");
    }

    #[test]
    fn test_sanitize_filename_invalid_chars() {
        assert_eq!(sanitize_filename("foo/bar", "fallback"), "foo_bar");
        assert_eq!(sanitize_filename("foo\\bar", "fallback"), "foo_bar");
        assert_eq!(sanitize_filename("foo:bar", "fallback"), "foo_bar");
        assert_eq!(sanitize_filename("foo*bar", "fallback"), "foo_bar");
        assert_eq!(sanitize_filename("foo?bar", "fallback"), "foo_bar");
        assert_eq!(sanitize_filename("foo\"bar", "fallback"), "foo_bar");
        assert_eq!(sanitize_filename("foo<bar", "fallback"), "foo_bar");
        assert_eq!(sanitize_filename("foo>bar", "fallback"), "foo_bar");
        assert_eq!(sanitize_filename("foo|bar", "fallback"), "foo_bar");
        // Multiple invalid chars
        assert_eq!(sanitize_filename("a/b\\c:d", "fallback"), "a_b_c_d");
    }

    #[test]
    fn test_sanitize_filename_control_chars() {
        assert_eq!(sanitize_filename("foo\x00bar", "fallback"), "foo_bar");
        assert_eq!(sanitize_filename("foo\nbar", "fallback"), "foo_bar");
        assert_eq!(sanitize_filename("foo\tbar", "fallback"), "foo_bar");
    }

    #[test]
    fn test_sanitize_filename_trailing_dots_spaces() {
        assert_eq!(sanitize_filename("test.", "fallback"), "test");
        assert_eq!(sanitize_filename("test...", "fallback"), "test");
        assert_eq!(sanitize_filename("test ", "fallback"), "test");
        assert_eq!(sanitize_filename(" test ", "fallback"), "test");
        assert_eq!(sanitize_filename("test. ", "fallback"), "test");
    }

    #[test]
    fn test_sanitize_filename_empty_fallback() {
        assert_eq!(sanitize_filename("", "fallback"), "fallback");
        assert_eq!(sanitize_filename("   ", "fallback"), "fallback");
        assert_eq!(sanitize_filename("...", "fallback"), "fallback");
        // Note: "///" becomes "___" (slashes replaced), not fallback
        assert_eq!(sanitize_filename("///", "192.168.1.1"), "___");
    }

    #[test]
    fn test_sanitize_filename_windows_reserved() {
        // Reserved names should be prefixed with underscore
        assert_eq!(sanitize_filename("CON", "fallback"), "_CON");
        assert_eq!(sanitize_filename("con", "fallback"), "_con");
        assert_eq!(sanitize_filename("Con", "fallback"), "_Con");
        assert_eq!(sanitize_filename("PRN", "fallback"), "_PRN");
        assert_eq!(sanitize_filename("AUX", "fallback"), "_AUX");
        assert_eq!(sanitize_filename("NUL", "fallback"), "_NUL");
        assert_eq!(sanitize_filename("COM1", "fallback"), "_COM1");
        assert_eq!(sanitize_filename("COM9", "fallback"), "_COM9");
        assert_eq!(sanitize_filename("LPT1", "fallback"), "_LPT1");
        assert_eq!(sanitize_filename("LPT9", "fallback"), "_LPT9");
    }

    #[test]
    fn test_sanitize_filename_unicode() {
        // Unicode should pass through unchanged
        assert_eq!(sanitize_filename("服务器", "fallback"), "服务器");
        assert_eq!(sanitize_filename("サーバー", "fallback"), "サーバー");
        assert_eq!(sanitize_filename("Сервер", "fallback"), "Сервер");
    }
}
