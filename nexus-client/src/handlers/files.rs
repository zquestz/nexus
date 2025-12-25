//! Files panel handlers

use iced::Task;
use nexus_common::protocol::ClientMessage;

use crate::NexusApp;
use crate::i18n::t;
use crate::types::{ActivePanel, Message, PendingRequests, ResponseRouting};

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

        // Remember the current path - don't reset it
        let current_path = conn.files_management.current_path.clone();

        // Clear entries and error to show loading state, but keep the path
        conn.files_management.entries = None;
        conn.files_management.error = None;

        // Fetch the file list for the current path (or home if first time)
        self.send_file_list_request(conn_id, current_path)
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

        // Fetch the file list for the path
        self.send_file_list_request(conn_id, path)
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

        // Fetch the file list for the new path
        self.send_file_list_request(conn_id, new_path)
    }

    /// Navigate to the home directory (or refresh if already there)
    pub fn handle_file_navigate_home(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        // Navigate to home and clear entries to show loading state
        conn.files_management.navigate_to(String::new());

        // Fetch the file list for home
        self.send_file_list_request(conn_id, String::new())
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
        conn.files_management.entries = None;
        conn.files_management.error = None;

        // Re-fetch the file list for the current path
        self.send_file_list_request(conn_id, current_path)
    }

    // ==================== Helper Functions ====================

    /// Send a FileList request to the server
    fn send_file_list_request(&mut self, conn_id: usize, path: String) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        match conn.send(ClientMessage::FileList { path, root: false }) {
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
}
