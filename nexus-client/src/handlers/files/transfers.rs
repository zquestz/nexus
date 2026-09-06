//! File transfer handlers (share, download, upload, drag-and-drop)

use std::path::{Path, PathBuf};

use iced::Task;
use iced_toasts::{ToastLevel, toast};

use super::sanitize_filename;
use crate::NexusApp;
use crate::i18n::{t, t_args};
use crate::transfers::is_safe_download_name;
use crate::types::{ActivePanel, FilesManagementState, Message};
use crate::uri::url_encode_path;
use crate::views::constants::{PERMISSION_FILE_UPLOAD, PERMISSION_FILE_UPLOAD_ANYWHERE};

/// Build a lexical child of the configured directory without touching the filesystem.
/// The configured directory and local symlinks remain user-controlled, trusted paths.
fn download_destination(
    download_dir: &Path,
    remote_path: &str,
    is_directory: bool,
    server_name: &str,
    server_address: &str,
) -> Option<(PathBuf, String)> {
    let trimmed_path = remote_path.trim_matches('/');
    let name = if is_directory && trimmed_path.is_empty() {
        sanitize_filename(server_name, server_address)
    } else {
        trimmed_path
            .rsplit_once('/')
            .map_or(trimmed_path, |(_, name)| name)
            .to_string()
    };

    // Validate before joining so native roots, prefixes, and traversal cannot
    // replace the configured directory or escape it.
    if !is_safe_download_name(&name) {
        return None;
    }

    Some((download_dir.join(&name), name))
}

impl NexusApp {
    // ==================== Share ====================

    /// Handle share request - copies nexus:// URL to clipboard
    ///
    /// Builds a deep link URL with the current connection info and file path,
    /// then copies it to the system clipboard. Folder type suffixes are stripped
    /// from the path since the server resolves paths without them.
    pub fn handle_file_share(&mut self, path: String) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get(&conn_id) else {
            return Task::none();
        };

        // Build the root URI (prefers public_address when advertised, with
        // IPv6 bracketing and default-port omission handled centrally).
        let info = &conn.connection_info;
        let root =
            crate::uri::build_share_uri(conn.public_address.as_deref(), &info.address, info.port);

        // Strip folder type suffixes from each path segment
        // Server resolves paths without suffixes (e.g., "uploads" -> "uploads [NEXUS-UL]")
        let clean_path = path
            .split('/')
            .map(FilesManagementState::display_name)
            .collect::<Vec<_>>()
            .join("/");

        let url = format!("{}/files/{}", root, url_encode_path(&clean_path));

