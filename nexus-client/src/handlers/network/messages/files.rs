//! File response handlers

use crate::i18n::{t, t_args};
use crate::types::ChatMessage;

use iced::Task;
use nexus_common::ErrorKind;
use nexus_common::framing::MessageId;
use nexus_common::protocol::{FileEntry, FileInfoDetails, FileSearchResult};

use crate::NexusApp;
use crate::handlers::files::sort_search_results;
use crate::types::{FilesManagementState, InputId, Message, PendingOverwrite, ResponseRouting};

/// Data from a FileListResponse message
pub struct FileListResponseData {
    pub success: bool,
    pub error: Option<String>,
    pub path: Option<String>,
    pub entries: Option<Vec<FileEntry>>,
    pub can_upload: bool,
    pub dropbox_owner: Option<String>,
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
        let (tab_id, uri_target) = match routing {
            Some(ResponseRouting::PopulateFileList { tab_id, uri_target }) => (tab_id, uri_target),
            _ => return Task::none(),
        };

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
            tab.dropbox_owner = data.dropbox_owner;

            tab.entries = data.entries;
            tab.error = None;

            // Build sorted entries cache
            tab.update_sorted_entries();

            if let Some(target) = &tab.scroll_target
                && !tab
                    .entries
                    .as_ref()
                    .is_some_and(|entries| entries.iter().any(|entry| entry.name == target.name))
            {
                tab.error = Some(t_args("files-not-found", &[("name", &target.name)]));
                tab.scroll_target = None;
            }

