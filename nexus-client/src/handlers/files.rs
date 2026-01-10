//! Files panel handlers

use iced::Task;
use iced::widget::{Id, operation};
use nexus_common::protocol::{ClientMessage, FileSearchResult};
use nexus_common::validators::{self, DirNameError};

use crate::NexusApp;
use crate::i18n::t;
use crate::types::{
    ActivePanel, ClipboardItem, ClipboardOperation, FileSortColumn, InputId, Message,
    PendingRequests, ResponseRouting,
};
use crate::views::files::build_navigate_path;

/// Sort search results by the specified column and direction
///
/// For the Name column, directories are always sorted first.
/// For other columns, ties are broken by name (case-insensitive, ascending).
pub fn sort_search_results(
    results: &mut [FileSearchResult],
    column: FileSortColumn,
    ascending: bool,
) {
    match column {
        FileSortColumn::Name => {
            // Sort by name, keeping directories first
            results.sort_by(|a, b| match (a.is_directory, b.is_directory) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => {
                    let cmp = a.name.to_lowercase().cmp(&b.name.to_lowercase());
                    if ascending { cmp } else { cmp.reverse() }
                }
            });
        }
        FileSortColumn::Path => {
            results.sort_by(|a, b| {
                let cmp = a.path.to_lowercase().cmp(&b.path.to_lowercase());
                let cmp = if ascending { cmp } else { cmp.reverse() };
                // Sub-sort by name for items in same directory
                cmp.then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            });
        }
        FileSortColumn::Size => {
            results.sort_by(|a, b| {
                let cmp = a.size.cmp(&b.size);
                let cmp = if ascending { cmp } else { cmp.reverse() };
                // Sub-sort by name for items with same size
                cmp.then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            });
        }
        FileSortColumn::Modified => {
            results.sort_by(|a, b| {
                let cmp = a.modified.cmp(&b.modified);
                let cmp = if ascending { cmp } else { cmp.reverse() };
                // Sub-sort by name for items with same modified time
                cmp.then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            });
        }
    }
}