        // Copy to clipboard, then show toast feedback
        let toast_text = t("toast-link-copied");
        iced::clipboard::write(url).chain(Task::done(Message::ShowToast(toast_text)))
    }

    // ==================== Downloads ====================

    /// Handle file download request (single file)
    ///
    /// Creates a new transfer in the transfer manager and queues it for download.
    pub fn handle_file_download(&mut self, path: String) -> Task<Message> {
        self.queue_download(path, false)
    }

    /// Handle directory download request (recursive)
    ///
    /// Creates a new transfer in the transfer manager and queues it for download.
    pub fn handle_file_download_all(&mut self, path: String) -> Task<Message> {
        self.queue_download(path, true)
    }

    /// Queue a download transfer
    ///
    /// Creates a Transfer with Queued status and adds it to the transfer manager.
    /// Uses the current tab's viewing_root for the remote root context.
    fn queue_download(&mut self, remote_path: String, is_directory: bool) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get(&conn_id) else {
            return Task::none();
        };

        // Get the current viewing mode (root or user area)
        let remote_root = conn.files_management.active_tab().viewing_root;

        self.queue_download_with_root(remote_path, is_directory, remote_root)
    }

    /// Queue a download transfer with explicit root context
    ///
    /// This variant is used when the root context is known explicitly,
    /// such as when downloading from search results where the search
    /// may have been performed with a different root setting than the
    /// current tab's browsing mode.
    pub(crate) fn queue_download_with_root(
        &mut self,
        remote_path: String,
        is_directory: bool,
        remote_root: bool,
    ) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get(&conn_id) else {
            return Task::none();
        };

        // Build local path from download directory + remote filename
        let download_dir = self
            .config
            .settings
            .download_path
            .clone()
            .or_else(crate::config::settings::default_download_path)
            .unwrap_or_else(|| ".".to_string());

        let Some((local_path, toast_filename)) = download_destination(
            Path::new(&download_dir),
            &remote_path,
            is_directory,
            &conn.connection_info.server_name,
            &conn.connection_info.address,
        ) else {
            self.toasts
                .push(toast(&t("toast-download-invalid-name")).level(ToastLevel::Error));
            return Task::none();
        };
        // Create the transfer
        let queue_position = self.transfer_manager.next_queue_position();
        let transfer = crate::transfers::Transfer::new_download(
            conn.connection_info.clone(),
            remote_path,
            remote_root,
            is_directory,
            local_path,
            conn.bookmark_id,
            queue_position,
        );

        // Add to transfer manager
        self.transfer_manager.add(transfer);

        // Save transfers to disk
        let save_task = match self.transfer_manager.save() {
            Ok(()) => Task::none(),
            Err(e) => self.add_background_error_message(conn_id, e),
        };

        // Show toast feedback
        let toast_text = if self.config.settings.queue_transfers {
            t_args("toast-download-queued", &[("filename", &toast_filename)])
        } else {
            t_args("toast-download-started", &[("filename", &toast_filename)])
        };
        self.toasts
            .push(toast(&toast_text).level(ToastLevel::Success));

        save_task
    }

    // ==================== Uploads ====================

    /// Handle upload request - opens file picker for multiple files
    ///
    /// The destination path is where files will be uploaded to on the server.
    ///
    /// Note: The `rfd` crate's `pick_files()` only allows selecting files, not folders.
    /// There's no cross-platform way to select both files and folders in a single dialog.
    /// Directory upload is fully supported in the executor - we just need a separate
    /// folder picker trigger (e.g., "Upload Folder" menu item or drag-and-drop) to use it.
    pub fn handle_file_upload(&mut self, destination: String) -> Task<Message> {
        let destination_clone = destination.clone();
        Task::perform(
            async move {
                let handle = rfd::AsyncFileDialog::new()
                    .set_title(t("file-picker-upload-title"))
                    .pick_files()
                    .await;

                match handle {
                    Some(files) => {
                        let paths: Vec<std::path::PathBuf> =
                            files.into_iter().map(|f| f.path().to_path_buf()).collect();
                        Message::FileUploadSelected(destination_clone, paths)
                    }
                    None => {
                        // User cancelled - no-op, keeps panel open
                        Message::FileUploadCancelled
                    }
                }
            },
            |msg| msg,
        )
    }

    /// Handle file picker result - queue uploads
    pub fn handle_file_upload_selected(
        &mut self,
        destination: String,
        paths: Vec<std::path::PathBuf>,
    ) -> Task<Message> {
        if paths.is_empty() {
            return Task::none();
        }

        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get(&conn_id) else {
            return Task::none();
        };

        // Get the current viewing mode (root or user area)
        let remote_root = conn.files_management.active_tab().viewing_root;

        let is_queued = self.config.settings.queue_transfers;
        let upload_count = paths.len();
        let first_filename = paths
            .first()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("file")
            .to_string();

        // Queue each selected file/directory as a separate upload
        for local_path in paths {
            let is_directory = local_path.is_dir();

            // For directory uploads, append the directory name to the destination
            // so the server creates the directory structure (e.g., "/Uploads/MyFolder/")
            let remote_path = if is_directory {
                if let Some(dir_name) = local_path.file_name().and_then(|n| n.to_str()) {
                    if destination.is_empty() || destination == "/" {
                        format!("/{dir_name}")
                    } else {
                        format!("{}/{}", destination.trim_end_matches('/'), dir_name)
                    }
                } else {
                    destination.clone()
                }
            } else {
                destination.clone()
            };

            // Create the transfer
            let queue_position = self.transfer_manager.next_queue_position();
            let transfer = crate::transfers::Transfer::new_upload(
                conn.connection_info.clone(),
                remote_path,
                remote_root,
                is_directory,
                local_path,
                conn.bookmark_id,
                queue_position,
            );

            // Add to transfer manager
            self.transfer_manager.add(transfer);
        }

        // Save transfers to disk
        let save_task = match self.transfer_manager.save() {
            Ok(()) => Task::none(),
            Err(e) => self.add_background_error_message(conn_id, e),
        };

        // Show toast feedback (single file: show name, multiple: show count)
        let toast_text = if upload_count == 1 {
            if is_queued {
                t_args("toast-upload-queued", &[("filename", &first_filename)])
            } else {
                t_args("toast-upload-started", &[("filename", &first_filename)])
            }
        } else {
            let count_str = upload_count.to_string();
            if is_queued {
                t_args("toast-uploads-queued", &[("count", &count_str)])
            } else {
                t_args("toast-uploads-started", &[("count", &count_str)])
            }
        };
        self.toasts
            .push(toast(&toast_text).level(ToastLevel::Success));

        save_task
    }

    // ==================== Drag and Drop ====================

    /// Whether a drag-and-drop file upload would be accepted right now.
    ///
    /// Requires: Files panel active, any upload permission, and either the
    /// current folder is an upload/dropbox type OR the user has the
    /// `file_upload_anywhere` bypass.
    pub fn can_accept_file_drop(&self) -> bool {
        let Some(conn_id) = self.active_connection else {
            return false;
        };
        let Some(conn) = self.connections.get(&conn_id) else {
            return false;
        };

        // Must be in Files panel
        if conn.active_panel != ActivePanel::Files {
            return false;
        }

        // Must have upload capability — either permission works.
        if !conn.has_any_permission(&[PERMISSION_FILE_UPLOAD, PERMISSION_FILE_UPLOAD_ANYWHERE]) {
            return false;
        }

        // Folder must allow uploads, OR the user's file_upload_anywhere bypass applies.
        conn.files_management.active_tab().current_dir_can_upload
            || conn.has_permission(PERMISSION_FILE_UPLOAD_ANYWHERE)
    }

    /// Handle file being dragged over window
    ///
    /// Just sets the dragging flag - visual feedback is handled in the view.
    pub fn handle_file_drag_hovered(&mut self) -> Task<Message> {
        self.dragging_files = true;
        Task::none()
    }

    /// Handle file dropped on window
    ///
    /// If we're in a valid upload context (Files panel active, uploadable folder,
    /// file_upload permission), queue the dropped file/folder for upload.
    pub fn handle_file_drag_dropped(&mut self, path: std::path::PathBuf) -> Task<Message> {
        // Clear dragging state
        self.dragging_files = false;

        // Check if we can accept the drop
        if !self.can_accept_file_drop() {
            return Task::none();
        }

        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get(&conn_id) else {
            return Task::none();
        };

        // Get upload destination (current directory)
        let destination = conn.files_management.active_tab().current_path.clone();
        let remote_root = conn.files_management.active_tab().viewing_root;
        let is_directory = path.is_dir();
        let path_filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string());

        // For directory uploads, append the directory name to the destination
        // so the server creates the directory structure (e.g., "/Uploads/MyFolder/")
        let remote_path = if is_directory {
            if let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) {
                if destination.is_empty() || destination == "/" {
                    format!("/{dir_name}")
                } else {
                    format!("{}/{}", destination.trim_end_matches('/'), dir_name)
                }
            } else {
                destination.clone()
            }
        } else {
            destination.clone()
        };

        // Create the transfer
        let queue_position = self.transfer_manager.next_queue_position();
        let transfer = crate::transfers::Transfer::new_upload(
            conn.connection_info.clone(),
            remote_path,
            remote_root,
            is_directory,
            path,
            conn.bookmark_id,
            queue_position,
        );

        // Add to transfer manager
        self.transfer_manager.add(transfer);

        // Save transfers to disk
        let save_task = match self.transfer_manager.save() {
            Ok(()) => Task::none(),
            Err(e) => self.add_background_error_message(conn_id, e),
        };

        // Show toast feedback
        let filename = path_filename.as_deref().unwrap_or("file");
        let toast_text = if self.config.settings.queue_transfers {
            t_args("toast-upload-queued", &[("filename", filename)])
        } else {
            t_args("toast-upload-started", &[("filename", filename)])
        };
        self.toasts
            .push(toast(&toast_text).level(ToastLevel::Success));

        save_task
    }

    /// Handle drag leaving window
    pub fn handle_file_drag_left(&mut self) -> Task<Message> {
        self.dragging_files = false;
        Task::none()
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use iced::advanced::widget::Tree;
    use iced::widget::Space;
    use iced_toasts::toast_container;
    use nexus_common::protocol::FileSearchResult;
    use tempfile::TempDir;
    use uuid::Uuid;

    use super::*;
    use crate::testing::support::test_connection_with_receiver;
    use crate::transfers::{Transfer, TransferDirection, TransferManager, TransferStatus};

    fn assert_download_toast(app: &NexusApp, message: &str, level: ToastLevel) {
        let view = app.toasts.view(Space::new());
        let tree = Tree::new(&view);
        assert_eq!(tree.children.len(), 2, "content plus exactly one toast");

        // ToastContainer exposes its messages and levels through Debug, not getters.
        let toasts = format!("{:?}", app.toasts);
        assert!(toasts.contains(&format!("message: {message:?}")));
        assert!(toasts.contains(&format!("level: Some({level:?})")));
    }

    #[test]
    fn download_destinations_are_direct_children_without_renaming() {
        let temp = TempDir::new().unwrap();
        let missing = temp.path().join("not-created");
        for base in [Path::new("."), Path::new("downloads"), missing.as_path()] {
            for (remote, expected) in [
                ("report.txt", "report.txt"),
                ("/Documents/report.txt", "report.txt"),
                ("/Documents/My Files/", "My Files"),
                ("nested/.hidden", ".hidden"),
                ("nested/ leading space.txt", " leading space.txt"),
                ("nested/Uploads [NEXUS-UL]", "Uploads [NEXUS-UL]"),
                ("nested/文件.txt", "文件.txt"),
                ("nested/données.zip", "données.zip"),
            ] {
                for is_directory in [false, true] {
                    let (path, name) =
                        download_destination(base, remote, is_directory, "Server", "example.test")
                            .unwrap();
                    assert_eq!(name, expected);
                    assert_eq!(path, base.join(expected));
                    assert_eq!(path.parent(), Some(base));
                    assert_eq!(path.strip_prefix(base).unwrap().components().count(), 1);
                }
            }
        }
        assert!(!missing.exists(), "queuing must not create the destination");
    }

    #[test]
    fn root_download_destinations_use_safe_generated_labels() {
        let base = Path::new("downloads");
        for root in ["", "/", "///"] {
            for (server_name, address, expected) in [
                ("My Server", "example.test", "My Server"),
                ("Server/Files", "example.test", "Server_Files"),
                ("NUL.txt", "example.test", "_NUL.txt"),
                ("..", "../../outside", ".._.._outside"),
                ("", "2001:db8::1", "2001_db8__1"),
                (".", "C:\\outside", "C__outside"),
                ("", "CON .txt", "_CON .txt"),
                (" . . ", " . . ", "server"),
            ] {
                let (path, name) =
                    download_destination(base, root, true, server_name, address).unwrap();
                assert_eq!(name, expected);
                assert_eq!(path.parent(), Some(base));
                assert_eq!(path.strip_prefix(base).unwrap(), Path::new(expected));
                assert!(is_safe_download_name(&name));
            }
        }
    }

    #[test]
    fn download_destinations_preserve_names_allowed_by_the_current_platform() {
        let temp = TempDir::new().unwrap();
        let missing = temp.path().join("not-created");
        for base in [Path::new("."), Path::new("downloads"), missing.as_path()] {
            for name in [
                "report:final.txt",
                "dir\\file",
                "C:outside",
                "\\\\server\\share",
                "file<name",
                "file>name",
                "file\"name",
                "file|name",
                "file?name",
                "file*name",
                "file.",
                "file ",
                ".. ",
                "...",
                "CON.txt",
                "COM\u{b9}.txt",
                "file\n.txt",
            ] {
                for is_directory in [false, true] {
                    let destination = download_destination(
                        base,
                        &format!("nested/{name}"),
                        is_directory,
                        "Server",
                        "example.test",
                    );
                    if cfg!(windows) {
                        assert!(destination.is_none(), "{name:?}");
                    } else {
                        let (path, actual_name) = destination.unwrap();
                        assert_eq!(actual_name, name);
                        assert_eq!(path, base.join(name));
                        assert_eq!(path.parent(), Some(base));
                        assert_eq!(path.strip_prefix(base).unwrap().components().count(), 1);
                    }
                }
            }
        }
        assert!(!missing.exists(), "queuing must not create the destination");
    }

    #[test]
    fn single_file_downloads_require_a_filename() {
        for remote in ["", "/", "///"] {
            assert!(
                download_destination(Path::new("downloads"), remote, false, "Server", "host")
                    .is_none()
            );
        }
    }

    #[test]
    fn valid_downloads_queue_and_persist_the_destination_and_origin() {
        for is_directory in [false, true] {
            for from_search in [false, true] {
                let temp = TempDir::new().unwrap();
                let download_dir = temp.path().join("downloads");
                let saved_path = temp.path().join("queue/transfers.json");
                let (other_conn, mut other_rx) = test_connection_with_receiver(1);
                let (mut conn, mut rx) = test_connection_with_receiver(2);
                conn.active_panel = ActivePanel::Files;
                conn.bookmark_id = Some(Uuid::new_v4());
                conn.connection_info.server_name = "Music Server".into();
                conn.connection_info.address = "music.example.test".into();
                conn.connection_info.port = 7500;
                conn.connection_info.transfer_port = 7501;
                conn.connection_info.certificate_fingerprint = "test-fingerprint".into();
                conn.connection_info.username = "listener".into();
                conn.connection_info.password = "test-password".into();
                conn.connection_info.nickname = "Listener".into();
                conn.files_management.active_tab_mut().viewing_root = true;
                conn.files_management.active_tab_mut().search_viewing_root = false;
                let expected_connection = serde_json::to_value(&conn.connection_info).unwrap();
                let expected_bookmark = conn.bookmark_id;
                let existing = Transfer::new_download(
                    other_conn.connection_info.clone(),
                    "existing/report.txt".into(),
                    false,
                    false,
                    download_dir.join("report.txt"),
                    None,
                    7,
                );
                let existing_id = existing.id;
                let expected_existing = serde_json::to_value(&existing).unwrap();
                let mut app = NexusApp {
                    active_connection: Some(2),
                    transfer_manager: TransferManager::new_for_test(saved_path.clone()),
                    ..NexusApp::default()
                };
                app.config.settings.download_path = Some(download_dir.to_str().unwrap().into());
                app.config.settings.queue_transfers = !from_search;
                app.transfer_manager.add(existing);
                app.transfer_manager.save().unwrap();
                app.connections.insert(1, other_conn);
                app.connections.insert(2, conn);
                let (remote_path, name) = if is_directory {
                    ("artist/album", "album")
                } else {
                    ("artist/album/track.mp3", "track.mp3")
                };

                let task = if from_search {
                    app.handle_file_search_result_download(FileSearchResult {
                        path: format!("/{remote_path}"),
                        name: "different display name".into(),
                        size: 0,
                        modified: 0,
                        is_directory,
                    })
                } else if is_directory {
                    app.handle_file_download_all(remote_path.into())
                } else {
                    app.handle_file_download(remote_path.into())
                };
                drop(task);

                assert_eq!(app.transfer_manager.all().count(), 2);
                let transfer = app
                    .transfer_manager
                    .all()
                    .find(|transfer| transfer.id != existing_id)
                    .expect("valid download must add a transfer");
                assert_eq!(transfer.local_path, download_dir.join(name));
                assert_eq!(transfer.remote_path, remote_path);
                assert_eq!(transfer.remote_root, !from_search);
                assert_eq!(transfer.is_directory, is_directory);
                assert_eq!(transfer.bookmark_id, expected_bookmark);
                assert_eq!(
                    serde_json::to_value(&transfer.connection_info).unwrap(),
                    expected_connection
                );
                assert_eq!(transfer.direction, TransferDirection::Download);
                assert_eq!(transfer.status, TransferStatus::Queued);
                assert_eq!(transfer.queue_position, 8);
                assert_eq!(
                    serde_json::to_value(app.transfer_manager.get(existing_id).unwrap()).unwrap(),
                    expected_existing
                );

                let saved: serde_json::Value =
                    serde_json::from_slice(&fs::read(&saved_path).unwrap()).unwrap();
                let saved_transfers = saved["transfers"].as_array().unwrap();
                assert_eq!(saved_transfers.len(), 2, "new transfer must be saved");
                assert!(saved_transfers.contains(&expected_existing));
                assert!(saved_transfers.contains(&serde_json::to_value(transfer).unwrap()));
                let toast_key = if from_search {
                    "toast-download-started"
                } else {
                    "toast-download-queued"
                };
                assert_download_toast(
                    &app,
                    &t_args(toast_key, &[("filename", name)]),
                    ToastLevel::Success,
                );
                assert!(!download_dir.exists(), "queueing must not create downloads");
                assert!(rx.try_recv().is_err());
                assert!(other_rx.try_recv().is_err());
            }
        }
    }

    #[test]
    fn unsafe_downloads_show_a_toast_without_changing_tabs_queue_or_persistence() {
        let temp = TempDir::new().unwrap();
        let download_dir = temp.path().join("downloads");
        let (mut conn, mut rx) = test_connection_with_receiver(1);
        conn.active_panel = ActivePanel::Files;
        conn.files_management.active_tab_mut().error = Some("Background listing error".into());
        conn.files_management.new_tab();
        conn.files_management.active_tab_mut().error = Some("Active listing error".into());
        conn.files_management.active_tab_mut().search_error = Some("Existing search error".into());
        conn.files_management.active_tab_mut().viewing_root = true;
        conn.files_management.active_tab_mut().search_viewing_root = false;
        let files_before = format!("{:?}", conn.files_management);
        let (mut other_conn, mut other_rx) = test_connection_with_receiver(2);
        other_conn.files_management.active_tab_mut().error = Some("Other connection error".into());
        let other_files_before = format!("{:?}", other_conn.files_management);
        let mut app = NexusApp {
            active_connection: Some(1),
            transfer_manager: TransferManager::new(),
            ..NexusApp::default()
        };
        app.config.settings.download_path = Some(download_dir.to_str().unwrap().into());
        // Existing queue entries, including old unsafe destinations, are not migrated.
        app.transfer_manager.add(Transfer::new_download(
            conn.connection_info.clone(),
            "existing/report.txt".into(),
            true,
            false,
            temp.path().join("legacy/../report.txt"),
            conn.bookmark_id,
            7,
        ));
        app.connections.insert(1, conn);
        app.connections.insert(2, other_conn);
        let before = serde_json::to_value(app.transfer_manager.all().collect::<Vec<_>>()).unwrap();
        let saved_path = TransferManager::transfers_path().unwrap();
        let persisted_before = fs::read(&saved_path).ok();

        let unsafe_paths = [
            ".",
            "..",
            "folder/.",
            "folder/..",
            "folder/..//",
            "folder/file\0.txt",
        ];
        let windows_unsafe_paths = [
            "folder/.. ",
            "folder/C:",
            "folder/C:outside",
            "folder/C:\\outside",
            "folder/\\\\server\\share",
            "folder/\\\\?\\C:\\outside",
            "folder/file:stream",
            "folder/CON.txt",
            "folder/COM1 .txt",
            "folder/name.",
            "folder/name ",
        ];
        for remote in unsafe_paths
            .into_iter()
            .chain(windows_unsafe_paths.into_iter().filter(|_| cfg!(windows)))
        {
            for is_directory in [false, true] {
                for from_search in [false, true] {
                    app.toasts = toast_container(Message::ToastDismiss);
                    let task = if from_search {
                        app.handle_file_search_result_download(FileSearchResult {
                            path: format!("/{remote}"),
                            name: "untrusted display name".into(),
                            size: 0,
                            modified: 0,
                            is_directory,
                        })
                    } else if is_directory {
                        app.handle_file_download_all(remote.into())
                    } else {
                        app.handle_file_download(remote.into())
                    };
                    drop(task);
                    assert_download_toast(
                        &app,
                        &t("toast-download-invalid-name"),
                        ToastLevel::Error,
                    );
                    assert_eq!(
                        format!("{:?}", app.connections[&1].files_management),
                        files_before,
                        "{remote:?}, directory={is_directory}, search={from_search}"
                    );
                    assert_eq!(
                        format!("{:?}", app.connections[&2].files_management),
                        other_files_before
                    );
                    assert_eq!(
                        serde_json::to_value(app.transfer_manager.all().collect::<Vec<_>>())
                            .unwrap(),
                        before,
                        "rejection must preserve existing transfers and their metadata"
                    );
                    assert_eq!(fs::read(&saved_path).ok(), persisted_before);
                    assert!(rx.try_recv().is_err());
                    assert!(other_rx.try_recv().is_err());
                    assert!(!download_dir.exists());
                }
            }
        }
    }
}
