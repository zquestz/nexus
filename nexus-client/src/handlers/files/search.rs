//! File search handlers

use iced::Task;
use nexus_common::protocol::ClientMessage;
use nexus_common::validators::{self, SearchQueryError, validate_search_query};

use super::sort_search_results;
use super::strip_leading_slash;
use crate::NexusApp;
use crate::i18n::{t, t_args};
use crate::types::{FileSortColumn, Message, PendingRequests, ResponseRouting};

impl NexusApp {
    /// Handle search input text change
    pub fn handle_file_search_input_changed(&mut self, value: String) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        let tab = conn.files_management.active_tab_mut();
        tab.search_input = value;

        // Don't auto-clear search when input is emptied - let user explicitly
        // submit (Enter or button) to exit search mode. This allows them to
        // clear and type a new search without losing current results.

        Task::none()
    }

    /// Handle search submit (Enter or button click)
    pub fn handle_file_search_submit(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        let tab = conn.files_management.active_tab_mut();
        let query = tab.search_input.trim().to_string();

        // If query is empty, exit search mode and refresh the file list
        if query.is_empty() {
            let was_searching = tab.is_searching();
            tab.clear_search();

            // Refresh the file list to return to where we were
            if was_searching {
                let current_path = tab.current_path.clone();
                let viewing_root = tab.viewing_root;
                let show_hidden = self.config.settings.show_hidden_files;
                return self.send_file_list_request(
                    conn_id,
                    current_path,
                    viewing_root,
                    show_hidden,
                );
            }
            return Task::none();
        }

        // Validate the search query using shared validator
        if let Err(e) = validate_search_query(&query) {
            let error_msg = match e {
                SearchQueryError::Empty => {
                    // Already handled above, but included for completeness
                    return Task::none();
                }
                SearchQueryError::TooShort => t_args(
                    "files-search-query-too-short",
                    &[("min_length", &validators::MIN_QUERY_LENGTH.to_string())],
                ),
                SearchQueryError::TooLong => t_args(
                    "files-search-query-too-long",
                    &[(
                        "max_length",
                        &validators::MAX_SEARCH_QUERY_LENGTH.to_string(),
                    )],
                ),
                SearchQueryError::InvalidCharacters => t("files-search-query-invalid"),
            };
            tab.search_error = Some(error_msg);
            tab.search_query = Some(query);
            tab.search_results = None;
            tab.search_loading = false;
            return Task::none();
        }

        let tab_id = tab.id;
        let viewing_root = tab.viewing_root;

        // Use helper to send search request (handles loading state and race conditions)
        self.send_search_request(conn_id, tab_id, query, viewing_root)
    }

    /// Handle search result click (left-click) - opens new tab
    pub fn handle_file_search_result_clicked(
        &mut self,
        result: nexus_common::protocol::FileSearchResult,
    ) -> Task<Message> {
        self.open_search_result_in_new_tab(result)
    }

    /// Handle search result context menu - Download
    pub fn handle_file_search_result_download(
        &mut self,
        result: nexus_common::protocol::FileSearchResult,
    ) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get(&conn_id) else {
            return Task::none();
        };

        // Use the root context that was active when the search was performed
        // This ensures downloads work correctly even if user switches tabs
        let remote_root = conn.files_management.active_tab().search_viewing_root;

        // Strip leading slash for the download path
        let path = strip_leading_slash(&result.path);

        if result.is_directory {
            self.queue_download_with_root(path.to_string(), true, remote_root)
        } else {
            self.queue_download_with_root(path.to_string(), false, remote_root)
        }
    }

    /// Handle search result context menu - Info
    pub fn handle_file_search_result_info(
        &mut self,
        result: nexus_common::protocol::FileSearchResult,
    ) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        let tab_id = conn.files_management.active_tab_id();
        // Use the root context that was active when the search was performed
        let viewing_root = conn.files_management.active_tab().search_viewing_root;

        // Strip leading slash for the path
        let path = strip_leading_slash(&result.path);

        let message = ClientMessage::FileInfo {
            path: path.to_string(),
            root: viewing_root,
        };

        match conn.send(message) {
            Ok(message_id) => {
                conn.pending_requests
                    .track(message_id, ResponseRouting::FileInfoResult { tab_id });
            }
            Err(e) => {
                // Show error in the search tab
                if let Some(tab) = conn.files_management.tab_by_id_mut(tab_id) {
                    tab.search_error = Some(format!("{}: {}", t("err-send-failed"), e));
                }
            }
        }

        Task::none()
    }

    /// Handle search result context menu - Open (same as click)
    pub fn handle_file_search_result_open(
        &mut self,
        result: nexus_common::protocol::FileSearchResult,
    ) -> Task<Message> {
        self.open_search_result_in_new_tab(result)
    }

    /// Open a search result in a new tab
    ///
    /// For directories: navigates into the directory
    /// For files: navigates to the parent directory
    fn open_search_result_in_new_tab(
        &mut self,
        result: nexus_common::protocol::FileSearchResult,
    ) -> Task<Message> {
        use crate::types::FileTab;

        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        // Determine the target path
        let target_path = if result.is_directory {
            // Navigate into the directory
            strip_leading_slash(&result.path).to_string()
        } else {
            // Navigate to parent directory
            let path = strip_leading_slash(&result.path);
            if let Some(pos) = path.rfind('/') {
                path[..pos].to_string()
            } else {
                // File is at root
                String::new()
            }
        };

        // Use the root context that was active when the search was performed
        let viewing_root = conn.files_management.active_tab().search_viewing_root;

        // Create new tab at target path
        let new_tab = FileTab::new_at_path(target_path.clone(), viewing_root);
        let new_tab_id = new_tab.id;

        // Add and switch to the new tab
        conn.files_management.tabs.push(new_tab);
        conn.files_management.active_tab = conn.files_management.tabs.len() - 1;

        // Request file list for the new tab
        let message = ClientMessage::FileList {
            path: target_path,
            root: viewing_root,
            show_hidden: self.config.settings.show_hidden_files,
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

    /// Handle search results sort column click
    pub fn handle_file_search_sort_by(&mut self, column: FileSortColumn) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        let tab = conn.files_management.active_tab_mut();

        // Toggle direction if clicking same column, otherwise set new column ascending
        if tab.search_sort_column == column {
            tab.search_sort_ascending = !tab.search_sort_ascending;
        } else {
            tab.search_sort_column = column;
            tab.search_sort_ascending = true;
        }

        // Sort the search results in place
        if let Some(results) = &mut tab.search_results {
            sort_search_results(results, column, tab.search_sort_ascending);
        }

        Task::none()
    }
}
