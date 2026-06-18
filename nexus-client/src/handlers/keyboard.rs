//! Keyboard and window event handling

use iced::keyboard::{self, key};
use iced::window;
use iced::{Event, Task};

use nexus_common::protocol::ClientMessage;

use crate::NexusApp;
use crate::types::{
    ActivePanel, BookmarkEditMode, ChatTab, GroupManagementMode, InputId, Message,
    NewsManagementMode, PendingRequests, ResponseRouting, TrackerManagementMode,
    UserManagementMode,
};
use crate::views::constants::PERMISSION_TRACKER_LIST;
use crate::voice::ptt::build_hotkey_string;

impl NexusApp {
    /// Handle keyboard and window events (Tab, Enter, Escape, F5, file drag-and-drop)
    pub fn handle_keyboard_event(&mut self, event: Event) -> Task<Message> {
        // Handle PTT key capture when in settings and capture mode is active
        if let Some(form) = &self.settings_form
            && form.ptt_capturing
        {
            if let Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) = &event {
                // Convert Iced key to our PTT key string format
                let key_string = match key {
                    // Named keys
                    keyboard::Key::Named(named) => match named {
                        key::Named::Space => Some("Space".to_string()),
                        key::Named::Enter => Some("Enter".to_string()),
                        key::Named::Tab => Some("Tab".to_string()),
                        key::Named::Escape => {
                            // Escape cancels capture mode without changing the key
                            if let Some(form) = &mut self.settings_form {
                                form.ptt_capturing = false;
                            }
                            return Task::none();
                        }
                        key::Named::Backspace => Some("Backspace".to_string()),
                        key::Named::Delete => Some("Delete".to_string()),
                        key::Named::Insert => Some("Insert".to_string()),
                        key::Named::Home => Some("Home".to_string()),
                        key::Named::End => Some("End".to_string()),
                        key::Named::PageUp => Some("PageUp".to_string()),
                        key::Named::PageDown => Some("PageDown".to_string()),
                        key::Named::ArrowUp => Some("ArrowUp".to_string()),
                        key::Named::ArrowDown => Some("ArrowDown".to_string()),
                        key::Named::ArrowLeft => Some("ArrowLeft".to_string()),
                        key::Named::ArrowRight => Some("ArrowRight".to_string()),
                        key::Named::F1 => Some("F1".to_string()),
                        key::Named::F2 => Some("F2".to_string()),
                        key::Named::F3 => Some("F3".to_string()),
                        key::Named::F4 => Some("F4".to_string()),
                        key::Named::F5 => Some("F5".to_string()),
                        key::Named::F6 => Some("F6".to_string()),
                        key::Named::F7 => Some("F7".to_string()),
                        key::Named::F8 => Some("F8".to_string()),
                        key::Named::F9 => Some("F9".to_string()),
                        key::Named::F10 => Some("F10".to_string()),
                        key::Named::F11 => Some("F11".to_string()),
                        key::Named::F12 => Some("F12".to_string()),
                        key::Named::F13 => Some("F13".to_string()),
                        key::Named::F14 => Some("F14".to_string()),
                        key::Named::F15 => Some("F15".to_string()),
                        key::Named::F16 => Some("F16".to_string()),
                        key::Named::F17 => Some("F17".to_string()),
                        key::Named::F18 => Some("F18".to_string()),
                        key::Named::F19 => Some("F19".to_string()),
                        key::Named::F20 => Some("F20".to_string()),
                        key::Named::F21 => Some("F21".to_string()),
                        key::Named::F22 => Some("F22".to_string()),
                        key::Named::F23 => Some("F23".to_string()),
                        key::Named::F24 => Some("F24".to_string()),
                        _ => None,
                    },
                    // Character keys
                    keyboard::Key::Character(c) => {
                        let s = c.as_str();
                        match s {
                            // Special characters
                            "`" => Some("`".to_string()),
                            "-" => Some("-".to_string()),
                            "=" => Some("=".to_string()),
                            "[" => Some("[".to_string()),
                            "]" => Some("]".to_string()),
                            "\\" => Some("\\".to_string()),
                            ";" => Some(";".to_string()),
                            "'" => Some("'".to_string()),
                            "," => Some(",".to_string()),
                            "." => Some(".".to_string()),
                            "/" => Some("/".to_string()),
                            // Single character (letter or digit)
                            // Single-byte string (`s.len() == 1`). For
                            // valid UTF-8 that means exactly one ASCII
                            // char, but the `match` here doesn't rely
                            // on that invariant: only the
                            // alphanumeric-ASCII branch uppercases, and
                            // any other shape (including the
                            // never-actually-reachable empty string)
                            // falls through to "emit s verbatim".
                            _ if s.len() == 1 => match s.chars().next() {
                                Some(ch) if ch.is_ascii_alphanumeric() => {
                                    Some(ch.to_ascii_uppercase().to_string())
                                }
                                _ => Some(s.to_string()),
                            },
                            _ => None,
                        }
                    }
                    keyboard::Key::Unidentified => None,
                };

                if let Some(key_str) = key_string {
                    let hotkey_string = build_hotkey_string(modifiers, &key_str);
                    return self.update(Message::AudioPttKeyCaptured(hotkey_string));
                }
            }
            // While capturing, consume all key events to prevent other actions
            if matches!(event, Event::Keyboard(keyboard::Event::KeyPressed { .. })) {
                return Task::none();
            }
        }

