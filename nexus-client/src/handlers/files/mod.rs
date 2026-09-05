//! Files panel handlers
//!
//! Sub-modules handle specific handler groups:
//! - `navigation` — Navigate, refresh, toggle root/hidden
//! - `directories` — New directory CRUD
//! - `operations` — Delete, info, rename, clipboard, overwrite, sort
//! - `tabs` — Tab new/switch/close
//! - `transfers` — Share, download, upload, drag-and-drop
//! - `search` — Search input/submit/result handlers

mod directories;
mod navigation;
mod operations;
mod scroll;
mod search;
mod tabs;
mod transfers;

use iced::Task;
use nexus_common::protocol::{ClientMessage, FileSearchResult};
use nexus_common::validators::{self, DirNameError};

use crate::NexusApp;
use crate::i18n::t;
use crate::types::{FileSortColumn, Message, PendingRequests, ResponseRouting, TabId};

pub(crate) use navigation::FilesOpenIntent;

/// Strip leading slash from a path
///
/// Search results have paths with leading slashes (e.g., "/Documents/file.txt"),
/// but server requests expect paths without leading slashes (e.g., "Documents/file.txt").
fn strip_leading_slash(path: &str) -> &str {
    path.strip_prefix('/').unwrap_or(path)
}

/// Sort search results by the specified column and direction
///
/// For the Name column, directories are always sorted first.
/// For other columns, ties are broken by name (case-insensitive, ascending).
///
/// Note: This function has parallel sorting logic to `FileTab::update_sorted_entries()`
/// in `types/form.rs`. That function sorts `FileEntry` (for directory listings),
/// while this sorts `FileSearchResult` (for search results). If you modify sorting
/// behavior here, consider whether the same change should apply there.
pub fn sort_search_results(
    results: &mut [FileSearchResult],
    column: FileSortColumn,
    ascending: bool,
) {
    match column {
        FileSortColumn::Name => {
            // Sort by name, keeping directories first
            results.sort_by(|a, b| match (a.is_directory, b.is_directory) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => {
                    let cmp = a.name.to_lowercase().cmp(&b.name.to_lowercase());
                    if ascending { cmp } else { cmp.reverse() }
                }
            });
        }
        FileSortColumn::Path => {
            results.sort_by(|a, b| {
                let cmp = a.path.to_lowercase().cmp(&b.path.to_lowercase());
                let cmp = if ascending { cmp } else { cmp.reverse() };
                // Sub-sort by name for items in same directory
                cmp.then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            });
        }
        FileSortColumn::Size => {
            results.sort_by(|a, b| {
                let cmp = a.size.cmp(&b.size);
                let cmp = if ascending { cmp } else { cmp.reverse() };
                // Sub-sort by name for items with same size
                cmp.then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            });
        }
        FileSortColumn::Modified => {
            results.sort_by(|a, b| {
                let cmp = a.modified.cmp(&b.modified);
                let cmp = if ascending { cmp } else { cmp.reverse() };
                // Sub-sort by name for items with same modified time
                cmp.then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            });
        }
    }
}

/// Convert a directory name validation error to a localized error message
fn dir_name_error_message(error: DirNameError) -> String {
    match error {
        DirNameError::Empty => t("err-dir-name-empty"),
        DirNameError::TooLong => crate::i18n::t_args(
            "err-dir-name-too-long",
            &[("max", &validators::MAX_DIR_NAME_LENGTH.to_string())],
        ),
        DirNameError::ContainsPathSeparator => t("err-dir-name-path-separator"),
        DirNameError::ContainsParentRef => t("err-dir-name-parent-ref"),
        DirNameError::ContainsNull | DirNameError::InvalidCharacters => t("err-dir-name-invalid"),
    }
}

