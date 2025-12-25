//! File response handlers

use iced::widget::{Id, scrollable};
use iced::{Task, widget::operation};
use nexus_common::framing::MessageId;
use nexus_common::protocol::FileEntry;

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

            // Clear entries to show loading state
            conn.files_management.entries = None;
            conn.files_management.error = None;

            // Send refresh request
            self.send_file_list_request(connection_id, current_path, viewing_root)
        } else {
            // Show error in dialog
            conn.files_management.new_directory_error = error;

            // Focus the input field so user can retry
            operation::focus(Id::from(InputId::NewDirectoryName))
        }
    }
}