        // Auto-away: bump last_activity on key releases only
        // - Excludes ModifiersChanged (fires on window focus, would falsely trigger auto-back)
        // - KeyReleased isn't consumed by PTT capture (which only intercepts KeyPressed)
        if matches!(event, Event::Keyboard(keyboard::Event::KeyReleased { .. })) {
            let now = std::time::Instant::now();
            for conn in self.connections.values_mut() {
                conn.last_activity = now;
            }

            // Auto-back: send UserBack for connections that were auto-awayed
            let auto_away_conn_ids: Vec<usize> = self
                .connections
                .values()
                .filter(|conn| {
                    conn.is_auto_away
                        && !conn
                            .pending_requests
                            .values()
                            .any(|r| matches!(r, ResponseRouting::AutoBackResult))
                })
                .map(|conn| conn.connection_id)
                .collect();

            for conn_id in auto_away_conn_ids {
                if let Some(conn) = self.connections.get_mut(&conn_id) {
                    let msg = ClientMessage::UserBack;
                    if let Ok(message_id) = conn.send(msg) {
                        conn.pending_requests
                            .track(message_id, ResponseRouting::AutoBackResult);
                    }
                }
            }
        }

        // Handle window events (focus, file drag-and-drop)
        if let Event::Window(window_event) = &event {
            match window_event {
                window::Event::Focused => {
                    return self.update(Message::WindowFocused);
                }
                window::Event::Unfocused => {
                    return self.update(Message::WindowUnfocused);
                }
                window::Event::FileHovered(_) => {
                    return self.update(Message::FileDragHovered);
                }
                window::Event::FileDropped(path) => {
                    return self.update(Message::FileDragDropped(path.clone()));
                }
                window::Event::FilesHoveredLeft => {
                    return self.update(Message::FileDragLeft);
                }
                _ => {}
            }
        }