impl NexusApp {
    /// Send a FileList request to the server for the active tab
    ///
    /// This is used for user-initiated navigation (navigate, refresh, etc.)
    /// where we always want to update the currently active tab.
    pub fn send_file_list_request(
        &mut self,
        conn_id: usize,
        path: String,
        root: bool,
        show_hidden: bool,
    ) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        let tab_id = conn.files_management.active_tab_id();
        self.send_file_list_request_for_tab(conn_id, tab_id, path, root, show_hidden, None)
    }

    /// Send a FileList request to the server for a specific tab
    ///
    /// This is used by response handlers that need to refresh a specific tab
    /// (identified by tab_id) rather than the currently active tab.
    ///
    /// `uri_target` is set when navigating via URI - the target file/folder to find
    /// and navigate to when the response arrives.
    pub fn send_file_list_request_for_tab(
        &mut self,
        conn_id: usize,
        tab_id: crate::types::TabId,
        path: String,
        root: bool,
        show_hidden: bool,
        uri_target: Option<String>,
    ) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        match conn.send(ClientMessage::FileList {
            path,
            root,
            show_hidden,
        }) {
            Ok(message_id) => {
                conn.pending_requests.track(
                    message_id,
                    ResponseRouting::PopulateFileList { tab_id, uri_target },
                );
            }
            Err(e) => {
                // Show error on the specific tab if it still exists, otherwise active tab
                if let Some(tab) = conn.files_management.tab_by_id_mut(tab_id) {
                    tab.error = Some(format!("{}: {}", t("err-send-failed"), e));
                } else {
                    conn.files_management.active_tab_mut().error =
                        Some(format!("{}: {}", t("err-send-failed"), e));
                }
            }
        }

        Task::none()
    }

    /// Send a FileSearch request to the server for a specific tab
    ///
    /// This helper consolidates the search request logic used by:
    /// - `handle_file_search_submit` - new search
    /// - `handle_file_refresh` - re-run current search
    /// - `handle_file_toggle_root` - re-run with toggled scope
    ///
    /// It sets the loading state, clears previous results, tracks the request
    /// for the specific tab, and stores the message_id to detect stale responses.
    fn send_search_request(
        &mut self,
        conn_id: usize,
        tab_id: TabId,
        query: String,
        viewing_root: bool,
    ) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        let Some(tab) = conn.files_management.tab_by_id_mut(tab_id) else {
            return Task::none();
        };

        // Set loading state and clear previous results
        // Store the viewing_root used for this search (for downloads from results)
        tab.scroll_target = None;
        tab.reset_scroll();
        tab.search_query = Some(query.clone());
        tab.search_viewing_root = viewing_root;
        tab.search_loading = true;
        tab.search_results = None;
        tab.search_error = None;

        let message = ClientMessage::FileSearch {
            query,
            root: viewing_root,
        };

        match conn.send(message) {
            Ok(message_id) => {
                // Store the message_id to detect stale responses.
                // Note: We need a second tab lookup here because we can't hold the mutable
                // borrow of `tab` across `conn.send()` (which also borrows `conn`), and we
                // only get the `message_id` after the send succeeds.
                if let Some(tab) = conn.files_management.tab_by_id_mut(tab_id) {
                    tab.current_search_request = Some(message_id);
                }
                conn.pending_requests
                    .track(message_id, ResponseRouting::FileSearchResult { tab_id });
            }
            Err(err) => {
                if let Some(tab) = conn.files_management.tab_by_id_mut(tab_id) {
                    tab.search_loading = false;
                    tab.search_error = Some(err);
                    tab.current_search_request = None;
                }
            }
        }

        Task::none()
    }
}