/// Convert a directory name validation error to a localized error message
fn dir_name_error_message(error: DirNameError) -> String {
    match error {
        DirNameError::Empty => t("err-dir-name-empty"),
        DirNameError::TooLong => crate::i18n::t_args(
            "err-dir-name-too-long",
            &[("max", &validators::MAX_DIR_NAME_LENGTH.to_string())],
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

        // If in search mode, re-run the search
        if let Some(query) = tab.search_query.clone() {
            tab.search_loading = true;
            tab.search_results = None;
            tab.search_error = None;

            let tab_id = tab.id;
            let viewing_root = tab.viewing_root;

            let message = ClientMessage::FileSearch {
                query,
                root: viewing_root,
            };

            match conn.send(message) {
                Ok(message_id) => {
                    conn.pending_requests
                        .track(message_id, ResponseRouting::FileSearchResult { tab_id });
                }
                Err(err) => {
                    eprintln!("Failed to send file search request: {err}");
                    if let Some(tab) = conn.files_management.tab_by_id_mut(tab_id) {
                        tab.search_loading = false;
                        tab.search_error = Some(err);
                    }
                }
            }

            return Task::none();
        }

        // Normal browsing mode - refresh file list
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
    /// Requires file_root permission.
    /// In search mode: re-runs the search with toggled scope.
    /// In browsing mode: resets to root directory when toggling.
    pub fn handle_file_toggle_root(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        let tab = conn.files_management.active_tab_mut();

        // If in search mode, toggle root and re-run search
        if let Some(query) = tab.search_query.clone() {
            tab.viewing_root = !tab.viewing_root;
            tab.search_loading = true;
            tab.search_results = None;
            tab.search_error = None;

            let tab_id = tab.id;
            let viewing_root = tab.viewing_root;

            let message = ClientMessage::FileSearch {
                query,
                root: viewing_root,
            };

            match conn.send(message) {
                Ok(message_id) => {
                    conn.pending_requests
                        .track(message_id, ResponseRouting::FileSearchResult { tab_id });
                }
                Err(err) => {
                    eprintln!("Failed to send file search request: {err}");
                    if let Some(tab) = conn.files_management.tab_by_id_mut(tab_id) {
                        tab.search_loading = false;
                        tab.search_error = Some(err);
                    }
                }
            }

            return Task::none();
        }

        // Normal browsing mode - toggle and go to root/home
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

    // ==================== Uploads ====================

    /// Handle upload request - opens file picker for multiple files
    ///
    /// The destination path is where files will be uploaded to on the server.
    ///
    /// Note: The `rfd` crate's `pick_files()` only allows selecting files, not folders.
    /// There's no cross-platform way to select both files and folders in a single dialog.
    /// Directory upload is fully supported in the executor - we just need a separate
    /// folder picker trigger (e.g., "Upload Folder" menu item or drag-and-drop) to use it.
    pub fn handle_file_upload(&mut self, destination: String) -> Task<Message> {
        let destination_clone = destination.clone();
        Task::perform(
            async move {
                let handle = rfd::AsyncFileDialog::new()
                    .set_title(t("file-picker-upload-title"))
                    .pick_files()
                    .await;

                match handle {
                    Some(files) => {
                        let paths: Vec<std::path::PathBuf> =
                            files.into_iter().map(|f| f.path().to_path_buf()).collect();
                        Message::FileUploadSelected(destination_clone, paths)
                    }
                    None => {
                        // User cancelled - no-op, keeps panel open
                        Message::FileUploadCancelled
                    }
                }
            },
            |msg| msg,
        )
    }

    /// Handle file picker result - queue uploads
    pub fn handle_file_upload_selected(
        &mut self,
        destination: String,
        paths: Vec<std::path::PathBuf>,
    ) -> Task<Message> {
        if paths.is_empty() {
            return Task::none();
        }

        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get(&conn_id) else {
            return Task::none();
        };

        // Get the current viewing mode (root or user area)
        let remote_root = conn.files_management.active_tab().viewing_root;

        // Queue each selected file/directory as a separate upload
        for local_path in paths {
            let is_directory = local_path.is_dir();

            // For directory uploads, append the directory name to the destination
            // so the server creates the directory structure (e.g., "/Uploads/MyFolder/")
            let remote_path = if is_directory {
                if let Some(dir_name) = local_path.file_name().and_then(|n| n.to_str()) {
                    if destination.is_empty() || destination == "/" {
                        format!("/{dir_name}")
                    } else {
                        format!("{}/{}", destination.trim_end_matches('/'), dir_name)
                    }
                } else {
                    destination.clone()
                }
            } else {
                destination.clone()
            };

            // Create the transfer
            let transfer = crate::transfers::Transfer::new_upload(
                conn.connection_info.clone(),
                remote_path,
                remote_root,
                is_directory,
                local_path,
                conn.bookmark_id,
            );

            // Add to transfer manager
            self.transfer_manager.add(transfer);
        }

        // Save transfers to disk
        let _ = self.transfer_manager.save();

        Task::none()
    }

    // ==================== Drag and Drop ====================

    /// Handle file being dragged over window
    ///
    /// Just sets the dragging flag - visual feedback is handled in the view.
    pub fn handle_file_drag_hovered(&mut self) -> Task<Message> {
        self.dragging_files = true;
        Task::none()
    }

    /// Handle file dropped on window
    ///
    /// If we're in a valid upload context (Files panel active, uploadable folder,
    /// file_upload permission), queue the dropped file/folder for upload.
    pub fn handle_file_drag_dropped(&mut self, path: std::path::PathBuf) -> Task<Message> {
        // Clear dragging state
        self.dragging_files = false;

        // Check if we can accept the drop
        if !self.can_accept_file_drop() {
            return Task::none();
        }

        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get(&conn_id) else {
            return Task::none();
        };

        // Get upload destination (current directory)
        let destination = conn.files_management.active_tab().current_path.clone();
        let remote_root = conn.files_management.active_tab().viewing_root;
        let is_directory = path.is_dir();

        // For directory uploads, append the directory name to the destination
        // so the server creates the directory structure (e.g., "/Uploads/MyFolder/")
        let remote_path = if is_directory {
            if let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) {
                if destination.is_empty() || destination == "/" {
                    format!("/{dir_name}")
                } else {
                    format!("{}/{}", destination.trim_end_matches('/'), dir_name)
                }
            } else {
                destination.clone()
            }
        } else {
            destination.clone()
        };

        // Create the transfer
        let transfer = crate::transfers::Transfer::new_upload(
            conn.connection_info.clone(),
            remote_path,
            remote_root,
            is_directory,
            path,
            conn.bookmark_id,
        );

        // Add to transfer manager
        self.transfer_manager.add(transfer);

        // Save transfers to disk
        let _ = self.transfer_manager.save();

        Task::none()
    }

    /// Handle drag leaving window
    pub fn handle_file_drag_left(&mut self) -> Task<Message> {
        self.dragging_files = false;
        Task::none()
    }

    /// Check if we can accept a file drop for upload
    ///
    /// Returns true if:
    /// - Files panel is active
    /// - Current directory allows uploads
    /// - User has file_upload permission
    pub fn can_accept_file_drop(&self) -> bool {
        use crate::views::constants::PERMISSION_FILE_UPLOAD;

        let Some(conn_id) = self.active_connection else {
            return false;
        };
        let Some(conn) = self.connections.get(&conn_id) else {
            return false;
        };

        // Must be in Files panel
        if conn.active_panel != ActivePanel::Files {
            return false;
        }

        // Must have file_upload permission
        if !conn.has_permission(PERMISSION_FILE_UPLOAD) {
            return false;
        }

        // Current directory must allow uploads
        conn.files_management.active_tab().current_dir_can_upload
    }

    // ==================== File Search ====================

    /// Handle search input text change
    pub fn handle_file_search_input_changed(&mut self, value: String) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        let tab = conn.files_management.active_tab_mut();
        tab.search_input = value;

        // Don't auto-clear search when input is emptied - let user explicitly
        // submit (Enter or button) to exit search mode. This allows them to
        // clear and type a new search without losing current results.

        Task::none()
    }

    /// Handle search submit (Enter or button click)
    pub fn handle_file_search_submit(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        let tab = conn.files_management.active_tab_mut();
        let query = tab.search_input.trim().to_string();

        // If query is empty, exit search mode and refresh the file list
        if query.is_empty() {
            let was_searching = tab.is_searching();
            tab.clear_search();

            // Refresh the file list to return to where we were
            if was_searching {
                let current_path = tab.current_path.clone();
                let viewing_root = tab.viewing_root;
                let show_hidden = self.config.settings.show_hidden_files;
                return self.send_file_list_request(
                    conn_id,
                    current_path,
                    viewing_root,
                    show_hidden,
                );
            }
            return Task::none();
        }

        // Set loading state
        tab.search_query = Some(query.clone());
        tab.search_loading = true;
        tab.search_results = None;
        tab.search_error = None;

        let tab_id = tab.id;
        let viewing_root = tab.viewing_root;

        // Send search request
        let message = ClientMessage::FileSearch {
            query,
            root: viewing_root,
        };

        match conn.send(message) {
            Ok(message_id) => {
                conn.pending_requests
                    .track(message_id, ResponseRouting::FileSearchResult { tab_id });
            }
            Err(err) => {
                eprintln!("Failed to send file search request: {err}");
                if let Some(tab) = conn.files_management.tab_by_id_mut(tab_id) {
                    tab.search_loading = false;
                    tab.search_error = Some(err);
                }
            }
        }

        Task::none()
    }

    /// Handle search result click (left-click) - opens new tab
    pub fn handle_file_search_result_clicked(
        &mut self,
        result: nexus_common::protocol::FileSearchResult,
    ) -> Task<Message> {
        self.open_search_result_in_new_tab(result)
    }

    /// Handle search result context menu - Download
    pub fn handle_file_search_result_download(
        &mut self,
        result: nexus_common::protocol::FileSearchResult,
    ) -> Task<Message> {
        // Queue the download using the result's path
        // Strip leading slash for the download path
        let path = result.path.strip_prefix('/').unwrap_or(&result.path);

        if result.is_directory {
            self.handle_file_download_all(path.to_string())
        } else {
            self.handle_file_download(path.to_string())
        }
    }

    /// Handle search result context menu - Info
    pub fn handle_file_search_result_info(
        &mut self,
        result: nexus_common::protocol::FileSearchResult,
    ) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        let tab_id = conn.files_management.active_tab_id();
        let viewing_root = conn.files_management.active_tab().viewing_root;

        // Strip leading slash for the path
        let path = result.path.strip_prefix('/').unwrap_or(&result.path);

        let message = ClientMessage::FileInfo {
            path: path.to_string(),
            root: viewing_root,
        };

        match conn.send(message) {
            Ok(message_id) => {
                conn.pending_requests
                    .track(message_id, ResponseRouting::FileInfoResult { tab_id });
            }
            Err(err) => {
                eprintln!("Failed to send file info request: {err}");
            }
        }

        Task::none()
    }

    /// Handle search result context menu - Open (same as click)
    pub fn handle_file_search_result_open(
        &mut self,
        result: nexus_common::protocol::FileSearchResult,
    ) -> Task<Message> {
        self.open_search_result_in_new_tab(result)
    }

    /// Open a search result in a new tab
    ///
    /// For directories: navigates into the directory
    /// For files: navigates to the parent directory
    fn open_search_result_in_new_tab(
        &mut self,
        result: nexus_common::protocol::FileSearchResult,
    ) -> Task<Message> {
        use crate::types::FileTab;

        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        // Determine the target path
        let target_path = if result.is_directory {
            // Navigate into the directory
            result
                .path
                .strip_prefix('/')
                .unwrap_or(&result.path)
                .to_string()
        } else {
            // Navigate to parent directory
            let path = result.path.strip_prefix('/').unwrap_or(&result.path);
            if let Some(pos) = path.rfind('/') {
                path[..pos].to_string()
            } else {
                // File is at root
                String::new()
            }
        };

        // Get viewing_root from current search tab
        let viewing_root = conn.files_management.active_tab().viewing_root;

        // Create new tab at target path
        let new_tab = FileTab::new_at_path(target_path.clone(), viewing_root);
        let new_tab_id = new_tab.id;

        // Add and switch to the new tab
        conn.files_management.tabs.push(new_tab);
        conn.files_management.active_tab = conn.files_management.tabs.len() - 1;

        // Request file list for the new tab
        let message = ClientMessage::FileList {
            path: target_path,
            root: viewing_root,
            show_hidden: self.config.settings.show_hidden_files,
        };

        match conn.send(message) {
            Ok(message_id) => {
                conn.pending_requests.track(
                    message_id,
                    ResponseRouting::PopulateFileList { tab_id: new_tab_id },
                );
            }
            Err(err) => {
                eprintln!("Failed to send file list request: {err}");
                if let Some(tab) = conn.files_management.tab_by_id_mut(new_tab_id) {
                    tab.error = Some(err);
                }
            }
        }

        Task::none()
    }

    /// Handle search results sort column click
    pub fn handle_file_search_sort_by(&mut self, column: FileSortColumn) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        let tab = conn.files_management.active_tab_mut();

        // Toggle direction if clicking same column, otherwise set new column ascending
        if tab.search_sort_column == column {
            tab.search_sort_ascending = !tab.search_sort_ascending;
        } else {
            tab.search_sort_column = column;
            tab.search_sort_ascending = true;
        }

        // Sort the search results in place
        if let Some(results) = &mut tab.search_results {
            sort_search_results(results, column, tab.search_sort_ascending);
        }

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

    // =========================================================================
    // sort_search_results Tests
    // =========================================================================

    fn make_search_result(
        name: &str,
        path: &str,
        size: u64,
        is_directory: bool,
    ) -> nexus_common::protocol::FileSearchResult {
        nexus_common::protocol::FileSearchResult {
            path: path.to_string(),
            name: name.to_string(),
            size,
            modified: 0,
            is_directory,
        }
    }

    #[test]
    fn test_sort_search_results_by_name_directories_first() {
        let mut results = vec![
            make_search_result("zebra.txt", "/zebra.txt", 100, false),
            make_search_result("alpha", "/alpha", 0, true),
            make_search_result("apple.txt", "/apple.txt", 200, false),
            make_search_result("beta", "/beta", 0, true),
        ];

        sort_search_results(&mut results, FileSortColumn::Name, true);

        // Directories should come first, then files, both alphabetically
        assert_eq!(results[0].name, "alpha");
        assert!(results[0].is_directory);
        assert_eq!(results[1].name, "beta");
        assert!(results[1].is_directory);
        assert_eq!(results[2].name, "apple.txt");
        assert!(!results[2].is_directory);
        assert_eq!(results[3].name, "zebra.txt");
        assert!(!results[3].is_directory);
    }

    #[test]
    fn test_sort_search_results_by_name_descending() {
        let mut results = vec![
            make_search_result("apple.txt", "/apple.txt", 100, false),
            make_search_result("zebra.txt", "/zebra.txt", 200, false),
            make_search_result("middle.txt", "/middle.txt", 150, false),
        ];

        sort_search_results(&mut results, FileSortColumn::Name, false);

        assert_eq!(results[0].name, "zebra.txt");
        assert_eq!(results[1].name, "middle.txt");
        assert_eq!(results[2].name, "apple.txt");
    }

    #[test]
    fn test_sort_search_results_by_path() {
        let mut results = vec![
            make_search_result("file.txt", "/z/file.txt", 100, false),
            make_search_result("file.txt", "/a/file.txt", 100, false),
            make_search_result("file.txt", "/m/file.txt", 100, false),
        ];

        sort_search_results(&mut results, FileSortColumn::Path, true);

        assert_eq!(results[0].path, "/a/file.txt");
        assert_eq!(results[1].path, "/m/file.txt");
        assert_eq!(results[2].path, "/z/file.txt");
    }

    #[test]
    fn test_sort_search_results_by_path_subsorts_by_name() {
        let mut results = vec![
            make_search_result("zebra.txt", "/docs/zebra.txt", 100, false),
            make_search_result("apple.txt", "/docs/apple.txt", 100, false),
            make_search_result("banana.txt", "/docs/banana.txt", 100, false),
        ];

        sort_search_results(&mut results, FileSortColumn::Path, true);

        // Same path prefix, should be sub-sorted by name
        assert_eq!(results[0].name, "apple.txt");
        assert_eq!(results[1].name, "banana.txt");
        assert_eq!(results[2].name, "zebra.txt");
    }

    #[test]
    fn test_sort_search_results_by_size() {
        let mut results = vec![
            make_search_result("medium.txt", "/medium.txt", 500, false),
            make_search_result("small.txt", "/small.txt", 100, false),
            make_search_result("large.txt", "/large.txt", 1000, false),
        ];

        sort_search_results(&mut results, FileSortColumn::Size, true);

        assert_eq!(results[0].size, 100);
        assert_eq!(results[1].size, 500);
        assert_eq!(results[2].size, 1000);

        // Descending
        sort_search_results(&mut results, FileSortColumn::Size, false);

        assert_eq!(results[0].size, 1000);
        assert_eq!(results[1].size, 500);
        assert_eq!(results[2].size, 100);
    }

    #[test]
    fn test_sort_search_results_by_size_subsorts_by_name() {
        let mut results = vec![
            make_search_result("zebra.txt", "/zebra.txt", 100, false),
            make_search_result("apple.txt", "/apple.txt", 100, false),
            make_search_result("banana.txt", "/banana.txt", 100, false),
        ];

        sort_search_results(&mut results, FileSortColumn::Size, true);

        // Same size, should be sub-sorted by name
        assert_eq!(results[0].name, "apple.txt");
        assert_eq!(results[1].name, "banana.txt");
        assert_eq!(results[2].name, "zebra.txt");
    }

    #[test]
    fn test_sort_search_results_case_insensitive() {
        let mut results = vec![
            make_search_result("Zebra.txt", "/Zebra.txt", 100, false),
            make_search_result("apple.txt", "/apple.txt", 100, false),
            make_search_result("BANANA.txt", "/BANANA.txt", 100, false),
        ];

        sort_search_results(&mut results, FileSortColumn::Name, true);

        assert_eq!(results[0].name, "apple.txt");
        assert_eq!(results[1].name, "BANANA.txt");
        assert_eq!(results[2].name, "Zebra.txt");
    }

    #[test]
    fn test_sort_search_results_empty() {
        let mut results: Vec<nexus_common::protocol::FileSearchResult> = vec![];
        // Should not panic on empty vec
        sort_search_results(&mut results, FileSortColumn::Name, true);
        assert!(results.is_empty());
    }

    #[test]
    fn test_sort_search_results_single_item() {
        let mut results = vec![make_search_result("test.txt", "/test.txt", 100, false)];
        sort_search_results(&mut results, FileSortColumn::Name, true);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "test.txt");
    }

    #[test]
    fn test_sort_search_results_path_case_insensitive() {
        let mut results = vec![
            make_search_result("file.txt", "/Zebra/file.txt", 100, false),
            make_search_result("file.txt", "/apple/file.txt", 100, false),
            make_search_result("file.txt", "/BANANA/file.txt", 100, false),
        ];

        sort_search_results(&mut results, FileSortColumn::Path, true);

        assert_eq!(results[0].path, "/apple/file.txt");
        assert_eq!(results[1].path, "/BANANA/file.txt");
        assert_eq!(results[2].path, "/Zebra/file.txt");
    }

    #[test]
    fn test_sort_search_results_modified() {
        let mut results = vec![
            make_search_result("old.txt", "/old.txt", 100, false),
            make_search_result("new.txt", "/new.txt", 100, false),
            make_search_result("mid.txt", "/mid.txt", 100, false),
        ];
        // Manually set modified times
        results[0].modified = 1000;
        results[1].modified = 3000;
        results[2].modified = 2000;

        sort_search_results(&mut results, FileSortColumn::Modified, true);

        assert_eq!(results[0].modified, 1000);
        assert_eq!(results[1].modified, 2000);
        assert_eq!(results[2].modified, 3000);

        // Descending
        sort_search_results(&mut results, FileSortColumn::Modified, false);

        assert_eq!(results[0].modified, 3000);
        assert_eq!(results[1].modified, 2000);
        assert_eq!(results[2].modified, 1000);
    }

    #[test]
    fn test_sort_search_results_modified_subsorts_by_name() {
        let mut results = vec![
            make_search_result("zebra.txt", "/zebra.txt", 100, false),
            make_search_result("apple.txt", "/apple.txt", 100, false),
            make_search_result("banana.txt", "/banana.txt", 100, false),
        ];
        // Same modified time for all
        results[0].modified = 1000;
        results[1].modified = 1000;
        results[2].modified = 1000;

        sort_search_results(&mut results, FileSortColumn::Modified, true);

        // Same modified time, should be sub-sorted by name
        assert_eq!(results[0].name, "apple.txt");
        assert_eq!(results[1].name, "banana.txt");
        assert_eq!(results[2].name, "zebra.txt");
    }

    #[test]
    fn test_sort_search_results_directories_always_first_for_name() {
        let mut results = vec![
            make_search_result("zebra", "/zebra", 0, true),
            make_search_result("apple.txt", "/apple.txt", 100, false),
            make_search_result("alpha", "/alpha", 0, true),
            make_search_result("banana.txt", "/banana.txt", 200, false),
        ];

        // Ascending - dirs first, then files, both alphabetical
        sort_search_results(&mut results, FileSortColumn::Name, true);

        assert!(results[0].is_directory);
        assert_eq!(results[0].name, "alpha");
        assert!(results[1].is_directory);
        assert_eq!(results[1].name, "zebra");
        assert!(!results[2].is_directory);
        assert_eq!(results[2].name, "apple.txt");
        assert!(!results[3].is_directory);
        assert_eq!(results[3].name, "banana.txt");

        // Descending - dirs still first, but both groups reversed
        sort_search_results(&mut results, FileSortColumn::Name, false);

        assert!(results[0].is_directory);
        assert_eq!(results[0].name, "zebra");
        assert!(results[1].is_directory);
        assert_eq!(results[1].name, "alpha");
        assert!(!results[2].is_directory);
        assert_eq!(results[2].name, "banana.txt");
        assert!(!results[3].is_directory);
        assert_eq!(results[3].name, "apple.txt");
    }
}