            // Check for URI target (from nexus:// URI navigation)
            // Match against both the full name (with suffix) and display name (without suffix)
            // to support both "uploads [NEXUS-UL]" and "uploads" style URIs
            if let Some(ref target) = uri_target
                && let Some(ref entries) = tab.entries
            {
                // Find the target in the file list (case-insensitive)
                // Match either full name or display name (with suffix stripped)
                let target_lower = target.to_lowercase();
                if let Some(entry) = entries.iter().find(|e| {
                    let name_lower = e.name.to_lowercase();
                    let display_lower = FilesManagementState::display_name(&e.name).to_lowercase();
                    name_lower == target_lower || display_lower == target_lower
                }) {
                    if entry.dir_type.is_some() {
                        // It's a directory - navigate into it
                        let new_path = if tab.current_path.is_empty() {
                            entry.name.clone()
                        } else {
                            format!("{}/{}", tab.current_path, entry.name)
                        };
                        let root = tab.viewing_root;
                        let show_hidden = self.config.settings.show_hidden_files;
                        tab.navigate_to(new_path.clone());
                        return self.send_file_list_request(
                            connection_id,
                            new_path,
                            root,
                            show_hidden,
                        );
                    } else {
                        // It's a file - initiate download
                        let file_path = if tab.current_path.is_empty() {
                            entry.name.clone()
                        } else {
                            format!("{}/{}", tab.current_path, entry.name)
                        };
                        let remote_root = tab.viewing_root;
                        return self.queue_download_with_root(file_path, false, remote_root);
                    }
                }
                // Target not found - show error message above the listing
                tab.error = Some(t_args("files-not-found", &[("name", target)]));
            }
        } else {
            tab.entries = None;
            tab.sorted_entries = None;
            tab.scroll_target = None;
            tab.error = data.error;
        }

        // Restore/reset only this listing, including when it arrives in the
        // background. The view reveals any pending target after layout.
        if !tab.is_searching() {
            tab.reset_scroll();
        }
        Task::none()
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
                None,
            )
        } else {
            // Show error in dialog (re-lookup tab). Split the tab
            // mutation from the focus dispatch so the `conn` borrow
            // is released before `self.focus_field` runs.
            let dialog_open = if let Some(tab) = conn.files_management.tab_by_id_mut(tab_id) {
                tab.is_create_dir_submitting = false;
                tab.new_directory_error = None;
                tab.new_directory_submission_error = error;
                true
            } else {
                false
            };
            if dialog_open {
                // Focus the input field so user can retry
                self.focus_field(InputId::NewDirectoryName)
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
            tab.is_delete_submitting = false;

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
                None,
            )
        } else {
            // Show error in the delete dialog (re-lookup tab)
            if let Some(tab) = conn.files_management.tab_by_id_mut(tab_id) {
                tab.is_delete_submitting = false;
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
            tab.rename_submission_error = None;
            tab.is_rename_submitting = false;

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
                None,
            )
        } else {
            // Show error in the rename dialog (re-lookup tab). Split
            // the tab mutation from the focus dispatch so the `conn`
            // borrow is released before `self.focus_field` runs.
            let dialog_open = if let Some(tab) = conn.files_management.tab_by_id_mut(tab_id) {
                tab.is_rename_submitting = false;
                tab.rename_error = None;
                tab.rename_submission_error = error;
                true
            } else {
                false
            };
            if dialog_open {
                // Focus the input field so user can retry
                self.focus_field(InputId::RenameName)
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
            tab.is_paste_submitting = false;

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
                None,
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
                        tab.is_paste_submitting = false;
                        tab.error = None;
                    }
                    Task::none()
                }
                Some(ErrorKind::NotFound) => {
                    // Source no longer exists - clear clipboard
                    conn.files_management.clipboard = None;
                    if let Some(tab) = conn.files_management.tab_by_id_mut(tab_id) {
                        tab.is_paste_submitting = false;
                        tab.error = error;
                    }
                    Task::none()
                }
                _ => {
                    // Show error in panel (permission, invalid_path, or unknown)
                    if let Some(tab) = conn.files_management.tab_by_id_mut(tab_id) {
                        tab.is_paste_submitting = false;
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
            tab.is_paste_submitting = false;

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
                None,
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
                        tab.is_paste_submitting = false;
                        tab.error = None;
                    }
                    Task::none()
                }
                Some(ErrorKind::NotFound) => {
                    // Source no longer exists - clear clipboard
                    conn.files_management.clipboard = None;
                    if let Some(tab) = conn.files_management.tab_by_id_mut(tab_id) {
                        tab.is_paste_submitting = false;
                        tab.error = error;
                    }
                    Task::none()
                }
                _ => {
                    // Show error in panel (permission, invalid_path, or unknown)
                    if let Some(tab) = conn.files_management.tab_by_id_mut(tab_id) {
                        tab.is_paste_submitting = false;
                        tab.error = error;
                    }
                    Task::none()
                }
            }
        }
    }

    /// Handle FileReindexResponse from server
    pub fn handle_file_reindex_response(
        &mut self,
        connection_id: usize,
        success: bool,
        error: Option<String>,
    ) -> Task<Message> {
        if success {
            let msg = t("msg-reindex-triggered");
            self.add_active_tab_message(connection_id, ChatMessage::info(msg))
        } else {
            let error_msg = error.unwrap_or_else(|| t("err-unknown"));
            self.add_active_tab_message(connection_id, ChatMessage::error(error_msg))
        }
    }

    /// Handle FileSearchResponse from server
    pub fn handle_file_search_response(
        &mut self,
        connection_id: usize,
        message_id: MessageId,
        success: bool,
        error: Option<String>,
        results: Option<Vec<FileSearchResult>>,
    ) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        // Check if this was a tracked request
        let routing = conn.pending_requests.remove(&message_id);

        // Only handle if this was a tracked file search request
        let tab_id = match routing {
            Some(ResponseRouting::FileSearchResult { tab_id }) => tab_id,
            _ => return Task::none(),
        };

        // Find the tab by ID (it may have been closed)
        let Some(tab) = conn.files_management.tab_by_id_mut(tab_id) else {
            return Task::none();
        };

        // Check if this response is for the current search request
        // If user submitted a new search before this response arrived, ignore this stale response
        if tab.current_search_request != Some(message_id) {
            return Task::none();
        }

        // Clear the current request tracker now that we're processing it
        tab.current_search_request = None;
        tab.search_loading = false;

        if success {
            // Sort results based on current sort settings
            // Treat None as empty results (defensive against malformed server response)
            let mut sorted_results = results.unwrap_or_default();
            sort_search_results(
                &mut sorted_results,
                tab.search_sort_column,
                tab.search_sort_ascending,
            );
            tab.search_results = Some(sorted_results);
            tab.search_error = None;
        } else {
            tab.search_results = None;
            tab.search_error = error;
        }

        Task::none()
    }
}
