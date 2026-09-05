//! File search handlers

use iced::Task;
use iced::widget::Id;
use nexus_common::protocol::ClientMessage;
use nexus_common::validators::{self, SearchQueryError, validate_search_query};

use super::sort_search_results;
use super::strip_leading_slash;
use crate::NexusApp;
use crate::i18n::{t, t_args};
use crate::types::{
    FileScrollTarget, FileSortColumn, FileTab, Message, PendingRequests, ResponseRouting,
};

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
        let query = tab.search_input.clone();

        // If the query is empty (or only whitespace), exit search mode and
        // refresh the file list. Emptiness gate only — a non-empty query is
        // searched as typed.
        if query.trim().is_empty() {
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

    /// Handle search result click (left-click on a directory) - opens new tab
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

    /// Handle search result context menu - Open in a new tab
    pub fn handle_file_search_result_open(
        &mut self,
        result: nexus_common::protocol::FileSearchResult,
    ) -> Task<Message> {
        self.open_search_result_in_new_tab(result)
    }

    /// Open a search result in a new tab.
    ///
    /// For directories: navigates into the directory.
    /// For files: navigates to the parent directory and scrolls to the file.
    fn open_search_result_in_new_tab(
        &mut self,
        result: nexus_common::protocol::FileSearchResult,
    ) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        let target_path = if result.is_directory {
            strip_leading_slash(&result.path).to_string()
        } else {
            let path = strip_leading_slash(&result.path);
            if let Some(pos) = path.rfind('/') {
                path[..pos].to_string()
            } else {
                String::new()
            }
        };

        // Use the root context that was active when the search was performed
        let viewing_root = conn.files_management.active_tab().search_viewing_root;

        // Create new tab at target path
        let mut new_tab = FileTab::new_at_path(target_path.clone(), viewing_root);
        if !result.is_directory {
            new_tab.scroll_target = Some(FileScrollTarget {
                name: result
                    .path
                    .rsplit_once('/')
                    .map_or(result.path.as_str(), |(_, name)| name)
                    .to_string(),
                id: Id::unique(),
            });
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use iced::advanced::renderer::Headless;
    use iced::advanced::widget::{
        Operation,
        operation::{
            Outcome,
            scrollable::{self, AbsoluteOffset, Scrollable},
        },
    };
    use iced::advanced::{clipboard, mouse};
    use iced::futures::StreamExt;
    use iced::{Event, Font, Pixels, Size, window};
    use iced_runtime::user_interface::{Cache, UserInterface};
    use iced_runtime::{Action, task};
    use nexus_common::framing::MessageId;
    use nexus_common::protocol::{FileEntry, FileSearchResult, ServerMessage};
    use tokio::sync::mpsc;

    use crate::testing::support::test_connection_with_receiver;
    use crate::transfers::TransferManager;
    use crate::types::{ActivePanel, FileScrollOutcome};
    use crate::views::constants::{
        PERMISSION_FILE_COPY, PERMISSION_FILE_CREATE_DIR, PERMISSION_FILE_DELETE,
        PERMISSION_FILE_DOWNLOAD, PERMISSION_FILE_INFO, PERMISSION_FILE_LIST, PERMISSION_FILE_MOVE,
        PERMISSION_FILE_RENAME, PERMISSION_FILE_ROOT, PERMISSION_FILE_SEARCH,
        PERMISSION_FILE_UPLOAD, PERMISSION_FILE_UPLOAD_ANYWHERE,
    };
    use crate::views::files::{FilePermissions, files_view};

    fn search_app() -> (
        NexusApp,
        mpsc::UnboundedReceiver<(MessageId, ClientMessage)>,
    ) {
        let (mut conn, rx) = test_connection_with_receiver(1);
        conn.is_admin = false;
        conn.permissions = vec![
            PERMISSION_FILE_LIST.to_string(),
            PERMISSION_FILE_SEARCH.to_string(),
        ];
        let tab = conn.files_management.active_tab_mut();
        tab.search_query = Some("target".to_string());
        tab.search_viewing_root = true;
        let mut app = NexusApp {
            active_connection: Some(1),
            transfer_manager: TransferManager::new(),
            ..NexusApp::default()
        };
        app.connections.insert(1, conn);
        app.set_active_panel(ActivePanel::Files);
        (app, rx)
    }

    fn result(path: &str, is_directory: bool) -> FileSearchResult {
        FileSearchResult {
            name: path
                .rsplit('/')
                .next()
                .expect("path has a final segment")
                .to_string(),
            path: path.to_string(),
            size: 100,
            modified: 0,
            is_directory,
        }
    }

    fn listing(names: &[&str]) -> ServerMessage {
        ServerMessage::FileListResponse {
            success: true,
            error: None,
            path: Some("docs".to_string()),
            entries: Some(
                names
                    .iter()
                    .map(|name| FileEntry {
                        name: (*name).to_string(),
                        size: 100,
                        modified: 0,
                        dir_type: None,
                        can_upload: false,
                    })
                    .collect(),
            ),
            can_upload: false,
            dropbox_owner: None,
        }
    }

    fn draw_files(
        app: &NexusApp,
        cache: Cache,
        renderer: &mut iced::Renderer,
    ) -> (Cache, Vec<Message>) {
        draw_files_with_operation(app, cache, renderer, None)
    }

    fn draw_files_with_operation(
        app: &NexusApp,
        cache: Cache,
        renderer: &mut iced::Renderer,
        operation: Option<&mut dyn Operation>,
    ) -> (Cache, Vec<Message>) {
        draw_files_with_events(
            app,
            cache,
            renderer,
            operation,
            &[],
            mouse::Cursor::Unavailable,
        )
    }

    fn draw_files_with_events(
        app: &NexusApp,
        cache: Cache,
        renderer: &mut iced::Renderer,
        operation: Option<&mut dyn Operation>,
        events: &[Event],
        cursor: mouse::Cursor,
    ) -> (Cache, Vec<Message>) {
        let conn = &app.connections[&1];
        let files = &conn.files_management;
        let perms = FilePermissions {
            file_root: conn.has_permission(PERMISSION_FILE_ROOT),
            file_create_dir: conn.has_permission(PERMISSION_FILE_CREATE_DIR),
            file_info: conn.has_permission(PERMISSION_FILE_INFO),
            file_delete: conn.has_permission(PERMISSION_FILE_DELETE),
            file_rename: conn.has_permission(PERMISSION_FILE_RENAME),
            file_move: conn.has_permission(PERMISSION_FILE_MOVE),
            file_copy: conn.has_permission(PERMISSION_FILE_COPY),
            file_download: conn.has_permission(PERMISSION_FILE_DOWNLOAD),
            file_upload: conn
                .has_any_permission(&[PERMISSION_FILE_UPLOAD, PERMISSION_FILE_UPLOAD_ANYWHERE]),
            file_upload_anywhere: conn.has_permission(PERMISSION_FILE_UPLOAD_ANYWHERE),
            file_search: conn.has_permission(PERMISSION_FILE_SEARCH),
        };
        let mut ui = UserInterface::build(
            files_view(
                files,
                perms,
                false,
                app.dragging_files && app.can_accept_file_drop(),
                "me",
            ),
            Size::new(640.0, 480.0),
            cache,
            renderer,
        );
        if let Some(operation) = operation {
            ui.operate(renderer, operation);
            let mut outcome = operation.finish();
            while let Outcome::Chain(mut next) = outcome {
                ui.operate(renderer, next.as_mut());
                outcome = next.finish();
            }
        }
        let mut messages = Vec::new();
        let mut events = events.to_vec();
        events.push(Event::Window(window::Event::RedrawRequested(
            iced::time::Instant::now(),
        )));
        let _ = ui.update(
            &events,
            cursor,
            renderer,
            &mut clipboard::Null,
            &mut messages,
        );
        (ui.into_cache(), messages)
    }

    fn reveal_message(messages: Vec<Message>) -> Message {
        messages
            .into_iter()
            .find(|message| matches!(message, Message::FileScrollToTarget { .. }))
            .expect("rendering a pending target must request a reveal")
    }

    #[derive(Default)]
    struct CaptureScroll {
        id: Option<Id>,
        offset: Option<f32>,
        bounds: Option<iced::Rectangle>,
        texts: Vec<(String, iced::Rectangle)>,
    }

    impl Operation for CaptureScroll {
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
            operate(self);
        }

        fn scrollable(
            &mut self,
            id: Option<&Id>,
            bounds: iced::Rectangle,
            _content: iced::Rectangle,
            translation: iced::Vector,
            _state: &mut dyn Scrollable,
        ) {
            if self
                .id
                .as_ref()
                .is_some_and(|expected| id != Some(expected))
            {
                return;
            }
            assert!(
                self.offset.is_none(),
                "only one file viewport should be visible"
            );
            self.offset = Some(translation.y);
            self.bounds = Some(bounds);
        }

        fn text(&mut self, _id: Option<&Id>, bounds: iced::Rectangle, text: &str) {
            self.texts.push((text.to_string(), bounds));
        }
    }

    async fn run_reveal_operation(
        app: &NexusApp,
        cache: Cache,
        renderer: &mut iced::Renderer,
        reveal: Task<Message>,
    ) -> (Cache, Message) {
        let mut actions = task::into_stream(reveal).expect("reveal task");
        let Some(Action::Widget(mut operation)) = actions.next().await else {
            panic!("widget operation");
        };
        let (cache, _) = draw_files_with_operation(app, cache, renderer, Some(operation.as_mut()));
        let Some(Action::Output(completion)) = actions.next().await else {
            panic!("reveal completion");
        };
        (cache, completion)
    }

    #[tokio::test]
    async fn opening_search_results_preserves_each_tabs_scroll_position() {
        let mut renderer = iced::Renderer::new(Font::DEFAULT, Pixels(16.0), Some("tiny-skia"))
            .await
            .expect("headless software renderer");
        for existing_tabs in [1, 2] {
            for search_offset in [0.0, 240.0] {
                for is_directory in [false, true] {
                    let (mut app, mut rx) = search_app();
                    let files = &mut app
                        .connections
                        .get_mut(&1)
                        .expect("connection")
                        .files_management;
                    if existing_tabs == 2 {
                        files.tabs.insert(0, FileTab::default());
                        files.active_tab = 1;
                    }
                    let tab = files.active_tab_mut();
                    tab.search_results = Some(
                        (0..30)
                            .map(|i| result(&format!("/docs/file-{i:03}.txt"), false))
                            .collect(),
                    );
                    let search_tab = tab.id;
                    let search_scroll_id = tab.scroll_id.clone();
                    let mut set_offset = scrollable::scroll_to(
                        search_scroll_id,
                        AbsoluteOffset {
                            x: None,
                            y: Some(search_offset),
                        },
                    );
                    let (mut cache, messages) = draw_files_with_operation(
                        &app,
                        Cache::new(),
                        &mut renderer,
                        Some(&mut set_offset),
                    );
                    for message in messages {
                        drop(app.update(message));
                    }
                    assert_eq!(
                        app.connections[&1]
                            .files_management
                            .active_tab()
                            .scroll_offset,
                        search_offset
                    );

                    drop(app.handle_file_search_result_open(result(
                        if is_directory {
                            "/docs"
                        } else {
                            "/docs/file-070.txt"
                        },
                        is_directory,
                    )));
                    let destination_tab = app.connections[&1].files_management.active_tab().id;
                    let (request, _) = rx.try_recv().expect("listing request");
                    // Preserve the cache through the loading view as the app does.
                    (cache, _) = draw_files(&app, cache, &mut renderer);
                    let names: Vec<_> = (0..100).map(|i| format!("file-{i:03}.txt")).collect();
                    drop(app.handle_server_message_received(
                        1,
                        request,
                        listing(&names.iter().map(String::as_str).collect::<Vec<_>>()),
                        None,
                    ));
                    let (next_cache, messages) = draw_files(&app, cache, &mut renderer);
                    cache = next_cache;
                    if !is_directory {
                        let reveal = app.update(reveal_message(messages));
                        let (next_cache, completion) =
                            run_reveal_operation(&app, cache, &mut renderer, reveal).await;
                        cache = next_cache;
                        drop(app.update(completion));
                    } else {
                        for message in messages {
                            drop(app.update(message));
                        }
                    }
                    let destination_offset = app.connections[&1]
                        .files_management
                        .active_tab()
                        .scroll_offset;
                    if is_directory {
                        assert_eq!(destination_offset, 0.0, "enter directories at the top");
                    } else {
                        assert!(destination_offset > 1500.0, "reveal the distant file");
                    }

                    // Return before another redraw can save the reveal offset.
                    drop(app.handle_file_tab_switch(search_tab));
                    let mut capture = CaptureScroll::default();
                    let (next_cache, messages) =
                        draw_files_with_operation(&app, cache, &mut renderer, Some(&mut capture));
                    cache = next_cache;
                    assert_eq!(
                        capture.offset,
                        Some(search_offset),
                        "return to the original search position"
                    );
                    for message in messages {
                        drop(app.update(message));
                    }
                    assert_eq!(
                        app.connections[&1]
                            .files_management
                            .active_tab()
                            .scroll_offset,
                        search_offset
                    );

                    drop(app.handle_file_tab_switch(destination_tab));
                    let mut capture = CaptureScroll::default();
                    (cache, _) =
                        draw_files_with_operation(&app, cache, &mut renderer, Some(&mut capture));
                    assert_eq!(
                        capture.offset,
                        Some(destination_offset.round()),
                        "destination position is independent"
                    );

                    drop(app.handle_file_tab_close(destination_tab));
                    let mut capture = CaptureScroll::default();
                    let _ =
                        draw_files_with_operation(&app, cache, &mut renderer, Some(&mut capture));
                    assert_eq!(
                        capture.offset,
                        Some(search_offset),
                        "closing the destination restores search too"
                    );
                }
            }
        }
    }

    #[test]
    fn open_file_retains_exact_target_and_search_scope_without_downloading() {
        for (path, parent, name) in [
            ("/docs/target.txt", "docs", "target.txt"),
            ("/target.txt", "", "target.txt"),
            ("target.txt", "", "target.txt"),
            ("/docs/Target.txt", "docs", "Target.txt"),
            (
                "/docs/\u{65e5}\u{672c}\u{8a9e}.txt",
                "docs",
                "\u{65e5}\u{672c}\u{8a9e}.txt",
            ),
        ] {
            let (mut app, mut rx) = search_app();
            let mut search_result = result(path, false);
            search_result.name = "display-only.txt".to_string();
            drop(app.handle_file_search_result_open(search_result));
            let (message_id, message) = rx.try_recv().expect("file-list request");
            assert!(
                matches!(message, ClientMessage::FileList { path, root: true, .. } if path == parent)
            );
            assert!(
                rx.try_recv().is_err(),
                "Open sends only one listing request"
            );
            assert_eq!(
                app.transfer_manager.all().count(),
                0,
                "Open must not queue a transfer"
            );
            let conn = &app.connections[&1];
            let tab = conn.files_management.active_tab();
            assert_eq!(tab.current_path, parent);
            assert_eq!(tab.scroll_target.as_ref().expect("file target").name, name);
            assert!(
                matches!(conn.pending_requests.get(&message_id), Some(ResponseRouting::PopulateFileList { tab_id, uri_target: None }) if *tab_id == tab.id)
            );
            assert_eq!(
                conn.files_management.tabs[0].search_query.as_deref(),
                Some("target")
            );
        }
    }

    #[test]
    fn open_directory_still_enters_it_without_a_scroll_target() {
        let (mut app, mut rx) = search_app();
        drop(app.handle_file_search_result_open(result("/docs/folder", true)));
        let (_, message) = rx.try_recv().expect("file-list request");
        assert!(
            matches!(message, ClientMessage::FileList { path, root: true, .. } if path == "docs/folder")
        );
        let tab = app.connections[&1].files_management.active_tab();
        assert_eq!(tab.current_path, "docs/folder");
        assert!(tab.scroll_target.is_none());
    }

    #[test]
    fn delayed_listing_keeps_target_until_its_tab_is_shown_and_scrolled() {
        let (mut app, mut rx) = search_app();
        drop(app.handle_file_search_result_open(result("/docs/target.txt", false)));
        let (message_id, _) = rx.try_recv().expect("file-list request");
        let tab = app.connections[&1].files_management.active_tab();
        let tab_id = tab.id;
        let target_id = tab.scroll_target.as_ref().expect("target").id.clone();
        let search_tab_id = app.connections[&1].files_management.tabs[0].id;
        drop(app.handle_file_tab_switch(search_tab_id));
        assert_eq!(
            app.handle_server_message_received(
                1,
                message_id,
                listing(&["z.txt", "target.txt", "a.txt"]),
                None,
            )
            .units(),
            0
        );
        assert_eq!(
            app.handle_file_scroll_to_target(tab_id, target_id.clone())
                .units(),
            0
        );

        drop(app.handle_file_tab_switch(tab_id));
        let target_id = app.connections[&1]
            .files_management
            .active_tab()
            .scroll_target
            .as_ref()
            .expect("current target")
            .id
            .clone();
        assert!(
            app.handle_file_scroll_to_target(tab_id, target_id.clone())
                .units()
                > 0
        );
        let tab = app.connections[&1].files_management.active_tab();
        assert_eq!(
            tab.sorted_entries.as_ref().expect("sorted entries")[1].name,
            "target.txt"
        );
        drop(app.handle_file_scroll_completed(
            1,
            tab_id,
            target_id.clone(),
            FileScrollOutcome::ViewUnavailable,
        ));
        let retry_id = app.connections[&1]
            .files_management
            .active_tab()
            .scroll_target
            .as_ref()
            .expect("retry target")
            .id
            .clone();
        assert_ne!(retry_id, target_id);
        drop(app.handle_file_scroll_completed(
            1,
            tab_id,
            target_id.clone(),
            FileScrollOutcome::Scrolled(0.0),
        ));
        assert!(
            app.connections[&1]
                .files_management
                .active_tab()
                .scroll_target
                .is_some()
        );
        drop(app.handle_file_scroll_completed(
            1,
            tab_id,
            retry_id,
            FileScrollOutcome::Scrolled(0.0),
        ));
        assert!(
            app.connections[&1]
                .files_management
                .active_tab()
                .scroll_target
                .is_none()
        );
        assert_eq!(
            app.handle_file_scroll_to_target(tab_id, target_id).units(),
            0
        );
    }

    #[test]
    fn pending_scroll_ignores_other_panels_and_connections() {
        let (mut app, mut rx) = search_app();
        drop(app.handle_file_search_result_open(result("/docs/target.txt", false)));
        let (message_id, _) = rx.try_recv().expect("file-list request");
        drop(app.handle_server_message_received(1, message_id, listing(&["target.txt"]), None));
        let tab = app.connections[&1].files_management.active_tab();
        let tab_id = tab.id;
        let target_id = tab.scroll_target.as_ref().expect("target").id.clone();
        app.set_active_panel(ActivePanel::None);
        assert_eq!(
            app.handle_file_scroll_to_target(tab_id, target_id.clone())
                .units(),
            0
        );
        app.set_active_panel(ActivePanel::Files);
        let (conn, _rx) = test_connection_with_receiver(2);
        app.connections.insert(2, conn);
        app.active_connection = Some(2);
        app.set_active_panel(ActivePanel::Files);
        assert_eq!(
            app.handle_file_scroll_to_target(tab_id, target_id.clone())
                .units(),
            0
        );
        app.active_connection = Some(1);
        assert!(app.handle_file_scroll_to_target(tab_id, target_id).units() > 0);
    }

    #[tokio::test]
    async fn missing_search_result_clears_target_and_reports_not_found() {
        let (mut app, mut rx) = search_app();
        drop(app.handle_file_search_result_open(result("/docs/target.txt", false)));
        let (message_id, _) = rx.try_recv().expect("file-list request");
        drop(app.handle_server_message_received(1, message_id, listing(&["other.txt"]), None));
        let tab = app.connections[&1].files_management.active_tab();
        assert!(tab.scroll_target.is_none());
        assert_eq!(
            tab.error.as_deref(),
            Some(t_args("files-not-found", &[("name", "target.txt")]).as_str())
        );
        let error = tab.error.clone().expect("missing-file error");
        let mut renderer = iced::Renderer::new(Font::DEFAULT, Pixels(16.0), Some("tiny-skia"))
            .await
            .expect("headless software renderer");
        let mut capture = CaptureScroll::default();
        let (_, messages) =
            draw_files_with_operation(&app, Cache::new(), &mut renderer, Some(&mut capture));
        let viewport = capture.bounds.expect("listing viewport");
        let banner = capture
            .texts
            .iter()
            .find(|(text, _)| text == &error)
            .expect("rendered error banner")
            .1;
        let row = capture
            .texts
            .iter()
            .find(|(text, _)| text == "other.txt")
            .expect("remaining file is rendered")
            .1;
        assert!(
            banner.y + banner.height <= viewport.y,
            "banner is above the listing"
        );
        assert!(
            row.y >= viewport.y && row.y + row.height <= viewport.y + viewport.height,
            "listing remains visible below the banner"
        );
        assert!(
            !messages
                .iter()
                .any(|message| matches!(message, Message::FileScrollToTarget { .. }))
        );
    }

    #[tokio::test]
    async fn unreachable_row_stops_reveal_without_retrying() {
        let (mut app, mut rx) = search_app();
        drop(app.handle_file_search_result_open(result("/docs/target.txt", false)));
        let (request, _) = rx.try_recv().expect("listing request");
        drop(app.handle_server_message_received(
            1,
            request,
            listing(&["other.txt", "target.txt"]),
            None,
        ));
        let tab = app
            .connections
            .get_mut(&1)
            .expect("connection")
            .files_management
            .active_tab_mut();
        // Simulate a view inconsistency: the server listed the file, but its
        // rendered table has no target row. This is not evidence of deletion.
        tab.sorted_entries
            .as_mut()
            .expect("sorted entries")
            .retain(|entry| entry.name != "target.txt");
        let tab_id = tab.id;
        let target_id = tab
            .scroll_target
            .as_ref()
            .expect("pending target")
            .id
            .clone();
        let mut renderer = iced::Renderer::new(Font::DEFAULT, Pixels(16.0), Some("tiny-skia"))
            .await
            .expect("headless software renderer");
        let (cache, messages) = draw_files(&app, Cache::new(), &mut renderer);
        let reveal = app.update(reveal_message(messages));
        let (mut cache, completion) =
            run_reveal_operation(&app, cache, &mut renderer, reveal).await;
        assert!(matches!(
            completion,
            Message::FileScrollCompleted {
                outcome: FileScrollOutcome::TargetMissing,
                ..
            }
        ));
        drop(app.update(completion));
        drop(app.handle_file_scroll_completed(
            1,
            tab_id,
            target_id,
            FileScrollOutcome::Scrolled(240.0),
        ));
        let tab = app.connections[&1].files_management.active_tab();
        assert!(tab.scroll_target.is_none());
        assert!(
            tab.error.is_none(),
            "do not report a filesystem error for an unreachable widget"
        );
        assert_eq!(tab.scroll_offset, 0.0);
        for _ in 0..3 {
            let (next_cache, messages) = draw_files(&app, cache, &mut renderer);
            cache = next_cache;
            assert!(
                !messages
                    .iter()
                    .any(|message| matches!(message, Message::FileScrollToTarget { .. })),
                "missing rows must not start a redraw retry loop"
            );
        }
        assert!(rx.try_recv().is_err(), "no extra listing requests");
    }

    #[tokio::test]
    async fn unavailable_view_defers_reveal_until_dialog_closes() {
        let (mut app, mut rx) = search_app();
        drop(app.handle_file_search_result_open(result("/docs/target.txt", false)));
        let (request, _) = rx.try_recv().expect("listing request");
        drop(app.handle_server_message_received(1, request, listing(&["target.txt"]), None));
        let mut renderer = iced::Renderer::new(Font::DEFAULT, Pixels(16.0), Some("tiny-skia"))
            .await
            .expect("headless software renderer");
        let (cache, messages) = draw_files(&app, Cache::new(), &mut renderer);
        let reveal = app.update(reveal_message(messages));
        app.connections
            .get_mut(&1)
            .expect("connection")
            .files_management
            .active_tab_mut()
            .open_new_directory_dialog();
        let (cache, completion) = run_reveal_operation(&app, cache, &mut renderer, reveal).await;
        assert!(matches!(
            completion,
            Message::FileScrollCompleted {
                outcome: FileScrollOutcome::ViewUnavailable,
                ..
            }
        ));
        drop(app.update(completion));
        let tab = app
            .connections
            .get_mut(&1)
            .expect("connection")
            .files_management
            .active_tab_mut();
        assert!(
            tab.scroll_target.is_some(),
            "temporary unmount is not a missing row"
        );
        tab.close_new_directory_dialog();
        let (cache, messages) = draw_files(&app, cache, &mut renderer);
        let reveal = app.update(reveal_message(messages));
        let (_, completion) = run_reveal_operation(&app, cache, &mut renderer, reveal).await;
        assert!(matches!(
            completion,
            Message::FileScrollCompleted {
                outcome: FileScrollOutcome::Scrolled(_),
                ..
            }
        ));
        drop(app.update(completion));
        assert!(
            app.connections[&1]
                .files_management
                .active_tab()
                .scroll_target
                .is_none()
        );
    }

    #[tokio::test]
    async fn deletion_during_pending_reveal_refresh_ignores_late_completion() {
        let mut renderer = iced::Renderer::new(Font::DEFAULT, Pixels(16.0), Some("tiny-skia"))
            .await
            .expect("headless software renderer");
        for render_loading in [false, true] {
            let (mut app, mut rx) = search_app();
            drop(app.handle_file_search_result_open(result("/docs/target.txt", false)));
            let (request, _) = rx.try_recv().expect("listing request");
            drop(app.handle_server_message_received(
                1,
                request,
                listing(&["other.txt", "target.txt"]),
                None,
            ));
            let (cache, messages) = draw_files(&app, Cache::new(), &mut renderer);
            let reveal = app.update(reveal_message(messages));
            let (mut cache, completion) =
                run_reveal_operation(&app, cache, &mut renderer, reveal).await;
            assert!(matches!(
                completion,
                Message::FileScrollCompleted {
                    outcome: FileScrollOutcome::Scrolled(_),
                    ..
                }
            ));
            drop(app.handle_file_refresh());
            if render_loading {
                (cache, _) = draw_files(&app, cache, &mut renderer);
            }
            let (request, _) = rx.try_recv().expect("refresh request");
            drop(app.handle_server_message_received(1, request, listing(&["other.txt"]), None));
            drop(app.update(completion));
            let tab = app.connections[&1].files_management.active_tab();
            assert!(tab.scroll_target.is_none());
            assert_eq!(tab.scroll_offset, 0.0);
            assert_eq!(
                tab.error.as_deref(),
                Some(t_args("files-not-found", &[("name", "target.txt")]).as_str())
            );
            for _ in 0..3 {
                let (next_cache, messages) = draw_files(&app, cache, &mut renderer);
                cache = next_cache;
                assert!(
                    !messages
                        .iter()
                        .any(|message| matches!(message, Message::FileScrollToTarget { .. }))
                );
            }
            assert!(
                rx.try_recv().is_err(),
                "deletion does not cause extra requests"
            );
        }
    }

    #[test]
    fn failed_listing_clears_pending_reveal_and_ignores_late_completion() {
        let (mut app, mut rx) = search_app();
        drop(app.handle_file_search_result_open(result("/docs/target.txt", false)));
        let (request, _) = rx.try_recv().expect("listing request");
        drop(app.handle_server_message_received(1, request, listing(&["target.txt"]), None));
        drop(app.handle_file_refresh());
        let (request, _) = rx.try_recv().expect("refresh request");
        let tab = app.connections[&1].files_management.active_tab();
        assert!(
            tab.sorted_entries.is_some(),
            "refresh retains the previous rendered listing"
        );
        let tab_id = tab.id;
        let target_id = tab
            .scroll_target
            .as_ref()
            .expect("pending target")
            .id
            .clone();
        drop(app.handle_server_message_received(
            1,
            request,
            ServerMessage::FileListResponse {
                success: false,
                error: Some("Directory no longer exists".to_string()),
                path: None,
                entries: None,
                can_upload: false,
                dropbox_owner: None,
            },
            None,
        ));
        for outcome in [
            FileScrollOutcome::ViewUnavailable,
            FileScrollOutcome::TargetMissing,
            FileScrollOutcome::Scrolled(240.0),
        ] {
            drop(app.handle_file_scroll_completed(1, tab_id, target_id.clone(), outcome));
        }
        let tab = app.connections[&1].files_management.active_tab();
        assert!(tab.scroll_target.is_none());
        assert!(tab.entries.is_none());
        assert!(tab.sorted_entries.is_none());
        assert_eq!(tab.error.as_deref(), Some("Directory no longer exists"));
        assert_eq!(tab.scroll_offset, 0.0);
    }

    #[tokio::test]
    async fn wheel_position_survives_dialog_and_drop_overlay_remounts() {
        let (mut app, mut rx) = search_app();
        drop(app.handle_file_search_result_open(result("/docs", true)));
        let (request, _) = rx.try_recv().expect("listing request");
        let names: Vec<_> = (0..80).map(|i| format!("file-{i:03}.txt")).collect();
        drop(app.handle_server_message_received(
            1,
            request,
            listing(&names.iter().map(String::as_str).collect::<Vec<_>>()),
            None,
        ));
        let conn = app.connections.get_mut(&1).expect("connection");
        conn.is_admin = true;
        conn.files_management
            .active_tab_mut()
            .current_dir_can_upload = true;
        let mut renderer = iced::Renderer::new(Font::DEFAULT, Pixels(16.0), Some("tiny-skia"))
            .await
            .expect("headless software renderer");
        let mut capture = CaptureScroll::default();
        let (cache, messages) =
            draw_files_with_operation(&app, Cache::new(), &mut renderer, Some(&mut capture));
        for message in messages {
            drop(app.update(message));
        }
        let cursor = mouse::Cursor::Available(capture.bounds.expect("listing viewport").center());
        let wheel = Event::Mouse(mouse::Event::WheelScrolled {
            delta: mouse::ScrollDelta::Pixels { x: 0.0, y: -240.0 },
        });
        let (mut cache, messages) = draw_files_with_events(
            &app,
            cache,
            &mut renderer,
            None,
            std::slice::from_ref(&wheel),
            cursor,
        );
        assert!(
            messages.iter().any(
                |message| matches!(message, Message::FileScrolled { offset, .. } if *offset > 0.0)
            ),
            "wheel input must publish the viewport"
        );
        for message in messages {
            drop(app.update(message));
        }
        let saved = app.connections[&1]
            .files_management
            .active_tab()
            .scroll_offset;
        assert!(saved > 0.0);

        app.connections
            .get_mut(&1)
            .expect("connection")
            .files_management
            .active_tab_mut()
            .open_new_directory_dialog();
        let mut capture = CaptureScroll {
            id: Some(
                app.connections[&1]
                    .files_management
                    .active_tab()
                    .scroll_id
                    .clone(),
            ),
            ..CaptureScroll::default()
        };
        (cache, _) = draw_files_with_operation(&app, cache, &mut renderer, Some(&mut capture));
        assert!(
            capture.bounds.is_none(),
            "dialog replaces the file viewport"
        );
        app.connections
            .get_mut(&1)
            .expect("connection")
            .files_management
            .active_tab_mut()
            .close_new_directory_dialog();

        for dragging in [false, true, false] {
            if dragging {
                drop(app.handle_file_drag_hovered());
            } else {
                drop(app.handle_file_drag_left());
            }
            assert!(
                app.can_accept_file_drop(),
                "fixture supports the upload overlay"
            );
            let mut capture = CaptureScroll::default();
            let (next_cache, messages) =
                draw_files_with_operation(&app, cache, &mut renderer, Some(&mut capture));
            cache = next_cache;
            assert_eq!(
                capture.offset,
                Some(saved.round()),
                "restoration precedes viewport notifications"
            );
            assert_eq!(
                capture
                    .texts
                    .iter()
                    .any(|(text, _)| text == &t("drop-to-upload")),
                dragging
            );
            for message in messages {
                drop(app.update(message));
            }
            assert_eq!(
                app.connections[&1]
                    .files_management
                    .active_tab()
                    .scroll_offset,
                saved
            );
        }

        let (_, messages) =
            draw_files_with_events(&app, cache, &mut renderer, None, &[wheel], cursor);
        for message in messages {
            drop(app.update(message));
        }
        assert!(
            app.connections[&1]
                .files_management
                .active_tab()
                .scroll_offset
                > saved,
            "restoration must not override subsequent wheel input"
        );
    }

    #[test]
    fn directory_changes_discard_pending_scroll() {
        for change in [
            |tab: &mut FileTab| tab.navigate_to("elsewhere".to_string()),
            |tab: &mut FileTab| tab.navigate_home(),
            |tab: &mut FileTab| tab.toggle_root(),
            |tab: &mut FileTab| tab.navigate_up(),
        ] {
            let (mut app, _rx) = search_app();
            drop(app.handle_file_search_result_open(result("/docs/target.txt", false)));
            let tab = app
                .connections
                .get_mut(&1)
                .expect("connection")
                .files_management
                .active_tab_mut();
            change(tab);
            assert!(tab.scroll_target.is_none());
        }
    }

    #[tokio::test]
    async fn interrupted_reveal_retries_after_reload_with_cached_widgets() {
        let mut renderer = iced::Renderer::new(Font::DEFAULT, Pixels(16.0), Some("tiny-skia"))
            .await
            .expect("headless software renderer");
        for (render_loading_state, completion_during_reload) in [
            (true, None),
            (false, None),
            (true, Some(FileScrollOutcome::Scrolled(0.0))),
            (false, Some(FileScrollOutcome::Scrolled(0.0))),
            (true, Some(FileScrollOutcome::TargetMissing)),
            (false, Some(FileScrollOutcome::TargetMissing)),
            (true, Some(FileScrollOutcome::ViewUnavailable)),
            (false, Some(FileScrollOutcome::ViewUnavailable)),
        ] {
            let (mut app, mut rx) = search_app();
            drop(app.handle_file_search_result_open(result("/docs/target.txt", false)));
            let (message_id, _) = rx.try_recv().expect("file-list request");
            drop(app.handle_server_message_received(1, message_id, listing(&["target.txt"]), None));
            let (mut cache, messages) = draw_files(&app, Cache::new(), &mut renderer);
            let first_reveal = reveal_message(messages);

            if let Some(outcome) = completion_during_reload {
                let tab = app.connections[&1].files_management.active_tab();
                let tab_id = tab.id;
                let target_id = tab.scroll_target.as_ref().expect("target").id.clone();
                assert!(app.update(first_reveal).units() > 0);
                drop(app.handle_file_refresh());
                drop(app.handle_file_scroll_completed(1, tab_id, target_id, outcome));
            } else {
                drop(app.handle_file_refresh());
                assert_eq!(app.update(first_reveal).units(), 0, "defer while loading");
            }
            if render_loading_state {
                let (loading_cache, messages) = draw_files(&app, cache, &mut renderer);
                assert!(messages.is_empty(), "do not reveal while loading");
                cache = loading_cache;
            }
            let (message_id, _) = rx.try_recv().expect("refresh request");
            drop(app.handle_server_message_received(1, message_id, listing(&["target.txt"]), None));
            let (_, messages) = draw_files(&app, cache, &mut renderer);
            assert!(
                app.update(reveal_message(messages)).units() > 0,
                "retry after reload"
            );
        }
    }

    #[tokio::test]
    async fn failed_reveal_retries_with_cached_widgets_and_ignores_old_completion() {
        let (mut app, mut rx) = search_app();
        drop(app.handle_file_search_result_open(result("/docs/target.txt", false)));
        let (message_id, _) = rx.try_recv().expect("file-list request");
        drop(app.handle_server_message_received(1, message_id, listing(&["target.txt"]), None));
        let mut renderer = iced::Renderer::new(Font::DEFAULT, Pixels(16.0), Some("tiny-skia"))
            .await
            .expect("headless software renderer");
        let (cache, messages) = draw_files(&app, Cache::new(), &mut renderer);
        let first_reveal = reveal_message(messages);
        let tab = app.connections[&1].files_management.active_tab();
        let tab_id = tab.id;
        let first_id = tab.scroll_target.as_ref().expect("target").id.clone();
        assert!(app.update(first_reveal).units() > 0);

        drop(app.handle_file_scroll_completed(
            1,
            tab_id,
            first_id.clone(),
            FileScrollOutcome::ViewUnavailable,
        ));
        drop(app.handle_file_scroll_completed(
            1,
            tab_id,
            first_id,
            FileScrollOutcome::Scrolled(0.0),
        ));
        let (cache, messages) = draw_files(&app, cache, &mut renderer);
        assert!(app.update(reveal_message(messages)).units() > 0);
        let retry_id = app.connections[&1]
            .files_management
            .active_tab()
            .scroll_target
            .as_ref()
            .expect("retry target")
            .id
            .clone();
        drop(app.handle_file_scroll_completed(
            1,
            tab_id,
            retry_id,
            FileScrollOutcome::Scrolled(0.0),
        ));
        let (_, messages) = draw_files(&app, cache, &mut renderer);
        assert!(messages.is_empty(), "successful reveal must not repeat");
    }
}
