//! File browser tab handlers

use iced::Task;
use nexus_common::protocol::ClientMessage;

use crate::NexusApp;
use crate::types::{FileTab, Message, PendingRequests, ResponseRouting};

impl NexusApp {
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

    /// Open a directory in a new file browser tab.
    pub fn handle_file_open_directory_in_new_tab(&mut self, path: String) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };

        let (viewing_root, new_tab_id) = {
            let Some(conn) = self.connections.get_mut(&conn_id) else {
                return Task::none();
            };

            let viewing_root = conn.files_management.active_tab().viewing_root;
            let new_tab = FileTab::new_at_path(path.clone(), viewing_root);
            let new_tab_id = new_tab.id;

            conn.files_management.tabs.push(new_tab);
            conn.files_management.active_tab = conn.files_management.tabs.len() - 1;

            (viewing_root, new_tab_id)
        };

        let show_hidden = self.config.settings.show_hidden_files;
        let message = ClientMessage::FileList {
            path,
            root: viewing_root,
            show_hidden,
        };

        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        match conn.send(message) {
            Ok(message_id) => {
                conn.pending_requests.track(
                    message_id,
                    ResponseRouting::PopulateFileList {
                        tab_id: new_tab_id,
                        uri_target: None,
                    },
                );
            }
            Err(err) => {
                if let Some(tab) = conn.files_management.tab_by_id_mut(new_tab_id) {
                    tab.error = Some(err);
                }
            }
        }

        Task::none()
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
    ///
    /// Also cleans up any pending requests associated with this tab to prevent
    /// orphaned entries in the pending_requests map.
    pub fn handle_file_tab_close(&mut self, tab_id: crate::types::TabId) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        // Clean up pending requests for this tab
        conn.pending_requests.retain(|_, routing| {
            !matches!(
                routing,
                ResponseRouting::PopulateFileList { tab_id: tid, .. }
                    | ResponseRouting::FileCreateDirResult { tab_id: tid }
                    | ResponseRouting::FileDeleteResult { tab_id: tid }
                    | ResponseRouting::FileInfoResult { tab_id: tid }
                    | ResponseRouting::FileRenameResult { tab_id: tid }
                    | ResponseRouting::FileMoveResult { tab_id: tid, .. }
                    | ResponseRouting::FileCopyResult { tab_id: tid, .. }
                    | ResponseRouting::FileSearchResult { tab_id: tid }
                    if *tid == tab_id
            )
        });

        conn.files_management.close_tab_by_id(tab_id);
        Task::none()
    }
}
