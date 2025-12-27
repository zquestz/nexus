//! File response handlers

use iced::widget::{Id, scrollable};
use iced::{Task, widget::operation};
use nexus_common::framing::MessageId;
use nexus_common::protocol::{FileEntry, FileInfoDetails};

use crate::NexusApp;
use crate::types::{InputId, Message, ResponseRouting, ScrollableId};

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
        if !matches!(routing, Some(ResponseRouting::PopulateFileList)) {
            return Task::none();
        }

        if data.success {
            // Update the current path if provided
            if let Some(path) = data.path {
                conn.files_management.current_path = path;
            }

            // Use server-provided can_upload flag for the current directory
            conn.files_management.current_dir_can_upload = data.can_upload;

            conn.files_management.entries = data.entries;
            conn.files_management.error = None;
        } else {
            conn.files_management.entries = None;
            conn.files_management.error = data.error;
        }

        // Snap scroll to beginning when directory content changes
        operation::snap_to(
            ScrollableId::FilesContent,
            scrollable::RelativeOffset::START,
        )
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
        if !matches!(routing, Some(ResponseRouting::FileCreateDirResult)) {
            return Task::none();
        }

        if success {
            // Close the dialog
            conn.files_management.close_new_directory_dialog();

            // Refresh the current directory listing
            let current_path = conn.files_management.current_path.clone();
            let viewing_root = conn.files_management.viewing_root;
            let show_hidden = conn.files_management.show_hidden;

            // Clear entries to show loading state
            conn.files_management.entries = None;
            conn.files_management.error = None;

            // Send refresh request
            self.send_file_list_request(connection_id, current_path, viewing_root, show_hidden)
        } else {
            // Show error in dialog
            conn.files_management.new_directory_error = error;

            // Focus the input field so user can retry
            operation::focus(Id::from(InputId::NewDirectoryName))
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
        if !matches!(routing, Some(ResponseRouting::FileDeleteResult)) {
            return Task::none();
        }

        if success {
            // Close the delete dialog
            conn.files_management.pending_delete = None;
            conn.files_management.delete_error = None;

            // Refresh the current directory listing
            let current_path = conn.files_management.current_path.clone();
            let viewing_root = conn.files_management.viewing_root;
            let show_hidden = conn.files_management.show_hidden;

            // Clear entries to show loading state
            conn.files_management.entries = None;
            conn.files_management.error = None;

            // Send refresh request
            self.send_file_list_request(connection_id, current_path, viewing_root, show_hidden)
        } else {
            // Show error in the delete dialog (keep dialog open for retry)
            conn.files_management.delete_error = error;
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
        if !matches!(routing, Some(ResponseRouting::FileInfoResult)) {
            return Task::none();
        }

        if success {
            // Show the info dialog
            conn.files_management.pending_info = info;
        } else {
            // Show error in the files panel
            conn.files_management.error = error;
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
        if !matches!(routing, Some(ResponseRouting::FileRenameResult)) {
            return Task::none();
        }

        if success {
            // Close the rename dialog
            conn.files_management.pending_rename = None;
            conn.files_management.rename_name = String::new();
            conn.files_management.rename_error = None;

            // Refresh the current directory listing
            let current_path = conn.files_management.current_path.clone();
            let viewing_root = conn.files_management.viewing_root;
            let show_hidden = conn.files_management.show_hidden;

            // Clear entries to show loading state
            conn.files_management.entries = None;
            conn.files_management.error = None;

            // Send refresh request
            self.send_file_list_request(connection_id, current_path, viewing_root, show_hidden)
        } else {
            // Show error in the rename dialog (keep dialog open for retry)
            conn.files_management.rename_error = error;

            // Focus the input field so user can retry
            operation::focus(Id::from(InputId::RenameName))
        }
    }
}
