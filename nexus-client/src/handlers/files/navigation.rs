//! File navigation handlers (navigate, refresh, toggle root/hidden)

use iced::Task;

use crate::NexusApp;
use crate::types::{ActivePanel, InputId, Message};

/// How the Files panel should open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FilesOpenIntent {
    /// Toolbar click: preserve the current tab path and no-op when already open.
    Toolbar,
    /// URI deep link: open the panel and resolve this non-empty path through
    /// the file-list response so it can become either navigation or download.
    UriPath(String),
}

impl NexusApp {
    // ==================== Panel Toggle ====================

    pub fn handle_toggle_files(&mut self, intent: FilesOpenIntent) -> Task<Message> {
        use crate::views::constants::PERMISSION_FILE_SEARCH;

        let uri_path = match intent {
            FilesOpenIntent::Toolbar => None,
            FilesOpenIntent::UriPath(path) if path.is_empty() => None,
            FilesOpenIntent::UriPath(path) => Some(path),
        };

        if uri_path.is_none() && self.active_panel() == ActivePanel::Files {
            return Task::none();
        }

        self.set_active_panel(ActivePanel::Files);

        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        // Check if user has file_search permission (for focus)
        let has_search = conn.has_permission(PERMISSION_FILE_SEARCH);

        // Initialize show_hidden from config on first open
        let show_hidden = self.config.settings.show_hidden_files;

        let tab = conn.files_management.active_tab_mut();

        if let Some(path) = uri_path {
            // Server resolves paths with folder type suffixes (e.g.,
            // "uploads" -> "uploads [NEXUS-UL]"), so the response handler
            // resolves the final segment after listing its parent.
            let (parent_path, target_name) = if let Some(slash_idx) = path.rfind('/') {
                (path[..slash_idx].to_string(), &path[slash_idx + 1..])
            } else {
                (String::new(), path.as_str())
            };
            let uri_target = (!target_name.is_empty()).then(|| target_name.to_string());

            tab.navigate_to(parent_path.clone());
            tab.viewing_root = false;
            tab.entries = None;
            tab.error = None;
            let tab_id = tab.id;

            return self.send_file_list_request_for_tab(
                conn_id,
                tab_id,
                parent_path,
                false,
                show_hidden,
                uri_target,
            );
        }

        // Remember the current path - don't reset it
        let current_path = tab.current_path.clone();
        let viewing_root = tab.viewing_root;

        // Clear entries and error to show loading state, but keep the path
        tab.entries = None;
        tab.error = None;

        // Fetch the file list for the current path (or home if first time)
        let fetch_task =
            self.send_file_list_request(conn_id, current_path, viewing_root, show_hidden);

        // Focus search input if user has permission
        if has_search {
            Task::batch([fetch_task, self.focus_field(InputId::FileSearchInput)])
        } else {
            fetch_task
        }
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
            let tab_id = tab.id;
            let viewing_root = tab.viewing_root;

            return self.send_search_request(conn_id, tab_id, query, viewing_root);
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
            let tab_id = tab.id;
            let viewing_root = tab.viewing_root;

            return self.send_search_request(conn_id, tab_id, query, viewing_root);
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

        // Toggle the show_hidden state in config
        let show_hidden = !self.config.settings.show_hidden_files;
        self.config.settings.show_hidden_files = show_hidden;
        let save_task = match self.config.save() {
            Ok(()) => Task::none(),
            Err(e) => self.add_background_error_message(
                conn_id,
                crate::i18n::t_args("err-failed-save-config", &[("error", &e)]),
            ),
        };

        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return save_task;
        };

        // Get current path and root state from active tab
        let tab = conn.files_management.active_tab_mut();
        let current_path = tab.current_path.clone();
        let viewing_root = tab.viewing_root;

        // Clear entries to show loading state
        tab.entries = None;
        tab.error = None;

        // Refresh the file list with new show_hidden setting
        Task::batch([
            save_task,
            self.send_file_list_request(conn_id, current_path, viewing_root, show_hidden),
        ])
    }
}
