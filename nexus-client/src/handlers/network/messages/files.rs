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
                        // Continue in the originating tab, even if it is now in the background.
                        let new_path = if tab.current_path.is_empty() {
                            entry.name.clone()
                        } else {
                            format!("{}/{}", tab.current_path, entry.name)
                        };
                        let root = tab.viewing_root;
                        let show_hidden = self.config.settings.show_hidden_files;
                        tab.navigate_to(new_path.clone());
                        return self.send_file_list_request_for_tab(
                            connection_id,
                            tab_id,
                            new_path,
                            root,
                            show_hidden,
                            None,
                        );
                    } else {
                        // Download through the connection that supplied this listing.
                        let file_path = if tab.current_path.is_empty() {
                            entry.name.clone()
                        } else {
                            format!("{}/{}", tab.current_path, entry.name)
                        };
                        let remote_root = tab.viewing_root;
                        return self.queue_download_with_root(
                            connection_id,
                            file_path,
                            false,
                            remote_root,
                        );
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

#[cfg(test)]
mod tests {
    use std::fs;

    use nexus_common::protocol::{ClientMessage, FileEntryDirType};
    use tempfile::TempDir;
    use tokio::sync::mpsc::UnboundedReceiver;
    use uuid::Uuid;

    use super::*;
    use crate::handlers::files::FilesOpenIntent;
    use crate::testing::support::test_connection_with_receiver;
    use crate::transfers::{TransferDirection, TransferManager, TransferStatus};
    use crate::types::{ActivePanel, TabId};

    struct PendingUri {
        app: NexusApp,
        origin_tab: TabId,
        other_tab: TabId,
        request_id: MessageId,
        origin_rx: UnboundedReceiver<(MessageId, ClientMessage)>,
        other_rx: UnboundedReceiver<(MessageId, ClientMessage)>,
        temp: TempDir,
    }

    fn pending_uri(target: &str) -> PendingUri {
        let temp = TempDir::new().unwrap();
        let (mut conn, mut origin_rx) = test_connection_with_receiver(1);
        conn.bookmark_id = Some(Uuid::new_v4());
        conn.connection_info.server_name = "Origin server".into();
        conn.connection_info.address = "origin.example.test".into();
        conn.connection_info.port = 7500;
        conn.connection_info.transfer_port = 7501;
        conn.connection_info.certificate_fingerprint = "origin-fingerprint".into();
        conn.connection_info.username = "origin-user".into();
        conn.connection_info.password = "origin-password".into();
        conn.connection_info.nickname = "Origin user".into();
        let tab = conn.files_management.active_tab_mut();
        tab.navigate_to("Other tab".into());
        tab.viewing_root = true;
        tab.error = Some("Other tab error".into());
        let other_tab = tab.id;
        conn.files_management.new_tab();
        let origin_tab = conn.files_management.active_tab_id();

        let (mut other_conn, other_rx) = test_connection_with_receiver(2);
        other_conn.active_panel = ActivePanel::Files;
        other_conn.connection_info.address = "other.example.test".into();
        let tab = other_conn.files_management.active_tab_mut();
        tab.navigate_to("Other connection".into());
        tab.viewing_root = true;
        tab.error = Some("Other connection error".into());

        let mut app = NexusApp {
            active_connection: Some(1),
            transfer_manager: TransferManager::new_for_test(temp.path().join("transfers.json")),
            ..NexusApp::default()
        };
        app.config.settings.download_path =
            Some(temp.path().join("downloads").to_str().unwrap().into());
        app.config.settings.queue_transfers = true;
        app.config.settings.show_hidden_files = true;
        app.connections.insert(1, conn);
        app.connections.insert(2, other_conn);

        drop(app.handle_toggle_files(FilesOpenIntent::UriPath(format!("Music/{target}"))));
        let (request_id, request) = origin_rx.try_recv().expect("URI parent listing request");
        assert!(matches!(request, ClientMessage::FileList {
            path, root: false, show_hidden: true
        } if path == "Music"));
        assert!(matches!(
            app.connections[&1].pending_requests.get(&request_id),
            Some(ResponseRouting::PopulateFileList { tab_id, uri_target: Some(name) })
                if *tab_id == origin_tab && name == target
        ));
        assert!(origin_rx.try_recv().is_err());

        PendingUri {
            app,
            origin_tab,
            other_tab,
            request_id,
            origin_rx,
            other_rx,
            temp,
        }
    }

    fn listing(path: &str, name: &str, is_directory: bool) -> FileListResponseData {
        FileListResponseData {
            success: true,
            error: None,
            path: Some(path.into()),
            entries: Some(vec![FileEntry {
                name: name.into(),
                size: 0,
                modified: 0,
                dir_type: is_directory.then_some(FileEntryDirType::Upload),
                can_upload: false,
            }]),
            can_upload: false,
            dropbox_owner: None,
        }
    }

    #[test]
    fn delayed_uri_directory_listing_stays_with_originating_tab() {
        for switch_connection in [false, true] {
            let mut fixture = pending_uri("album");
            let app = &mut fixture.app;
            drop(app.handle_file_tab_switch(fixture.other_tab));
            if switch_connection {
                drop(app.handle_switch_to_connection(2));
            }
            let active_connection = app.active_connection;
            let other_tab_before =
                format!("{:?}", app.connections[&1].files_management.active_tab());
            let other_connection_before = format!("{:?}", app.connections[&2].files_management);

            drop(app.handle_file_list_response(
                1,
                fixture.request_id,
                listing("Music [NEXUS-UL]", "Album [NEXUS-UL]", true),
            ));

            let (followup_id, request) = fixture.origin_rx.try_recv().expect("directory listing");
            let directory = "Music [NEXUS-UL]/Album [NEXUS-UL]";
            assert!(matches!(request, ClientMessage::FileList {
                path, root: false, show_hidden: true
            } if path == directory));
            assert!(matches!(
                app.connections[&1].pending_requests.get(&followup_id),
                Some(ResponseRouting::PopulateFileList { tab_id, uri_target: None })
                    if *tab_id == fixture.origin_tab
            ));

            let contents = listing(directory, "track.mp3", false);
            let expected_entries = contents.entries.clone();
            drop(app.handle_file_list_response(1, followup_id, contents));

            let conn = &app.connections[&1];
            let origin = conn.files_management.tab_by_id(fixture.origin_tab).unwrap();
            assert_eq!(origin.current_path, directory);
            assert_eq!(origin.entries, expected_entries);
            assert!(!conn.pending_requests.contains_key(&fixture.request_id));
            assert!(!conn.pending_requests.contains_key(&followup_id));
            assert_eq!(conn.files_management.active_tab_id(), fixture.other_tab);
            assert_eq!(
                format!("{:?}", conn.files_management.active_tab()),
                other_tab_before
            );
            assert_eq!(
                format!("{:?}", app.connections[&2].files_management),
                other_connection_before
            );
            assert_eq!(app.active_connection, active_connection);
            assert_eq!(app.active_panel(), ActivePanel::Files);
            assert_eq!(app.transfer_manager.all().count(), 0);
            assert!(!fixture.temp.path().join("transfers.json").exists());
            assert!(fixture.origin_rx.try_recv().is_err());
            assert!(fixture.other_rx.try_recv().is_err());
        }
    }

    #[test]
    fn delayed_uri_download_uses_originating_connection() {
        let mut fixture = pending_uri("TRACK.MP3");
        let app = &mut fixture.app;
        let expected_connection =
            serde_json::to_value(&app.connections[&1].connection_info).unwrap();
        let expected_bookmark = app.connections[&1].bookmark_id;
        drop(app.handle_file_tab_switch(fixture.other_tab));
        drop(app.handle_switch_to_connection(2));
        let other_tab_before = format!("{:?}", app.connections[&1].files_management.active_tab());
        let other_connection_before = format!("{:?}", app.connections[&2].files_management);

        drop(app.handle_file_list_response(
            1,
            fixture.request_id,
            listing("Music [NEXUS-UL]", "track.mp3", false),
        ));
        // Duplicate delivery cannot queue a second download after the routing is consumed.
        drop(app.handle_file_list_response(
            1,
            fixture.request_id,
            listing("Music [NEXUS-UL]", "track.mp3", false),
        ));

        assert_eq!(app.transfer_manager.all().count(), 1);
        let transfer = app.transfer_manager.all().next().unwrap();
        assert_eq!(
            serde_json::to_value(&transfer.connection_info).unwrap(),
            expected_connection
        );
        assert_eq!(transfer.bookmark_id, expected_bookmark);
        assert_eq!(transfer.remote_path, "Music [NEXUS-UL]/track.mp3");
        assert!(!transfer.remote_root);
        assert!(!transfer.is_directory);
        assert_eq!(transfer.direction, TransferDirection::Download);
        assert_eq!(transfer.status, TransferStatus::Queued);
        assert_eq!(
            transfer.local_path,
            fixture.temp.path().join("downloads/track.mp3")
        );
        let saved: serde_json::Value =
            serde_json::from_slice(&fs::read(fixture.temp.path().join("transfers.json")).unwrap())
                .unwrap();
        assert_eq!(saved["transfers"], serde_json::json!([transfer]));
        assert!(!fixture.temp.path().join("downloads").exists());

        let conn = &app.connections[&1];
        assert!(!conn.pending_requests.contains_key(&fixture.request_id));
        assert_eq!(conn.files_management.active_tab_id(), fixture.other_tab);
        assert_eq!(
            format!("{:?}", conn.files_management.active_tab()),
            other_tab_before
        );
        assert_eq!(
            format!("{:?}", app.connections[&2].files_management),
            other_connection_before
        );
        assert_eq!(app.active_connection, Some(2));
        assert_eq!(app.active_panel(), ActivePanel::Files);
        assert!(fixture.origin_rx.try_recv().is_err());
        assert!(fixture.other_rx.try_recv().is_err());
    }

    #[test]
    fn delayed_uri_response_is_ignored_after_origin_is_removed() {
        for is_directory in [false, true] {
            for remove_connection in [false, true] {
                let mut fixture = pending_uri("target");
                let app = &mut fixture.app;
                if remove_connection {
                    app.connections.remove(&1);
                } else {
                    drop(app.handle_file_tab_close(fixture.origin_tab));
                    let conn = &app.connections[&1];
                    assert!(
                        conn.files_management
                            .tab_by_id(fixture.origin_tab)
                            .is_none()
                    );
                    assert!(!conn.pending_requests.contains_key(&fixture.request_id));
                }
                drop(app.handle_switch_to_connection(2));
                let other_tab_before = app
                    .connections
                    .get(&1)
                    .map(|conn| format!("{:?}", conn.files_management));
                let other_connection_before = format!("{:?}", app.connections[&2].files_management);

                drop(app.handle_file_list_response(
                    1,
                    fixture.request_id,
                    listing("Music", "target", is_directory),
                ));

                assert_eq!(
                    app.connections
                        .get(&1)
                        .map(|conn| format!("{:?}", conn.files_management)),
                    other_tab_before
                );
                assert_eq!(
                    format!("{:?}", app.connections[&2].files_management),
                    other_connection_before
                );
                assert_eq!(app.active_connection, Some(2));
                assert_eq!(app.transfer_manager.all().count(), 0);
                assert!(!fixture.temp.path().join("transfers.json").exists());
                assert!(fixture.origin_rx.try_recv().is_err());
                assert!(fixture.other_rx.try_recv().is_err());
            }
        }
    }

    #[test]
    fn delayed_uri_directory_followup_is_ignored_after_tab_closes() {
        let mut fixture = pending_uri("album");
        let app = &mut fixture.app;
        drop(app.handle_file_list_response(1, fixture.request_id, listing("Music", "album", true)));
        let (followup_id, _) = fixture.origin_rx.try_recv().expect("directory listing");
        drop(app.handle_file_tab_close(fixture.origin_tab));
        assert!(
            app.connections[&1]
                .files_management
                .tab_by_id(fixture.origin_tab)
                .is_none()
        );
        assert!(
            !app.connections[&1]
                .pending_requests
                .contains_key(&followup_id)
        );
        drop(app.handle_switch_to_connection(2));
        let other_tab_before = format!("{:?}", app.connections[&1].files_management);
        let other_connection_before = format!("{:?}", app.connections[&2].files_management);

        drop(app.handle_file_list_response(
            1,
            followup_id,
            listing("Music/album", "track.mp3", false),
        ));

        assert_eq!(
            format!("{:?}", app.connections[&1].files_management),
            other_tab_before
        );
        assert_eq!(
            format!("{:?}", app.connections[&2].files_management),
            other_connection_before
        );
        assert_eq!(app.active_connection, Some(2));
        assert_eq!(app.transfer_manager.all().count(), 0);
        assert!(fixture.origin_rx.try_recv().is_err());
        assert!(fixture.other_rx.try_recv().is_err());
    }
}
