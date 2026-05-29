//! File transfer handlers (share, download, upload, drag-and-drop)

use iced::Task;
use iced_toasts::{ToastLevel, toast};

use super::sanitize_filename;
use crate::NexusApp;
use crate::constants::ERR_PATH_EMPTY;
use crate::i18n::{t, t_args};
use crate::types::{ActivePanel, FilesManagementState, Message};
use crate::uri::url_encode_path;
use crate::views::constants::{PERMISSION_FILE_UPLOAD, PERMISSION_FILE_UPLOAD_ANYWHERE};

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

        // Extract filename from remote path
        // For single files: use the filename
        // For directories: use the directory name as the containing folder
        // For root path ("/") downloads: use server name as the folder
        let trimmed_path = remote_path.trim_matches('/').to_string();
        let (local_path, toast_filename) = if is_directory && trimmed_path.is_empty() {
            // Root directory download - use server name as folder
            // Sanitize server name to be filesystem-safe, fall back to address
            let safe_name = sanitize_filename(
                &conn.connection_info.server_name,
                &conn.connection_info.address,
            );
            let path = std::path::PathBuf::from(&download_dir).join(&safe_name);
            (path, safe_name)
        } else {
            // Extract last path component for the local filename/folder
            // trimmed_path is guaranteed non-empty here, so rsplit will return a non-empty value
            let filename = trimmed_path
                .rsplit('/')
                .next()
                .expect(ERR_PATH_EMPTY)
                .to_string();
            let path = std::path::PathBuf::from(&download_dir).join(&filename);
            (path, filename)
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