        // Handle F5 for refresh in Files panel
        if let Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(key::Named::F5),
            ..
        }) = event
            && self.active_panel() == ActivePanel::Files
        {
            return self.update(Message::FileRefresh);
        }

        // Handle Cmd/Ctrl+Shift+Tab for previous chat tab (must be before plain Tab check)
        if let Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(key::Named::Tab),
            modifiers,
            ..
        }) = event
            && modifiers.command()
            && modifiers.shift()
        {
            return self.update(Message::PrevChatTab);
        }
        // Handle Cmd/Ctrl+Tab for next chat tab (must be before plain Tab check)
        if let Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(key::Named::Tab),
            modifiers,
            ..
        }) = event
            && modifiers.command()
            && !modifiers.shift()
        {
            return self.update(Message::NextChatTab);
        }

        // Handle plain Tab key for field cycling
        if let Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(key::Named::Tab),
            modifiers,
            ..
        }) = event
            && !modifiers.command()
            && !modifiers.shift()
        {
            return self.update(Message::TabPressed);
        }
        // Handle Enter key
        if let Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(key::Named::Enter),
            ..
        }) = event
        {
            // Disconnect dialog (kick/ban modal) takes precedence —
            // matches the Escape priority for the disconnect dialog
            // further below. The per-input `on_submit` on the reason
            // field already handles Enter while typing; this
            // fallback covers focus outside the input (e.g., user
            // clicked a radio button).
            let disconnect_dialog_open = self
                .active_connection
                .and_then(|id| self.connections.get(&id))
                .is_some_and(|conn| conn.disconnect_dialog.is_some());
            if disconnect_dialog_open {
                return self.update(Message::DisconnectDialogSubmit);
            }

            if self.connection_form.connect_origin.is_some() {
                // Connection form summoned (tracker click, server-list "+",
                // etc.) overlays whatever else is visible — validate-then-
                // submit. Empty required fields surface the form-level
                // error banner via `ValidateConnectionForm`. Mirrors the
                // per-input `on_submit` in `views/connection.rs`.
                let can_connect = !self.connection_form.server_name.trim().is_empty()
                    && !self.connection_form.server_address.trim().is_empty()
                    && !self.connection_form.is_connecting;
                let msg = if can_connect {
                    Message::ConnectPressed
                } else {
                    Message::ValidateConnectionForm
                };
                return self.update(msg);
            } else if self.bookmark_edit.mode != BookmarkEditMode::None {
                // Bookmark editor's confirm-delete modal takes
                // priority — Enter inside it confirms the deletion
                // (matches the project-wide "modal IS the safety;
                // Enter confirms" rule for destructive dialogs).
                if self.bookmark_edit.confirm_delete
                    && let BookmarkEditMode::Edit(id) = self.bookmark_edit.mode
                {
                    return self.update(Message::ConfirmDeleteBookmark(id));
                }
                // Otherwise validate-then-submit on the form.
                // Empty required fields surface the form-level error
                // banner via `ValidateBookmarkEdit` rather than
                // silently no-op'ing.
                let can_save = !self.bookmark_edit.bookmark.name.trim().is_empty()
                    && !self.bookmark_edit.bookmark.address.trim().is_empty()
                    && !self.bookmark_edit.edit_stale
                    && !self.bookmark_edit.is_submitting;
                let msg = if can_save {
                    Message::SaveBookmark
                } else {
                    Message::ValidateBookmarkEdit
                };
                return self.update(msg);
            } else if self.active_panel() == ActivePanel::UserManagement {
                // On user management screen, handle Enter based on mode.
                // Group sub-form takes precedence over the Users tab —
                // mirrors the Tab dispatch ordering in
                // `handle_tab_navigation`. Each form follows the
                // validate-then-submit pattern: a complete form
                // submits, an incomplete form dispatches its
                // `Validate*` message so the form-level error banner
                // shows up rather than silently no-op'ing.
                if let Some(conn_id) = self.active_connection
                    && let Some(conn) = self.connections.get(&conn_id)
                {
                    match &conn.user_management.group_management.mode {
                        GroupManagementMode::Create => {
                            let can_create =
                                !conn.user_management.group_management.name.trim().is_empty()
                                    && !conn.user_management.group_management.is_submitting;
                            let msg = if can_create {
                                Message::GroupManagementCreatePressed
                            } else {
                                Message::ValidateGroupManagementCreate
                            };
                            return self.update(msg);
                        }
                        GroupManagementMode::Edit { new_name, .. } => {
                            let can_save = conn
                                .user_management
                                .group_management
                                .mode
                                .has_effective_group_update_changes()
                                && !new_name.trim().is_empty()
                                && !conn.user_management.group_management.is_submitting;
                            let msg = if can_save {
                                Message::GroupManagementUpdatePressed
                            } else {
                                Message::ValidateGroupManagementEdit
                            };
                            return self.update(msg);
                        }
                        GroupManagementMode::ConfirmDelete { .. } => {
                            return self.update(Message::GroupManagementConfirmDelete);
                        }
                        GroupManagementMode::List => {}
                    }
                    match &conn.user_management.mode {
                        UserManagementMode::Create => {
                            let can_create = !conn.user_management.username.trim().is_empty()
                                && !conn.user_management.password.is_empty()
                                && !conn.user_management.is_submitting;
                            let msg = if can_create {
                                Message::UserManagementCreatePressed
                            } else {
                                Message::ValidateUserManagementCreate
                            };
                            return self.update(msg);
                        }
                        UserManagementMode::Edit { new_username, .. } => {
                            let can_save =
                                conn.user_management.mode.has_effective_user_update_changes(
                                    &conn.connection_info.username,
                                    conn.is_admin,
                                ) && !new_username.trim().is_empty()
                                    && !conn.user_management.is_submitting
                                    && !conn.user_management.edit_stale;
                            let msg = if can_save {
                                Message::UserManagementUpdatePressed
                            } else {
                                Message::ValidateUserManagementEdit
                            };
                            return self.update(msg);
                        }
                        UserManagementMode::List => {
                            // List mode: No Enter action (use Escape to close)
                        }
                        UserManagementMode::ConfirmDelete { .. } => {
                            // The modal itself is the safety check —
                            // Enter inside it confirms (matches Tracker
                            // ConfirmRemove convention).
                            return self.update(Message::UserManagementConfirmDelete);
                        }
                    }
                }
            } else if self.active_panel() == ActivePanel::Broadcast {
                // On broadcast screen, try to send broadcast.
                // Validate-then-submit: empty input dispatches
                // `ValidateBroadcast` so the form-level error banner
                // surfaces rather than silently no-op'ing.
                if let Some(conn_id) = self.active_connection
                    && let Some(conn) = self.connections.get(&conn_id)
                {
                    let msg = if !conn.broadcast_message.trim().is_empty() {
                        Message::SendBroadcastPressed
                    } else {
                        Message::ValidateBroadcast
                    };
                    return self.update(msg);
                }
            } else if self.active_panel() == ActivePanel::Settings {
                // On settings screen, save settings
                return self.update(Message::SaveSettings);
            } else if self.active_panel() == ActivePanel::About {
                // On about screen, close the panel
                return self.update(Message::CloseAbout);
            } else if self.active_panel() == ActivePanel::ServerInfo {
                // On server info screen, dispatch based on what subview is open:
                //   server_info_edit ─→ UpdateServerInfoPressed
                //   tracker Add/Edit ─→ Validate or Submit (mirrors the form's on_submit)
                //   tracker ConfirmRemove / AcceptFingerprint ─→ Confirm
                //     (these modals have no text inputs, so the keyboard handler is
                //      the only path that can fire their primary action)
                //   otherwise ─→ CloseServerInfo
                if let Some(conn_id) = self.active_connection
                    && let Some(conn) = self.connections.get(&conn_id)
                {
                    if let Some(edit_state) = conn.server_info_edit.as_ref() {
                        if edit_state.is_submitting {
                            return Task::none();
                        }
                        if edit_state.has_changes_from_original() {
                            return self.update(Message::UpdateServerInfoPressed);
                        }
                        return Task::none();
                    }
                    match &conn.tracker_management.mode {
                        TrackerManagementMode::Add => {
                            let can_submit = !conn.tracker_management.add_name.trim().is_empty()
                                && !conn.tracker_management.add_address.trim().is_empty()
                                && !conn.tracker_management.is_submitting;
                            let msg = if can_submit {
                                Message::AddTrackerSubmit
                            } else {
                                Message::ValidateAddTracker
                            };
                            return self.update(msg);
                        }
                        TrackerManagementMode::Edit { name, address, .. } => {
                            let can_submit = conn
                                .tracker_management
                                .mode
                                .has_effective_tracker_update_changes()
                                && !name.trim().is_empty()
                                && !address.trim().is_empty()
                                && !conn.tracker_management.is_submitting;
                            let msg = if can_submit {
                                Message::EditTrackerSubmit
                            } else {
                                Message::ValidateEditTracker
                            };
                            return self.update(msg);
                        }
                        TrackerManagementMode::ConfirmRemove { .. } => {
                            return self.update(Message::RemoveTrackerConfirm);
                        }
                        TrackerManagementMode::AcceptFingerprint { .. } => {
                            return self.update(Message::AcceptFingerprintConfirm);
                        }
                        TrackerManagementMode::List => {}
                    }
                }
                return self.update(Message::CloseServerInfo);
            } else if self.active_panel() == ActivePanel::UserInfo {
                // On user info screen, close the panel
                return self.update(Message::CloseUserInfo);
            } else if self.active_panel() == ActivePanel::ChangePassword {
                // Validate-then-submit. Empty fields / mismatched
                // passwords / weak passwords surface the form-level
                // error banner via `ValidateChangePassword` rather
                // than silently no-op'ing.
                let (can_save, is_submitting) = self
                    .active_connection
                    .and_then(|id| self.connections.get(&id))
                    .and_then(|conn| conn.password_change_state.as_ref())
                    .map(|state| {
                        let can = !state.current_password.is_empty()
                            && !state.new_password.is_empty()
                            && !state.confirm_password.is_empty();
                        (can, state.is_submitting)
                    })
                    .unwrap_or((false, false));
                if is_submitting {
                    return Task::none();
                }
                let msg = if can_save {
                    Message::ChangePasswordSavePressed
                } else {
                    Message::ValidateChangePassword
                };
                return self.update(msg);
            } else if self.active_panel() == ActivePanel::TrackerBrowser {
                // On the tracker discovery panel, dispatch based on
                // what subview is open:
                //   Add / Edit                  → Submit (or Validate
                //                                  if the form is incomplete,
                //                                  matching the form's
                //                                  on_submit fallback).
                //   ConfirmRemove               → RemoveConfirm.
                //   AcceptFingerprint           → AcceptFingerprintConfirm.
                //     (these modals have no text inputs, so the keyboard
                //      handler is the only path that can fire their
                //      primary action.)
                //   List                        → no Enter action
                //                                  (Escape closes the panel).
                match &self.tracker_browser.mode {
                    crate::types::TrackerBrowserMode::Add => {
                        let can_submit = !self.tracker_browser.add_name.trim().is_empty()
                            && !self.tracker_browser.add_address.trim().is_empty()
                            && !self.tracker_browser.is_submitting;
                        let msg = if can_submit {
                            Message::TrackerBrowserAddSubmit
                        } else {
                            Message::TrackerBrowserValidateAdd
                        };
                        return self.update(msg);
                    }
                    crate::types::TrackerBrowserMode::Edit { name, address, .. } => {
                        let can_submit = !name.trim().is_empty()
                            && !address.trim().is_empty()
                            && !self.tracker_browser.is_submitting;
                        let msg = if can_submit {
                            Message::TrackerBrowserEditSubmit
                        } else {
                            Message::TrackerBrowserValidateEdit
                        };
                        return self.update(msg);
                    }
                    crate::types::TrackerBrowserMode::ConfirmRemove { .. } => {
                        return self.update(Message::TrackerBrowserRemoveConfirm);
                    }
                    crate::types::TrackerBrowserMode::AcceptFingerprint { .. } => {
                        return self.update(Message::TrackerBrowserAcceptFingerprintConfirm);
                    }
                    crate::types::TrackerBrowserMode::List => {}
                }
            } else if self.active_panel() == ActivePanel::News {
                // On news screen, handle Enter based on mode
                if let Some(conn_id) = self.active_connection
                    && let Some(conn) = self.connections.get(&conn_id)
                {
                    match &conn.news_management.mode {
                        NewsManagementMode::Create | NewsManagementMode::Edit { .. } => {
                            // Create/Edit mode: Don't submit on Enter - the text_editor
                            // uses Enter for newlines. Users must click the submit button.
                        }
                        NewsManagementMode::List => {
                            // List mode: No Enter action (use Escape to close)
                        }
                        NewsManagementMode::ConfirmDelete { .. } => {
                            // The modal itself is the safety check —
                            // Enter inside it confirms.
                            return self.update(Message::NewsConfirmDelete);
                        }
                    }
                }
            } else if self.active_panel() == ActivePanel::Files {
                // Files panel — modal confirms (overwrite / delete) take
                // priority over the input dialogs (rename / create dir).
                // Mirrors the Escape cascade ordering. Confirm-delete
                // and overwrite-confirm are destructive; we follow the
                // project rule that the modal itself is the safety
                // check, so Enter inside it confirms.
                if let Some(conn_id) = self.active_connection
                    && let Some(conn) = self.connections.get(&conn_id)
                {
                    let tab = conn.files_management.active_tab();
                    if tab.pending_overwrite.is_some() {
                        return self.update(Message::FileOverwriteConfirm);
                    } else if tab.pending_delete.is_some() {
                        return self.update(Message::FileConfirmDelete);
                    } else if tab.pending_rename.is_some() {
                        return self.update(Message::FileRenameSubmit);
                    } else if tab.creating_directory {
                        return self.update(Message::FileNewDirectorySubmit);
                    }
                }
            } else if self.active_connection.is_none() {
                // Disconnected default state — connection form is the
                // layout fallback. Same validate-then-submit pattern
                // as the summoned overlay branch above.
                let can_connect = !self.connection_form.server_name.trim().is_empty()
                    && !self.connection_form.server_address.trim().is_empty()
                    && !self.connection_form.is_connecting;
                let msg = if can_connect {
                    Message::ConnectPressed
                } else {
                    Message::ValidateConnectionForm
                };
                return self.update(msg);
            }
        }
        // Handle Escape key
        if let Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(key::Named::Escape),
            ..
        }) = event
        {
            // First check if disconnect dialog is open
            if let Some(conn_id) = self.active_connection
                && let Some(conn) = self.connections.get(&conn_id)
                && conn.disconnect_dialog.is_some()
            {
                return self.update(Message::DisconnectDialogCancel);
            }

            if self.connection_form.connect_origin.is_some() {
                // Connection form is summoned (tracker click, "+", etc.).
                // Mirrors the layout's modal-like ordering — connect form
                // sits on top of bookmark edit, so Escape dismisses connect
                // form first. Default-state form has origin=None and falls
                // through to the panel match below (Escape on disconnected
                // fallthrough is a no-op via ActivePanel::None).
                return self.update(Message::ConnectionFormCancel);
            } else if self.bookmark_edit.mode != BookmarkEditMode::None {
                // If the confirm-delete modal is open, Escape closes
                // just the modal (matches the uniform "Escape cancels
                // the modal, not the form behind it" pattern). Only
                // an Escape in the editor with no modal active
                // dismisses the editor itself.
                if self.bookmark_edit.confirm_delete {
                    return self.update(Message::CancelDeleteBookmark);
                }
                return self.update(Message::CancelBookmarkEdit);
            } else {
                // Cancel active panel
                match self.active_panel() {
                    ActivePanel::About => return self.update(Message::CloseAbout),
                    ActivePanel::UserManagement => {
                        // In user management, Escape returns to list (or closes if on list)
                        return self.update(Message::CancelUserManagement);
                    }
                    ActivePanel::Broadcast => return self.update(Message::CancelBroadcast),
                    ActivePanel::Settings => return self.update(Message::CancelSettings),
                    ActivePanel::ServerInfo => {
                        // Escape priority on the Server Info panel:
                        //   1. Edit form open    → cancel edit, return to display.
                        //   2. Tracker subview / modal open → return to trackers list.
                        //   3. Otherwise         → close the panel entirely.
                        if let Some(conn_id) = self.active_connection
                            && let Some(conn) = self.connections.get(&conn_id)
                        {
                            if conn.server_info_edit.is_some() {
                                return self.update(Message::CancelEditServerInfo);
                            }
                            if !matches!(
                                conn.tracker_management.mode,
                                crate::types::TrackerManagementMode::List
                            ) {
                                return self.update(Message::CancelTrackerManagement);
                            }
                        }
                        return self.update(Message::CloseServerInfo);
                    }
                    ActivePanel::UserInfo => return self.update(Message::CloseUserInfo),
                    ActivePanel::ChangePassword => {
                        return self.update(Message::ChangePasswordCancelPressed);
                    }
                    ActivePanel::News => {
                        // In news panel, Escape returns to list (or closes if on list)
                        return self.update(Message::CancelNews);
                    }
                    ActivePanel::Files => {
                        // If overwrite confirmation is showing, cancel it
                        if let Some(conn_id) = self.active_connection
                            && let Some(conn) = self.connections.get(&conn_id)
                            && conn
                                .files_management
                                .active_tab()
                                .pending_overwrite
                                .is_some()
                        {
                            return self.update(Message::FileOverwriteCancel);
                        }
                        // If file info dialog is showing, close it
                        if let Some(conn_id) = self.active_connection
                            && let Some(conn) = self.connections.get(&conn_id)
                            && conn.files_management.active_tab().pending_info.is_some()
                        {
                            return self.update(Message::CloseFileInfo);
                        }
                        // If rename dialog is showing, cancel it
                        if let Some(conn_id) = self.active_connection
                            && let Some(conn) = self.connections.get(&conn_id)
                            && conn.files_management.active_tab().pending_rename.is_some()
                        {
                            return self.update(Message::FileRenameCancel);
                        }
                        // If delete confirmation is showing, cancel it
                        if let Some(conn_id) = self.active_connection
                            && let Some(conn) = self.connections.get(&conn_id)
                            && conn.files_management.active_tab().pending_delete.is_some()
                        {
                            return self.update(Message::FileCancelDelete);
                        }
                        // If creating directory, close dialog
                        if let Some(conn_id) = self.active_connection
                            && let Some(conn) = self.connections.get(&conn_id)
                            && conn.files_management.active_tab().creating_directory
                        {
                            return self.update(Message::FileNewDirectoryCancel);
                        }
                        // If clipboard has content, clear it
                        if let Some(conn_id) = self.active_connection
                            && let Some(conn) = self.connections.get(&conn_id)
                            && conn.files_management.clipboard.is_some()
                        {
                            return self.update(Message::FileClearClipboard);
                        }
                        // If in search mode, clear input and submit to exit search
                        if let Some(conn_id) = self.active_connection
                            && let Some(conn) = self.connections.get_mut(&conn_id)
                            && conn.files_management.active_tab().is_searching()
                        {
                            // Clear input directly, then submit to trigger exit from search mode
                            conn.files_management.active_tab_mut().search_input.clear();
                            return self.update(Message::FileSearchSubmit);
                        }
                        // Otherwise close panel
                        return self.update(Message::CancelFiles);
                    }
                    ActivePanel::Transfers => return self.update(Message::CloseTransfers),
                    ActivePanel::TrackerBrowser => {
                        // Escape priority on the discovery panel:
                        //   1. In a sub-mode (Add / Edit / ConfirmRemove /
                        //      AcceptFingerprint) → return to the list.
                        //   2. On the list → close the panel.
                        // Step 2 only reaches the list, but the sub-mode branch
                        // is wired now so step 3+ doesn't have to retouch this.
                        if !matches!(
                            self.tracker_browser.mode,
                            crate::types::TrackerBrowserMode::List
                        ) {
                            self.tracker_browser.reset_to_list();
                            return Task::none();
                        }
                        return self.update(Message::CloseTrackerBrowser);
                    }
                    ActivePanel::ConnectionMonitor => {
                        return self.update(Message::CloseConnectionMonitor);
                    }
                    ActivePanel::None => {}
                }
            }
        }
        Task::none()
    }

    /// Navigate to the next chat tab (wraps around)
    ///
    /// Works when chat is visible (no panel active) or when Files panel is active.
    pub fn handle_next_chat_tab(&mut self) -> Task<Message> {
        let active_panel = self.active_panel();

        // Handle file tabs when Files panel is active
        if active_panel == ActivePanel::Files {
            let Some(conn_id) = self.active_connection else {
                return Task::none();
            };
            let Some(conn) = self.connections.get_mut(&conn_id) else {
                return Task::none();
            };

            conn.files_management.next_tab();
            return Task::none();
        }

        // Only switch chat tabs when chat is visible (no panel active)
        if active_panel != ActivePanel::None {
            return Task::none();
        }

        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get(&conn_id) else {
            return Task::none();
        };

        // Build ordered list of tabs: Console + Channels (join order) + User messages (creation order)
        let mut tabs = vec![ChatTab::Console];
        for channel in &conn.channel_tabs {
            tabs.push(ChatTab::Channel(channel.clone()));
        }
        for nickname in &conn.user_message_tabs {
            tabs.push(ChatTab::UserMessage(nickname.clone()));
        }

        // Find current tab index and move to next (with wrap)
        let current_index = tabs
            .iter()
            .position(|t| *t == conn.active_chat_tab)
            .unwrap_or(0);
        let next_index = (current_index + 1) % tabs.len();
        let next_tab = tabs[next_index].clone();

        self.update(Message::SwitchChatTab(next_tab))
    }

    /// Navigate to the previous chat tab (wraps around)
    ///
    /// Works when chat is visible (no panel active) or when Files panel is active.
    pub fn handle_prev_chat_tab(&mut self) -> Task<Message> {
        let active_panel = self.active_panel();

        // Handle file tabs when Files panel is active
        if active_panel == ActivePanel::Files {
            let Some(conn_id) = self.active_connection else {
                return Task::none();
            };
            let Some(conn) = self.connections.get_mut(&conn_id) else {
                return Task::none();
            };

            conn.files_management.prev_tab();
            return Task::none();
        }

        // Only switch chat tabs when chat is visible (no panel active)
        if active_panel != ActivePanel::None {
            return Task::none();
        }

        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get(&conn_id) else {
            return Task::none();
        };

        // Build ordered list of tabs: Console + Channels (join order) + User messages (creation order)
        let mut tabs = vec![ChatTab::Console];
        for channel in &conn.channel_tabs {
            tabs.push(ChatTab::Channel(channel.clone()));
        }
        for nickname in &conn.user_message_tabs {
            tabs.push(ChatTab::UserMessage(nickname.clone()));
        }

        // Find current tab index and move to previous (with wrap)
        let current_index = tabs
            .iter()
            .position(|t| *t == conn.active_chat_tab)
            .unwrap_or(0);
        let prev_index = if current_index == 0 {
            tabs.len() - 1
        } else {
            current_index - 1
        };
        let prev_tab = tabs[prev_index].clone();

        self.update(Message::SwitchChatTab(prev_tab))
    }

    /// Handle Tab key navigation across different screens
    pub fn handle_tab_navigation(&mut self) -> Task<Message> {
        if self.connection_form.connect_origin.is_some() {
            // Connection form summoned (tracker click, server-list "+",
            // etc.) overlays whatever else is visible. Mirrors the
            // layout's modal-like ordering — connect form sits on top
            // of bookmark edit and every panel, so Tab cycles its
            // fields ahead of any other tab handler. Default-state
            // form has origin=None and falls through to the
            // `active_connection.is_none()` branch below.
            return self.update(Message::ConnectionFormTabPressed);
        } else if self.bookmark_edit.mode != BookmarkEditMode::None {
            // On bookmark edit screen, check actual focus and cycle
            return self.update(Message::BookmarkEditTabPressed);
        } else if self.active_panel() == ActivePanel::UserManagement {
            // On user management screen, handle Tab based on mode.
            if let Some(conn_id) = self.active_connection
                && let Some(conn) = self.connections.get(&conn_id)
            {
                // Group sub-form (Create / Edit) takes precedence over
                // the Users tab when active.
                match &conn.user_management.group_management.mode {
                    GroupManagementMode::Create => {
                        return self.update(Message::GroupManagementCreateTabPressed);
                    }
                    GroupManagementMode::Edit { .. } => {
                        return self.update(Message::GroupManagementEditTabPressed);
                    }
                    GroupManagementMode::List | GroupManagementMode::ConfirmDelete { .. } => {}
                }
                match &conn.user_management.mode {
                    UserManagementMode::Create => {
                        return self.update(Message::UserManagementCreateTabPressed);
                    }
                    UserManagementMode::Edit { .. } => {
                        return self.update(Message::UserManagementEditTabPressed);
                    }
                    UserManagementMode::List | UserManagementMode::ConfirmDelete { .. } => {
                        // List/ConfirmDelete: No Tab navigation
                    }
                }
            }
        } else if self.active_panel() == ActivePanel::ChangePassword {
            // Change password panel: check actual focus and cycle through fields
            return self.update(Message::ChangePasswordTabPressed);
        } else if self.active_panel() == ActivePanel::ServerInfo {
            // Server info panel: dispatch on edit-form / tracker-subview state.
            if let Some(conn_id) = self.active_connection
                && let Some(conn) = self.connections.get(&conn_id)
            {
                if conn.server_info_edit.is_some() {
                    return self.update(Message::ServerInfoEditTabPressed);
                }
                // Gate tracker-form Tab dispatches on `tracker_list` —
                // the tab itself is hidden from `views/server_info.rs`
                // when the perm is revoked mid-session, but the form's
                // mode flag may still be `Add`/`Edit` until the next
                // mode transition. Without the gate, Tab would fire
                // focus-ops at widgets that aren't mounted (no-op but
                // visibly inert).
                if conn.has_permission(PERMISSION_TRACKER_LIST) {
                    match &conn.tracker_management.mode {
                        crate::types::TrackerManagementMode::Add => {
                            return self.update(Message::AddTrackerTabPressed);
                        }
                        crate::types::TrackerManagementMode::Edit { .. } => {
                            return self.update(Message::EditTrackerTabPressed);
                        }
                        // List / ConfirmRemove / AcceptFingerprint: no Tab cycling.
                        _ => {}
                    }
                }
            }
        } else if self.active_panel() == ActivePanel::Files {
            // Files panel: focus the appropriate input if a dialog is
            // open; otherwise focus the search input. `operation::focus`
            // is a no-op when the target id isn't currently rendered
            // (e.g. user lacks `file_search` permission so the search
            // input isn't in the widget tree), so this is safe to fire
            // unconditionally.
            // Compute the focus target while the `conn` borrow is
            // alive, then drop it and dispatch the focus through
            // `focus_field` so the in-memory tracker stays in sync.
            let target = if let Some(conn_id) = self.active_connection
                && let Some(conn) = self.connections.get(&conn_id)
            {
                let tab = conn.files_management.active_tab();
                if tab.pending_rename.is_some() {
                    InputId::RenameName
                } else if tab.creating_directory {
                    InputId::NewDirectoryName
                } else {
                    InputId::FileSearchInput
                }
            } else {
                InputId::FileSearchInput
            };
            return self.focus_field(target);
        } else if self.active_panel() == ActivePanel::Broadcast {
            // Broadcast screen only has one field, so focus stays
            return self.focus_field(InputId::BroadcastMessage);
        } else if self.active_panel() == ActivePanel::News {
            // News uses `text_editor` for the body; in Create / Edit
            // mode, Tab refocuses it if focus has wandered (e.g. user
            // clicked the image picker button or a news row). List /
            // ConfirmDelete have no input to focus.
            let in_create_or_edit = self
                .active_connection
                .and_then(|id| self.connections.get(&id))
                .is_some_and(|conn| {
                    matches!(
                        conn.news_management.mode,
                        NewsManagementMode::Create | NewsManagementMode::Edit { .. }
                    )
                });
            if in_create_or_edit {
                return self.focus_field(InputId::NewsBody);
            }
            return Task::none();
        } else if self.active_panel() == ActivePanel::Settings {
            // Settings panel: check actual focus and cycle through fields
            return self.update(Message::SettingsTabPressed);
        } else if self.active_panel() == ActivePanel::TrackerBrowser {
            // Tracker discovery panel: dispatch on sub-mode.
            // ConfirmRemove / AcceptFingerprint have no text inputs.
            match &self.tracker_browser.mode {
                crate::types::TrackerBrowserMode::Add => {
                    return self.update(Message::TrackerBrowserAddTabPressed);
                }
                crate::types::TrackerBrowserMode::Edit { .. } => {
                    return self.update(Message::TrackerBrowserEditTabPressed);
                }
                crate::types::TrackerBrowserMode::List => {
                    // List mode has a single text input (search). Tab
                    // refocuses it regardless of where focus had
                    // wandered.
                    return self.focus_field(InputId::TrackerBrowserSearch);
                }
                _ => {}
            }
        } else if self.active_connection.is_some() {
            // Disconnect dialog (kick/ban modal) takes precedence
            // over the chat view when open — Tab focuses the reason
            // input.
            let dialog_open = self
                .active_connection
                .and_then(|id| self.connections.get(&id))
                .is_some_and(|conn| conn.disconnect_dialog.is_some());
            if dialog_open {
                return self.focus_field(InputId::DisconnectDialogReason);
            }
            // In chat view: query iced for focused widget. The
            // resolver picks between nickname tab-completion (when
            // ChatInput is focused) and refocusing ChatInput (when
            // focus has wandered to a non-input widget).
            return super::focus::dispatch_find_focused(Message::ChatTabResolved);
        } else if self.active_connection.is_none() {
            // On connection screen, check actual focus and cycle
            return self.update(Message::ConnectionFormTabPressed);
        }
        Task::none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use iced::keyboard::key::{Code, Physical};
    use nexus_common::framing::MessageId;
    use nexus_common::validators::{self, PasswordStrength};
    use tokio::sync::{Mutex, mpsc};

    use crate::i18n::t_args;
    use crate::types::{
        BookmarkEditMode, ConnectionInfo, DisconnectDialogState, ServerBookmark, ServerConnection,
        ServerConnectionParams,
    };

    fn key_press(named: key::Named, code: Code) -> Event {
        Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(named),
            modified_key: keyboard::Key::Named(named),
            physical_key: Physical::Code(code),
            location: keyboard::Location::Standard,
            modifiers: keyboard::Modifiers::default(),
            text: None,
            repeat: false,
        })
    }

    fn test_connection(connection_id: usize) -> ServerConnection {
        let (tx, _rx) = mpsc::unbounded_channel::<(MessageId, ClientMessage)>();
        ServerConnection::new(ServerConnectionParams {
            bookmark_id: None,
            user_id: None,
            nickname: "me".to_string(),
            connection_info: ConnectionInfo {
                server_name: String::new(),
                address: String::new(),
                port: 0,
                transfer_port: 0,
                certificate_fingerprint: String::new(),
                username: "me".to_string(),
                password: String::new(),
                nickname: "me".to_string(),
            },
            display_name: String::new(),
            connection_id,
            is_admin: false,
            permissions: Vec::new(),
            features: Vec::new(),
            server_name: None,
            server_description: None,
            public_address: None,
            server_version: None,
            server_image: String::new(),
            cached_server_image: None,
            chat_burst_limit: None,
            chat_rate_limit: None,
            max_connections_per_ip: None,
            max_outbound_rate: None,
            max_transfers_per_ip: None,
            file_reindex_interval: None,
            persistent_channels: None,
            auto_join_channels: None,
            min_password_strength: PasswordStrength::Weak,
            log_level: None,
            scheduler_chunk_size: None,
            tx,
            shutdown_handle: Arc::new(Mutex::new(None)),
        })
    }

    fn app_with_connection() -> NexusApp {
        let mut app = NexusApp {
            active_connection: Some(1),
            ..NexusApp::default()
        };
        app.connections.insert(1, test_connection(1));
        app
    }

    #[test]
    fn escape_prioritizes_disconnect_dialog_over_connection_form() {
        let mut app = app_with_connection();
        app.connection_form.connect_origin = Some(ActivePanel::Files);
        app.connections
            .get_mut(&1)
            .expect("test connection should exist")
            .disconnect_dialog = Some(DisconnectDialogState::new("Bob".to_string()));

        let _ = app.handle_keyboard_event(key_press(key::Named::Escape, Code::Escape));

        assert!(app.connections[&1].disconnect_dialog.is_none());
        assert_eq!(app.connection_form.connect_origin, Some(ActivePanel::Files));
    }

    #[test]
    fn enter_prioritizes_disconnect_dialog_over_connection_form() {
        let mut app = app_with_connection();
        app.connection_form.connect_origin = Some(ActivePanel::Files);
        app.connection_form.server_name = "Server".to_string();
        app.connection_form.server_address = "example.com".to_string();

        let mut dialog = DisconnectDialogState::new("Bob".to_string());
        dialog.reason = "x".repeat(validators::MAX_KICK_REASON_LENGTH + 1);
        app.connections
            .get_mut(&1)
            .expect("test connection should exist")
            .disconnect_dialog = Some(dialog);

        let _ = app.handle_keyboard_event(key_press(key::Named::Enter, Code::Enter));

        let dialog = app.connections[&1]
            .disconnect_dialog
            .as_ref()
            .expect("invalid dialog submit should keep dialog open");
        assert_eq!(
            dialog.error.as_deref(),
            Some(
                t_args(
                    "err-kick-reason-too-long",
                    &[("max", &validators::MAX_KICK_REASON_LENGTH.to_string())]
                )
                .as_str()
            )
        );
        assert!(!app.connection_form.is_connecting);
        assert_eq!(app.connection_form.connect_origin, Some(ActivePanel::Files));
    }

    #[test]
    fn escape_prioritizes_connection_form_over_bookmark_editor() {
        let mut app = NexusApp::default();
        app.connection_form.connect_origin = Some(ActivePanel::News);
        app.bookmark_edit.mode = BookmarkEditMode::Add;

        let _ = app.handle_keyboard_event(key_press(key::Named::Escape, Code::Escape));

        assert!(app.connection_form.connect_origin.is_none());
        assert_eq!(app.bookmark_edit.mode, BookmarkEditMode::Add);
    }

    #[test]
    fn enter_prioritizes_connection_form_over_bookmark_editor() {
        let mut app = NexusApp::default();
        app.connection_form.connect_origin = Some(ActivePanel::News);
        app.bookmark_edit.mode = BookmarkEditMode::Add;
        app.bookmark_edit.bookmark = ServerBookmark {
            name: "Valid Bookmark".to_string(),
            address: "example.com".to_string(),
            ..ServerBookmark::default()
        };

        let _ = app.handle_keyboard_event(key_press(key::Named::Enter, Code::Enter));

        assert!(app.connection_form.error.is_some());
        assert_eq!(app.bookmark_edit.mode, BookmarkEditMode::Add);
        assert!(app.bookmark_edit.error.is_none());
    }

    #[test]
    fn escape_prioritizes_bookmark_editor_over_active_panel() {
        let mut app = NexusApp::default();
        app.ui_state.active_panel = ActivePanel::About;
        app.bookmark_edit.mode = BookmarkEditMode::Add;

        let _ = app.handle_keyboard_event(key_press(key::Named::Escape, Code::Escape));

        assert_eq!(app.active_panel(), ActivePanel::About);
        assert_eq!(app.bookmark_edit.mode, BookmarkEditMode::None);
    }

    #[test]
    fn enter_prioritizes_bookmark_editor_over_active_panel() {
        let mut app = NexusApp::default();
        app.ui_state.active_panel = ActivePanel::About;
        app.bookmark_edit.mode = BookmarkEditMode::Add;

        let _ = app.handle_keyboard_event(key_press(key::Named::Enter, Code::Enter));

        assert_eq!(app.active_panel(), ActivePanel::About);
        assert_eq!(app.bookmark_edit.mode, BookmarkEditMode::Add);
        assert!(app.bookmark_edit.error.is_some());
    }
}
