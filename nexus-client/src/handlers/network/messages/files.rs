//! File response handlers

use iced::widget::{Id, scrollable};
use iced::{Task, widget::operation};
use nexus_common::ErrorKind;
use nexus_common::framing::MessageId;
use nexus_common::protocol::{FileEntry, FileInfoDetails};

use crate::NexusApp;
use crate::types::{InputId, Message, PendingOverwrite, ResponseRouting, ScrollableId};

/// Data from a FileListResponse message
pub struct FileListResponseData {
    pub success: bool,
    pub error: Option<String>,
    pub path: Option<String>,
    pub entries: Option<Vec<FileEntry>>,
    pub can_upload: bool,
}

impl NexusApp {
    /// Handle file list response
    ///
    /// Populates the file entries in the files management panel.
    pub fn handle_file_list_response(
        &mut self,
        connection_id: usize,
        message_id: MessageId,
        data: FileListResponseData,
    ) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        // Check if this was a tracked request
        let routing = conn.pending_requests.remove(&message_id);

        // Only handle if this was a tracked file list request
        let tab_id = match routing {
            Some(ResponseRouting::PopulateFileList { tab_id }) => tab_id,
            _ => return Task::none(),
        };

        // Check if this response is for the currently active tab (for scroll behavior)
        let is_active_tab = conn.files_management.active_tab_id() == tab_id;

        // Find the tab by ID (it may have been closed)
        let Some(tab) = conn.files_management.tab_by_id_mut(tab_id) else {
            return Task::none();
        };

        if data.success {
            // Update the current path if provided
            if let Some(path) = data.path {
                tab.current_path = path;
            }

            // Use server-provided can_upload flag for the current directory
            tab.current_dir_can_upload = data.can_upload;

            tab.entries = data.entries;
            tab.error = None;

            // Build sorted entries cache
            tab.update_sorted_entries();
        } else {
            // Re-lookup tab for the else branch (borrow checker)
            if let Some(tab) = conn.files_management.tab_by_id_mut(tab_id) {
                tab.entries = None;
                tab.sorted_entries = None;
                tab.error = data.error;
            }
        }