fn sanitize_filename(name: &str, fallback: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|c| match c {
            // Invalid on Windows and/or problematic on Unix
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            // Control characters
            c if c.is_control() => '_',
            c => c,
        })
        .collect();

    // Trim whitespace and dots from ends (Windows doesn't like trailing dots/spaces)
    let trimmed = sanitized.trim().trim_end_matches('.');

    // If empty after sanitization, use the fallback (typically server address)
    if trimmed.is_empty() {
        return fallback.to_string();
    }

    // Check for Windows reserved names (case-insensitive). The reservation
    // applies to the stem before the first dot, so "CON.txt" is reserved too.
    let stem = trimmed.split('.').next().unwrap_or(trimmed);
    let upper = stem.to_uppercase();
    let is_reserved = matches!(
        upper.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    );

    if is_reserved {
        // Prefix with underscore to make it safe
        format!("_{trimmed}")
    } else {
        trimmed.to_string()
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_filename_normal() {
        assert_eq!(sanitize_filename("My Server", "fallback"), "My Server");
        assert_eq!(sanitize_filename("test", "fallback"), "test");
        assert_eq!(sanitize_filename("server123", "fallback"), "server123");
    }

    #[test]
    fn test_sanitize_filename_invalid_chars() {
        assert_eq!(sanitize_filename("foo/bar", "fallback"), "foo_bar");
        assert_eq!(sanitize_filename("foo\\bar", "fallback"), "foo_bar");
        assert_eq!(sanitize_filename("foo:bar", "fallback"), "foo_bar");
        assert_eq!(sanitize_filename("foo*bar", "fallback"), "foo_bar");
        assert_eq!(sanitize_filename("foo?bar", "fallback"), "foo_bar");
        assert_eq!(sanitize_filename("foo\"bar", "fallback"), "foo_bar");
        assert_eq!(sanitize_filename("foo<bar", "fallback"), "foo_bar");
        assert_eq!(sanitize_filename("foo>bar", "fallback"), "foo_bar");
        assert_eq!(sanitize_filename("foo|bar", "fallback"), "foo_bar");
        // Multiple invalid chars
        assert_eq!(sanitize_filename("a/b\\c:d", "fallback"), "a_b_c_d");
    }

    #[test]
    fn test_sanitize_filename_control_chars() {
        assert_eq!(sanitize_filename("foo\x00bar", "fallback"), "foo_bar");
        assert_eq!(sanitize_filename("foo\nbar", "fallback"), "foo_bar");
        assert_eq!(sanitize_filename("foo\tbar", "fallback"), "foo_bar");
    }

    #[test]
    fn test_sanitize_filename_trailing_dots_spaces() {
        assert_eq!(sanitize_filename("test.", "fallback"), "test");
        assert_eq!(sanitize_filename("test...", "fallback"), "test");
        assert_eq!(sanitize_filename("test ", "fallback"), "test");
        assert_eq!(sanitize_filename(" test ", "fallback"), "test");
        assert_eq!(sanitize_filename("test. ", "fallback"), "test");
    }

    #[test]
    fn test_sanitize_filename_empty_fallback() {
        assert_eq!(sanitize_filename("", "fallback"), "fallback");
        assert_eq!(sanitize_filename("   ", "fallback"), "fallback");
        assert_eq!(sanitize_filename("...", "fallback"), "fallback");
        // Note: "///" becomes "___" (slashes replaced), not fallback
        assert_eq!(sanitize_filename("///", "192.168.1.1"), "___");
    }

    #[test]
    fn test_sanitize_filename_windows_reserved() {
        // Reserved names should be prefixed with underscore
        assert_eq!(sanitize_filename("CON", "fallback"), "_CON");
        assert_eq!(sanitize_filename("con", "fallback"), "_con");
        assert_eq!(sanitize_filename("Con", "fallback"), "_Con");
        assert_eq!(sanitize_filename("PRN", "fallback"), "_PRN");
        assert_eq!(sanitize_filename("AUX", "fallback"), "_AUX");
        assert_eq!(sanitize_filename("NUL", "fallback"), "_NUL");
        assert_eq!(sanitize_filename("COM1", "fallback"), "_COM1");
        assert_eq!(sanitize_filename("COM9", "fallback"), "_COM9");
        assert_eq!(sanitize_filename("LPT1", "fallback"), "_LPT1");
        assert_eq!(sanitize_filename("LPT9", "fallback"), "_LPT9");
        // Reserved even with an extension — the stem before the first dot is
        // what's reserved on Windows.
        assert_eq!(sanitize_filename("CON.txt", "fallback"), "_CON.txt");
        assert_eq!(sanitize_filename("nul.log", "fallback"), "_nul.log");
        assert_eq!(sanitize_filename("COM1.tar.gz", "fallback"), "_COM1.tar.gz");
        // A non-reserved stem (e.g. "console") with an extension is untouched.
        assert_eq!(sanitize_filename("console.txt", "fallback"), "console.txt");
    }

    #[test]
    fn test_sanitize_filename_unicode() {
        // Unicode should pass through unchanged
        assert_eq!(sanitize_filename("服务器", "fallback"), "服务器");
        assert_eq!(sanitize_filename("サーバー", "fallback"), "サーバー");
        assert_eq!(sanitize_filename("Сервер", "fallback"), "Сервер");
    }

    // =========================================================================
    // sort_search_results Tests
    // =========================================================================

    fn make_search_result(
        name: &str,
        path: &str,
        size: u64,
        is_directory: bool,
    ) -> nexus_common::protocol::FileSearchResult {
        nexus_common::protocol::FileSearchResult {
            path: path.to_string(),
            name: name.to_string(),
            size,
            modified: 0,
            is_directory,
        }
    }

    #[test]
    fn test_sort_search_results_by_name_directories_first() {
        let mut results = vec![
            make_search_result("zebra.txt", "/zebra.txt", 100, false),
            make_search_result("alpha", "/alpha", 0, true),
            make_search_result("apple.txt", "/apple.txt", 200, false),
            make_search_result("beta", "/beta", 0, true),
        ];

        sort_search_results(&mut results, FileSortColumn::Name, true);

        // Directories should come first, then files, both alphabetically
        assert_eq!(results[0].name, "alpha");
        assert!(results[0].is_directory);
        assert_eq!(results[1].name, "beta");
        assert!(results[1].is_directory);
        assert_eq!(results[2].name, "apple.txt");
        assert!(!results[2].is_directory);
        assert_eq!(results[3].name, "zebra.txt");
        assert!(!results[3].is_directory);
    }

    #[test]
    fn test_sort_search_results_by_name_descending() {
        let mut results = vec![
            make_search_result("apple.txt", "/apple.txt", 100, false),
            make_search_result("zebra.txt", "/zebra.txt", 200, false),
            make_search_result("middle.txt", "/middle.txt", 150, false),
        ];

        sort_search_results(&mut results, FileSortColumn::Name, false);

        assert_eq!(results[0].name, "zebra.txt");
        assert_eq!(results[1].name, "middle.txt");
        assert_eq!(results[2].name, "apple.txt");
    }

    #[test]
    fn test_sort_search_results_by_path() {
        let mut results = vec![
            make_search_result("file.txt", "/z/file.txt", 100, false),
            make_search_result("file.txt", "/a/file.txt", 100, false),
            make_search_result("file.txt", "/m/file.txt", 100, false),
        ];

        sort_search_results(&mut results, FileSortColumn::Path, true);

        assert_eq!(results[0].path, "/a/file.txt");
        assert_eq!(results[1].path, "/m/file.txt");
        assert_eq!(results[2].path, "/z/file.txt");
    }

    #[test]
    fn test_sort_search_results_by_path_subsorts_by_name() {
        let mut results = vec![
            make_search_result("zebra.txt", "/docs/zebra.txt", 100, false),
            make_search_result("apple.txt", "/docs/apple.txt", 100, false),
            make_search_result("banana.txt", "/docs/banana.txt", 100, false),
        ];

        sort_search_results(&mut results, FileSortColumn::Path, true);

        // Same path prefix, should be sub-sorted by name
        assert_eq!(results[0].name, "apple.txt");
        assert_eq!(results[1].name, "banana.txt");
        assert_eq!(results[2].name, "zebra.txt");
    }

    #[test]
    fn test_sort_search_results_by_size() {
        let mut results = vec![
            make_search_result("medium.txt", "/medium.txt", 500, false),
            make_search_result("small.txt", "/small.txt", 100, false),
            make_search_result("large.txt", "/large.txt", 1000, false),
        ];

        sort_search_results(&mut results, FileSortColumn::Size, true);

        assert_eq!(results[0].size, 100);
        assert_eq!(results[1].size, 500);
        assert_eq!(results[2].size, 1000);

        // Descending
        sort_search_results(&mut results, FileSortColumn::Size, false);

        assert_eq!(results[0].size, 1000);
        assert_eq!(results[1].size, 500);
        assert_eq!(results[2].size, 100);
    }

    #[test]
    fn test_sort_search_results_by_size_subsorts_by_name() {
        let mut results = vec![
            make_search_result("zebra.txt", "/zebra.txt", 100, false),
            make_search_result("apple.txt", "/apple.txt", 100, false),
            make_search_result("banana.txt", "/banana.txt", 100, false),
        ];

        sort_search_results(&mut results, FileSortColumn::Size, true);

        // Same size, should be sub-sorted by name
        assert_eq!(results[0].name, "apple.txt");
        assert_eq!(results[1].name, "banana.txt");
        assert_eq!(results[2].name, "zebra.txt");
    }

    #[test]
    fn test_sort_search_results_case_insensitive() {
        let mut results = vec![
            make_search_result("Zebra.txt", "/Zebra.txt", 100, false),
            make_search_result("apple.txt", "/apple.txt", 100, false),
            make_search_result("BANANA.txt", "/BANANA.txt", 100, false),
        ];

        sort_search_results(&mut results, FileSortColumn::Name, true);

        assert_eq!(results[0].name, "apple.txt");
        assert_eq!(results[1].name, "BANANA.txt");
        assert_eq!(results[2].name, "Zebra.txt");
    }

    #[test]
    fn test_sort_search_results_empty() {
        let mut results: Vec<nexus_common::protocol::FileSearchResult> = vec![];
        // Should not panic on empty vec
        sort_search_results(&mut results, FileSortColumn::Name, true);
        assert!(results.is_empty());
    }

    #[test]
    fn test_sort_search_results_single_item() {
        let mut results = vec![make_search_result("test.txt", "/test.txt", 100, false)];
        sort_search_results(&mut results, FileSortColumn::Name, true);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "test.txt");
    }

    #[test]
    fn test_sort_search_results_path_case_insensitive() {
        let mut results = vec![
            make_search_result("file.txt", "/Zebra/file.txt", 100, false),
            make_search_result("file.txt", "/apple/file.txt", 100, false),
            make_search_result("file.txt", "/BANANA/file.txt", 100, false),
        ];

        sort_search_results(&mut results, FileSortColumn::Path, true);

        assert_eq!(results[0].path, "/apple/file.txt");
        assert_eq!(results[1].path, "/BANANA/file.txt");
        assert_eq!(results[2].path, "/Zebra/file.txt");
    }

    #[test]
    fn test_sort_search_results_modified() {
        let mut results = vec![
            make_search_result("old.txt", "/old.txt", 100, false),
            make_search_result("new.txt", "/new.txt", 100, false),
            make_search_result("mid.txt", "/mid.txt", 100, false),
        ];
        // Manually set modified times
        results[0].modified = 1000;
        results[1].modified = 3000;
        results[2].modified = 2000;

        sort_search_results(&mut results, FileSortColumn::Modified, true);

        assert_eq!(results[0].modified, 1000);
        assert_eq!(results[1].modified, 2000);
        assert_eq!(results[2].modified, 3000);

        // Descending
        sort_search_results(&mut results, FileSortColumn::Modified, false);

        assert_eq!(results[0].modified, 3000);
        assert_eq!(results[1].modified, 2000);
        assert_eq!(results[2].modified, 1000);
    }

    #[test]
    fn test_sort_search_results_modified_subsorts_by_name() {
        let mut results = vec![
            make_search_result("zebra.txt", "/zebra.txt", 100, false),
            make_search_result("apple.txt", "/apple.txt", 100, false),
            make_search_result("banana.txt", "/banana.txt", 100, false),
        ];
        // Same modified time for all
        results[0].modified = 1000;
        results[1].modified = 1000;
        results[2].modified = 1000;

        sort_search_results(&mut results, FileSortColumn::Modified, true);

        // Same modified time, should be sub-sorted by name
        assert_eq!(results[0].name, "apple.txt");
        assert_eq!(results[1].name, "banana.txt");
        assert_eq!(results[2].name, "zebra.txt");
    }

    #[test]
    fn test_sort_search_results_directories_always_first_for_name() {
        let mut results = vec![
            make_search_result("zebra", "/zebra", 0, true),
            make_search_result("apple.txt", "/apple.txt", 100, false),
            make_search_result("alpha", "/alpha", 0, true),
            make_search_result("banana.txt", "/banana.txt", 200, false),
        ];

        // Ascending - dirs first, then files, both alphabetical
        sort_search_results(&mut results, FileSortColumn::Name, true);

        assert!(results[0].is_directory);
        assert_eq!(results[0].name, "alpha");
        assert!(results[1].is_directory);
        assert_eq!(results[1].name, "zebra");
        assert!(!results[2].is_directory);
        assert_eq!(results[2].name, "apple.txt");
        assert!(!results[3].is_directory);
        assert_eq!(results[3].name, "banana.txt");

        // Descending - dirs still first, but both groups reversed
        sort_search_results(&mut results, FileSortColumn::Name, false);

        assert!(results[0].is_directory);
        assert_eq!(results[0].name, "zebra");
        assert!(results[1].is_directory);
        assert_eq!(results[1].name, "alpha");
        assert!(!results[2].is_directory);
        assert_eq!(results[2].name, "banana.txt");
        assert!(!results[3].is_directory);
        assert_eq!(results[3].name, "apple.txt");
    }
}
