//! File list response handlers

use iced::Task;
use nexus_common::framing::MessageId;
use nexus_common::protocol::FileEntry;

use crate::NexusApp;
use crate::types::{Message, ResponseRouting};

impl NexusApp {
    /// Handle file list response
    ///
    /// Populates the file entries in the files management panel.
    pub fn handle_file_list_response(
        &mut self,
        connection_id: usize,
        message_id: MessageId,
        success: bool,
        error: Option<String>,
        path: Option<String>,
        entries: Option<Vec<FileEntry>>,
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

        if success {
            // Update the current path if provided
            if let Some(path) = path {
                conn.files_management.current_path = path;
            }

            conn.files_management.entries = entries;
            conn.files_management.error = None;
        } else {
            conn.files_management.entries = None;
            conn.files_management.error = error;
        }

        Task::none()
    }
}