        // Snap scroll to beginning when directory content changes (only for active tab)
        if is_active_tab {
            operation::snap_to(
                ScrollableId::FilesContent,
                scrollable::RelativeOffset::START,
            )
        } else {
            Task::none()
        }
    }

    /// Handle file create directory response
    ///
    /// On success, closes the dialog and refreshes the file list.
    /// On error, displays the error in the dialog.
    pub fn handle_file_create_dir_response(
        &mut self,
        connection_id: usize,
        message_id: MessageId,
        success: bool,
        error: Option<String>,
        _path: Option<String>,
    ) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        // Check if this was a tracked request
        let routing = conn.pending_requests.remove(&message_id);

        // Only handle if this was a tracked file create dir request
        let tab_id = match routing {
            Some(ResponseRouting::FileCreateDirResult { tab_id }) => tab_id,
            _ => return Task::none(),
        };

        // Find the tab by ID (it may have been closed)
        let Some(tab) = conn.files_management.tab_by_id_mut(tab_id) else {
            return Task::none();
        };

        if success {
            // Close the dialog
            tab.close_new_directory_dialog();

            // Refresh the current directory listing
            let current_path = tab.current_path.clone();
            let viewing_root = tab.viewing_root;

            // Clear entries to show loading state
            tab.entries = None;
            tab.error = None;

            let show_hidden = self.config.settings.show_hidden_files;

            // Send refresh request for the specific tab
            self.send_file_list_request_for_tab(
                connection_id,
                tab_id,
                current_path,
                viewing_root,
                show_hidden,
            )
        } else {
            // Show error in dialog (re-lookup tab)
            if let Some(tab) = conn.files_management.tab_by_id_mut(tab_id) {
                tab.new_directory_error = error;
                // Focus the input field so user can retry
                operation::focus(Id::from(InputId::NewDirectoryName))
            } else {
                Task::none()
            }
        }
    }

    /// Handle file delete response
    ///
    /// On success, refreshes the file list.
    /// On error, displays the error in the delete dialog so user can retry or cancel.
    pub fn handle_file_delete_response(
        &mut self,
        connection_id: usize,
        message_id: MessageId,
        success: bool,
        error: Option<String>,
    ) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        // Check if this was a tracked request
        let routing = conn.pending_requests.remove(&message_id);

        // Only handle if this was a tracked file delete request
        let tab_id = match routing {
            Some(ResponseRouting::FileDeleteResult { tab_id }) => tab_id,
            _ => return Task::none(),
        };

        // Find the tab by ID (it may have been closed)
        let Some(tab) = conn.files_management.tab_by_id_mut(tab_id) else {
            return Task::none();
        };

        if success {
            // Close the delete dialog
            tab.pending_delete = None;
            tab.delete_error = None;

            // Refresh the current directory listing
            let current_path = tab.current_path.clone();
            let viewing_root = tab.viewing_root;

            // Clear entries to show loading state
            tab.entries = None;
            tab.error = None;

            let show_hidden = self.config.settings.show_hidden_files;

            // Send refresh request for the specific tab
            self.send_file_list_request_for_tab(
                connection_id,
                tab_id,
                current_path,
                viewing_root,
                show_hidden,
            )
        } else {
            // Show error in the delete dialog (re-lookup tab)
            if let Some(tab) = conn.files_management.tab_by_id_mut(tab_id) {
                tab.delete_error = error;
            }
            Task::none()
        }
    }

    /// Handle file info response
    ///
    /// On success, displays the file info dialog.
    /// On error, displays the error in the files panel.
    pub fn handle_file_info_response(
        &mut self,
        connection_id: usize,
        message_id: MessageId,
        success: bool,
        error: Option<String>,
        info: Option<FileInfoDetails>,
    ) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        // Check if this was a tracked request
        let routing = conn.pending_requests.remove(&message_id);

        // Only handle if this was a tracked file info request
        let tab_id = match routing {
            Some(ResponseRouting::FileInfoResult { tab_id }) => tab_id,
            _ => return Task::none(),
        };

        // Find the tab by ID (it may have been closed)
        let Some(tab) = conn.files_management.tab_by_id_mut(tab_id) else {
            return Task::none();
        };

        if success {
            // Show the info dialog
            tab.pending_info = info;
        } else {
            // Show error in the files panel
            tab.error = error;
        }

        Task::none()
    }

    /// Handle file rename response
    ///
    /// On success, closes the dialog and refreshes the file list.
    /// On error, displays the error in the rename dialog so user can retry.
    pub fn handle_file_rename_response(
        &mut self,
        connection_id: usize,
        message_id: MessageId,
        success: bool,
        error: Option<String>,
    ) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        // Check if this was a tracked request
        let routing = conn.pending_requests.remove(&message_id);

        // Only handle if this was a tracked file rename request
        let tab_id = match routing {
            Some(ResponseRouting::FileRenameResult { tab_id }) => tab_id,
            _ => return Task::none(),
        };

        // Find the tab by ID (it may have been closed)
        let Some(tab) = conn.files_management.tab_by_id_mut(tab_id) else {
            return Task::none();
        };

        if success {
            // Close the rename dialog
            tab.pending_rename = None;
            tab.rename_name = String::new();
            tab.rename_error = None;

            // Refresh the current directory listing
            let current_path = tab.current_path.clone();
            let viewing_root = tab.viewing_root;

            // Clear entries to show loading state
            tab.entries = None;
            tab.error = None;

            let show_hidden = self.config.settings.show_hidden_files;

            // Send refresh request for the specific tab
            self.send_file_list_request_for_tab(
                connection_id,
                tab_id,
                current_path,
                viewing_root,
                show_hidden,
            )
        } else {
            // Show error in the rename dialog (re-lookup tab)
            if let Some(tab) = conn.files_management.tab_by_id_mut(tab_id) {
                tab.rename_error = error;
                // Focus the input field so user can retry
                operation::focus(Id::from(InputId::RenameName))
            } else {
                Task::none()
            }
        }
    }

    /// Handle file move response
    ///
    /// On success, clears clipboard (if cut) and refreshes file list.
    /// On "exists" error, shows overwrite confirmation dialog.
    /// On other errors, displays error in panel.
    pub fn handle_file_move_response(
        &mut self,
        connection_id: usize,
        message_id: MessageId,
        success: bool,
        error: Option<String>,
        error_kind: Option<String>,
    ) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        // Check if this was a tracked request and extract destination_dir
        let routing = conn.pending_requests.remove(&message_id);

        // Only handle if this was a tracked file move request
        let (tab_id, destination_dir) = match routing {
            Some(ResponseRouting::FileMoveResult {
                tab_id,
                destination_dir,
            }) => (tab_id, destination_dir),
            _ => return Task::none(),
        };

        if success {
            // Clear clipboard on successful move
            conn.files_management.clipboard = None;

            // Get mutable access to the tab (it may have been closed)
            let Some(tab) = conn.files_management.tab_by_id_mut(tab_id) else {
                return Task::none();
            };
            tab.pending_overwrite = None;

            // Refresh the current directory listing
            let current_path = tab.current_path.clone();
            let viewing_root = tab.viewing_root;

            tab.entries = None;
            tab.error = None;

            let show_hidden = self.config.settings.show_hidden_files;

            // Send refresh request for the specific tab
            self.send_file_list_request_for_tab(
                connection_id,
                tab_id,
                current_path,
                viewing_root,
                show_hidden,
            )
        } else {
            // Parse error_kind for type-safe matching
            let kind = error_kind.as_deref().and_then(ErrorKind::parse);

            match kind {
                Some(ErrorKind::Exists) => {
                    // Clone clipboard data first to avoid borrow conflicts
                    let pending = conn.files_management.clipboard.as_ref().map(|clipboard| {
                        let viewing_root = conn
                            .files_management
                            .tab_by_id(tab_id)
                            .map(|t| t.viewing_root)
                            .unwrap_or(false);
                        PendingOverwrite {
                            source_path: clipboard.path.clone(),
                            destination_dir,
                            name: clipboard.name.clone(),
                            is_move: true,
                            source_root: clipboard.root,
                            destination_root: viewing_root,
                        }
                    });

                    // Now set the pending overwrite
                    if let (Some(pending), Some(tab)) =
                        (pending, conn.files_management.tab_by_id_mut(tab_id))
                    {
                        tab.pending_overwrite = Some(pending);
                    }
                    Task::none()
                }
                Some(ErrorKind::NotFound) => {
                    // Source no longer exists - clear clipboard
                    conn.files_management.clipboard = None;
                    if let Some(tab) = conn.files_management.tab_by_id_mut(tab_id) {
                        tab.error = error;
                    }
                    Task::none()
                }
                _ => {
                    // Show error in panel (permission, invalid_path, or unknown)
                    if let Some(tab) = conn.files_management.tab_by_id_mut(tab_id) {
                        tab.error = error;
                    }
                    Task::none()
                }
            }
        }
    }

    /// Handle file copy response
    ///
    /// On success, refreshes file list (keeps clipboard for potential re-paste).
    /// On "exists" error, shows overwrite confirmation dialog.
    /// On other errors, displays error in panel.
    pub fn handle_file_copy_response(
        &mut self,
        connection_id: usize,
        message_id: MessageId,
        success: bool,
        error: Option<String>,
        error_kind: Option<String>,
    ) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        // Check if this was a tracked request and extract destination_dir
        let routing = conn.pending_requests.remove(&message_id);

        // Only handle if this was a tracked file copy request
        let (tab_id, destination_dir) = match routing {
            Some(ResponseRouting::FileCopyResult {
                tab_id,
                destination_dir,
            }) => (tab_id, destination_dir),
            _ => return Task::none(),
        };

        if success {
            // Get mutable access to the tab (it may have been closed)
            let Some(tab) = conn.files_management.tab_by_id_mut(tab_id) else {
                return Task::none();
            };
            tab.pending_overwrite = None;

            // Refresh the current directory listing
            let current_path = tab.current_path.clone();
            let viewing_root = tab.viewing_root;

            tab.entries = None;
            tab.error = None;

            let show_hidden = self.config.settings.show_hidden_files;

            // Send refresh request for the specific tab
            self.send_file_list_request_for_tab(
                connection_id,
                tab_id,
                current_path,
                viewing_root,
                show_hidden,
            )
        } else {
            // Parse error_kind for type-safe matching
            let kind = error_kind.as_deref().and_then(ErrorKind::parse);

            match kind {
                Some(ErrorKind::Exists) => {
                    // Clone clipboard data first to avoid borrow conflicts
                    let pending = conn.files_management.clipboard.as_ref().map(|clipboard| {
                        let viewing_root = conn
                            .files_management
                            .tab_by_id(tab_id)
                            .map(|t| t.viewing_root)
                            .unwrap_or(false);
                        PendingOverwrite {
                            source_path: clipboard.path.clone(),
                            destination_dir,
                            name: clipboard.name.clone(),
                            is_move: false,
                            source_root: clipboard.root,
                            destination_root: viewing_root,
                        }
                    });

                    // Now set the pending overwrite
                    if let (Some(pending), Some(tab)) =
                        (pending, conn.files_management.tab_by_id_mut(tab_id))
                    {
                        tab.pending_overwrite = Some(pending);
                    }
                    Task::none()
                }
                Some(ErrorKind::NotFound) => {
                    // Source no longer exists - clear clipboard
                    conn.files_management.clipboard = None;
                    if let Some(tab) = conn.files_management.tab_by_id_mut(tab_id) {
                        tab.error = error;
                    }
                    Task::none()
                }
                _ => {
                    // Show error in panel (permission, invalid_path, or unknown)
                    if let Some(tab) = conn.files_management.tab_by_id_mut(tab_id) {
                        tab.error = error;
                    }
                    Task::none()
                }
            }
        }
    }
}
