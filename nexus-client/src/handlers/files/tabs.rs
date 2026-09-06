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
    /// orphaned entries in the pending_requests map. The cleanup runs only when
    /// the tab is actually removed: `close_tab_by_id` refuses to close the last
    /// remaining tab, and dropping the responses of a tab that stays open would
    /// strand it in a permanent loading state.
    pub fn handle_file_tab_close(&mut self, tab_id: crate::types::TabId) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        if !conn.files_management.close_tab_by_id(tab_id) {
            return Task::none();
        }

        // Clean up pending requests for the tab that was removed
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

        Task::none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use nexus_common::framing::MessageId;

    use crate::testing::support::test_connection_with_receiver;
    use crate::types::TabId;

    /// Build an app whose active file tab is waiting on a listing.
    fn app_with_pending_listing(tab_count: usize) -> (NexusApp, TabId, MessageId) {
        let (conn, _rx) = test_connection_with_receiver(1);
        let mut app = NexusApp {
            active_connection: Some(1),
            ..NexusApp::default()
        };
        app.connections.insert(1, conn);

        let conn = app.connections.get_mut(&1).expect("test connection");
        while conn.files_management.tabs.len() < tab_count {
            conn.files_management.tabs.push(FileTab::default());
        }
        conn.files_management.active_tab = conn.files_management.tabs.len() - 1;

        let tab_id = conn.files_management.active_tab_id();
        let message_id = MessageId::new();
        conn.pending_requests.track(
            message_id,
            ResponseRouting::PopulateFileList {
                tab_id,
                uri_target: None,
            },
        );
        (app, tab_id, message_id)
    }

    #[test]
    fn closing_a_tab_drops_the_requests_it_was_waiting_on() {
        let (mut app, tab_id, message_id) = app_with_pending_listing(2);

        drop(app.handle_file_tab_close(tab_id));

        let conn = &app.connections[&1];
        assert!(conn.files_management.tab_by_id(tab_id).is_none());
        assert!(!conn.pending_requests.contains_key(&message_id));
    }

    #[test]
    fn refusing_to_close_the_last_tab_keeps_its_pending_requests() {
        let (mut app, tab_id, message_id) = app_with_pending_listing(1);

        drop(app.handle_file_tab_close(tab_id));

        let conn = &app.connections[&1];
        assert!(
            conn.files_management.tab_by_id(tab_id).is_some(),
            "the last tab stays open"
        );
        assert!(
            conn.pending_requests.contains_key(&message_id),
            "a tab that stays open must still receive its listing"
        );
    }

    #[test]
    fn closing_a_tab_leaves_other_tabs_waiting() {
        let (mut app, closed_tab, closed_request) = app_with_pending_listing(2);
        let other_tab = app.connections[&1].files_management.tabs[0].id;
        let other_request = MessageId::new();
        app.connections
            .get_mut(&1)
            .expect("test connection")
            .pending_requests
            .track(
                other_request,
                ResponseRouting::PopulateFileList {
                    tab_id: other_tab,
                    uri_target: None,
                },
            );

        drop(app.handle_file_tab_close(closed_tab));

        let conn = &app.connections[&1];
        assert!(!conn.pending_requests.contains_key(&closed_request));
        assert!(conn.pending_requests.contains_key(&other_request));
    }
}
