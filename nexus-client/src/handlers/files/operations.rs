//! File operation handlers (delete, info, rename, clipboard, overwrite, sort)

use iced::Task;
use nexus_common::protocol::ClientMessage;
use nexus_common::validators;

use super::dir_name_error_message;
use crate::NexusApp;
use crate::i18n::t;
use crate::types::{
    ClipboardItem, ClipboardOperation, InputId, Message, PendingRequests, ResponseRouting,
};
use crate::views::files::build_navigate_path;

impl NexusApp {
    pub fn handle_file_delete_clicked(&mut self, path: String) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        let tab = conn.files_management.active_tab_mut();
        tab.pending_delete = Some(path);
        tab.delete_error = None;
        tab.is_delete_submitting = false;

        Task::none()
    }

    /// Handle confirm delete button in modal
    ///
    /// Sends the FileDelete request to the server.
    /// Keeps the dialog open until we get a response (success closes it, error shows in dialog).
    pub fn handle_file_confirm_delete(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        let tab = conn.files_management.active_tab_mut();
        if tab.is_delete_submitting {
            return Task::none();
        }
        let Some(path) = tab.pending_delete.clone() else {
            return Task::none();
        };

        let root = tab.viewing_root;

        // Clear any previous error while the request is in flight
        tab.delete_error = None;
        tab.is_delete_submitting = true;

        let tab_id = conn.files_management.active_tab_id();
        match conn.send(ClientMessage::FileDelete { path, root }) {
            Ok(message_id) => {
                conn.pending_requests
                    .track(message_id, ResponseRouting::FileDeleteResult { tab_id });
            }
            Err(e) => {
                let tab = conn.files_management.active_tab_mut();
                tab.is_delete_submitting = false;
                tab.delete_error = Some(format!("{}: {}", t("err-send-failed"), e));
            }
        }

        Task::none()
    }

    /// Handle cancel delete (close modal)
    pub fn handle_file_cancel_delete(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        let tab = conn.files_management.active_tab_mut();
        tab.pending_delete = None;
        tab.delete_error = None;
        tab.is_delete_submitting = false;

        Task::none()
    }

    // ==================== File Info ====================

    /// Handle info clicked from context menu
    ///
    /// Sends a FileInfo request to the server to get detailed information.
    pub fn handle_file_info_clicked(&mut self, name: String) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        let tab = conn.files_management.active_tab();
        let current_path = &tab.current_path;
        let path = build_navigate_path(current_path, &name);
        let root = tab.viewing_root;

        let tab_id = conn.files_management.active_tab_id();
        match conn.send(ClientMessage::FileInfo { path, root }) {
            Ok(message_id) => {
                conn.pending_requests
                    .track(message_id, ResponseRouting::FileInfoResult { tab_id });
            }
            Err(e) => {
                conn.files_management.active_tab_mut().error =
                    Some(format!("{}: {}", t("err-send-failed"), e));
            }
        }

        Task::none()
    }

    /// Handle close file info dialog
    pub fn handle_close_file_info(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        conn.files_management.active_tab_mut().pending_info = None;

        Task::none()
    }

    // ==================== File Rename ====================

    /// Handle rename clicked from context menu
    ///
    /// Opens a rename dialog with the current name pre-populated.
    pub fn handle_file_rename_clicked(&mut self, name: String) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        // Build the full path for this entry
        let tab = conn.files_management.active_tab_mut();
        let current_path = &tab.current_path;
        let path = build_navigate_path(current_path, &name);

        // Set pending rename to show dialog, pre-populate with actual filesystem name
        // (including any suffixes like [NEXUS-UL] so admin can edit them)
        tab.pending_rename = Some(path);
        tab.rename_name = name;
        tab.rename_error = None;
        tab.is_rename_submitting = false;

        // Focus the name input field
        self.focus_field(InputId::RenameName)
    }

    /// Handle rename name input change
    pub fn handle_file_rename_name_changed(&mut self, name: String) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        // Validate the name in real-time (before storing to avoid clone)
        let validation_error = if name.is_empty() {
            None
        } else {
            validators::validate_dir_name(&name)
                .err()
                .map(dir_name_error_message)
        };

        let tab = conn.files_management.active_tab_mut();
        tab.rename_name = name;
        tab.rename_error = validation_error;

        // Track focus for tray-restore, matching the `*_changed`
        // convention used by every other form's input handlers.
        self.focused_field = InputId::RenameName;
        Task::none()
    }

    /// Handle rename submit button
    pub fn handle_file_rename_submit(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        let tab = conn.files_management.active_tab_mut();
        if tab.is_rename_submitting {
            return Task::none();
        }
        let new_name = &tab.rename_name;

        // Validate before sending
        if new_name.is_empty() {
            tab.rename_error = Some(t("err-dir-name-empty"));
            return Task::none();
        }

        if let Err(e) = validators::validate_dir_name(new_name) {
            tab.rename_error = Some(dir_name_error_message(e));
            return Task::none();
        }

        // Get the path to rename (don't take - keep dialog open until response)
        let Some(path) = tab.pending_rename.clone() else {
            return Task::none();
        };

        let new_name = tab.rename_name.clone();
        let root = tab.viewing_root;

        // Clear any previous error before sending
        let tab = conn.files_management.active_tab_mut();
        tab.rename_error = None;
        tab.is_rename_submitting = true;

        let tab_id = conn.files_management.active_tab_id();
        match conn.send(ClientMessage::FileRename {
            path,
            new_name,
            root,
        }) {
            Ok(message_id) => {
                conn.pending_requests
                    .track(message_id, ResponseRouting::FileRenameResult { tab_id });
            }
            Err(e) => {
                let tab = conn.files_management.active_tab_mut();
                tab.is_rename_submitting = false;
                tab.rename_error = Some(format!("{}: {}", t("err-send-failed"), e));
            }
        }

        Task::none()
    }

    /// Handle rename cancel button (close dialog)
    pub fn handle_file_rename_cancel(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        let tab = conn.files_management.active_tab_mut();
        tab.pending_rename = None;
        tab.rename_name = String::new();
        tab.rename_error = None;
        tab.is_rename_submitting = false;

        Task::none()
    }

    // ==================== Clipboard Operations ====================

    /// Handle cut action from context menu
    ///
    /// Stores the file/directory in clipboard for later move operation.
    pub fn handle_file_cut(&mut self, path: String, name: String) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        let root = conn.files_management.active_tab().viewing_root;
        conn.files_management.clipboard = Some(ClipboardItem {
            path,
            name,
            operation: ClipboardOperation::Cut,
            root,
        });

        Task::none()
    }

    /// Handle copy action from context menu
    ///
    /// Stores the file/directory in clipboard for later copy operation.
    pub fn handle_file_copy_to_clipboard(&mut self, path: String, name: String) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        let root = conn.files_management.active_tab().viewing_root;
        conn.files_management.clipboard = Some(ClipboardItem {
            path,
            name,
            operation: ClipboardOperation::Copy,
            root,
        });

        Task::none()
    }

    /// Handle paste action (to current directory)
    ///
    /// Sends FileMove or FileCopy request based on clipboard operation.
    pub fn handle_file_paste(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        let destination_dir = conn.files_management.active_tab().current_path.clone();
        self.send_paste_request(conn_id, destination_dir)
    }

    /// Handle paste into specific directory (from context menu on folder)
    ///
    /// Sends FileMove or FileCopy request to the specified directory.
    pub fn handle_file_paste_into(&mut self, destination_dir: String) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };

        self.send_paste_request(conn_id, destination_dir)
    }

    /// Send a paste (move/copy) request to the server
    ///
    /// Helper for handle_file_paste and handle_file_paste_into.
    fn send_paste_request(&mut self, conn_id: usize, destination_dir: String) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        if conn.files_management.active_tab().is_paste_submitting {
            return Task::none();
        }

        let Some(clipboard) = conn.files_management.clipboard.clone() else {
            return Task::none();
        };

        let source_root = clipboard.root;
        let destination_root = conn.files_management.active_tab().viewing_root;
        let tab_id = conn.files_management.active_tab_id();
        let is_move = matches!(clipboard.operation, ClipboardOperation::Cut);

        let message = if is_move {
            ClientMessage::FileMove {
                source_path: clipboard.path,
                destination_dir: destination_dir.clone(),
                overwrite: false,
                source_root,
                destination_root,
            }
        } else {
            ClientMessage::FileCopy {
                source_path: clipboard.path,
                destination_dir: destination_dir.clone(),
                overwrite: false,
                source_root,
                destination_root,
            }
        };

        conn.files_management.active_tab_mut().is_paste_submitting = true;

        let Ok(message_id) = conn.send(message) else {
            conn.files_management.active_tab_mut().is_paste_submitting = false;
            return Task::none();
        };

        let routing = if is_move {
            ResponseRouting::FileMoveResult {
                tab_id,
                destination_dir,
            }
        } else {
            ResponseRouting::FileCopyResult {
                tab_id,
                destination_dir,
            }
        };
        conn.pending_requests.track(message_id, routing);

        Task::none()
    }

    /// Handle clear clipboard action (Escape key)
    pub fn handle_file_clear_clipboard(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        conn.files_management.clipboard = None;

        Task::none()
    }

    /// Handle overwrite confirm button in dialog
    ///
    /// Resends the move/copy request with overwrite: true.
    pub fn handle_file_overwrite_confirm(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        if conn.files_management.active_tab().is_paste_submitting {
            return Task::none();
        }

        let Some(pending) = conn
            .files_management
            .active_tab_mut()
            .pending_overwrite
            .take()
        else {
            return Task::none();
        };

        // Clone destination_dir for routing before moving into message
        let destination_dir_for_routing = pending.destination_dir.clone();

        let message = if pending.is_move {
            ClientMessage::FileMove {
                source_path: pending.source_path,
                destination_dir: pending.destination_dir,
                overwrite: true,
                source_root: pending.source_root,
                destination_root: pending.destination_root,
            }
        } else {
            ClientMessage::FileCopy {
                source_path: pending.source_path,
                destination_dir: pending.destination_dir,
                overwrite: true,
                source_root: pending.source_root,
                destination_root: pending.destination_root,
            }
        };

        conn.files_management.active_tab_mut().is_paste_submitting = true;

        let Ok(message_id) = conn.send(message) else {
            conn.files_management.active_tab_mut().is_paste_submitting = false;
            return Task::none();
        };

        let tab_id = conn.files_management.active_tab_id();
        let routing = if pending.is_move {
            ResponseRouting::FileMoveResult {
                tab_id,
                destination_dir: destination_dir_for_routing,
            }
        } else {
            ResponseRouting::FileCopyResult {
                tab_id,
                destination_dir: destination_dir_for_routing,
            }
        };
        conn.pending_requests.track(message_id, routing);

        Task::none()
    }

    /// Handle overwrite cancel button in dialog
    pub fn handle_file_overwrite_cancel(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        let tab = conn.files_management.active_tab_mut();
        tab.pending_overwrite = None;
        tab.is_paste_submitting = false;

        Task::none()
    }

    /// Handle sort by column click
    ///
    /// Clicking the active column toggles ascending/descending.
    /// Clicking a different column switches to that column (ascending).
    pub fn handle_file_sort_by(&mut self, column: crate::types::FileSortColumn) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        let tab = conn.files_management.active_tab_mut();
        if tab.sort_column == column {
            // Toggle direction
            tab.sort_ascending = !tab.sort_ascending;
        } else {
            // Switch to new column, ascending
            tab.sort_column = column;
            tab.sort_ascending = true;
        }

        // Rebuild sorted entries cache
        tab.update_sorted_entries();

        Task::none()
    }
}
