//! Bookmark management

use iced::Task;
use iced::widget::{Id, operation};
use nexus_common::fingerprint::is_canonical_fingerprint;
use uuid::Uuid;

use crate::NexusApp;
use crate::i18n::{get_locale, t, t_args};
use crate::network::{ConnectionParams, ProxyConfig};
use crate::types::{
    BookmarkEditMode, BookmarkEditState, InputId, Message, normalize_certificate_fingerprint,
};

impl NexusApp {
    // ==================== Form Field Handlers ====================

    /// Handle bookmark address field change
    pub fn handle_bookmark_address_changed(&mut self, addr: String) -> Task<Message> {
        self.bookmark_edit.bookmark.address = addr;
        self.bookmark_edit.error = None;
        self.focused_field = InputId::BookmarkAddress;
        Task::none()
    }

    /// Handle bookmark auto-connect toggle
    pub fn handle_bookmark_auto_connect_toggled(&mut self, enabled: bool) -> Task<Message> {
        self.bookmark_edit.bookmark.auto_connect = enabled;
        Task::none()
    }

    /// Handle bookmark name field change
    pub fn handle_bookmark_name_changed(&mut self, name: String) -> Task<Message> {
        self.bookmark_edit.bookmark.name = name;
        self.bookmark_edit.error = None;
        self.focused_field = InputId::BookmarkName;
        Task::none()
    }

    /// Handle bookmark password field change
    pub fn handle_bookmark_password_changed(&mut self, password: String) -> Task<Message> {
        self.bookmark_edit.bookmark.password = password;
        self.bookmark_edit.error = None;
        self.focused_field = InputId::BookmarkPassword;
        Task::none()
    }

    /// Handle bookmark port field change
    pub fn handle_bookmark_port_changed(&mut self, port: u16) -> Task<Message> {
        self.bookmark_edit.bookmark.port = port;
        self.bookmark_edit.error = None;
        self.focused_field = InputId::BookmarkPort;
        Task::none()
    }

    /// Handle bookmark username field change
    pub fn handle_bookmark_username_changed(&mut self, username: String) -> Task<Message> {
        self.bookmark_edit.bookmark.username = username;
        self.bookmark_edit.error = None;
        self.focused_field = InputId::BookmarkUsername;
        Task::none()
    }

    /// Handle bookmark nickname field change
    pub fn handle_bookmark_nickname_changed(&mut self, nickname: String) -> Task<Message> {
        self.bookmark_edit.bookmark.nickname = nickname;
        self.bookmark_edit.error = None;
        self.focused_field = InputId::BookmarkNickname;
        Task::none()
    }

    /// Handle bookmark fingerprint pin field change.
    ///
    /// Routes through [`normalize_certificate_fingerprint`] so whitespace-only
    /// input (typing-then-deleting) collapses to `None` immediately rather
    /// than transiently storing `Some(" ")`. Same normalization Save runs, so
    /// Validate and Save see identical state.
    pub fn handle_bookmark_fingerprint_changed(&mut self, fingerprint: String) -> Task<Message> {
        self.bookmark_edit.bookmark.certificate_fingerprint =
            normalize_certificate_fingerprint(Some(fingerprint));
        self.bookmark_edit.error = None;
        self.focused_field = InputId::BookmarkFingerprint;
        Task::none()
    }

    // ==================== Dialog Actions ====================

    /// Cancel bookmark editing and close the dialog
    pub fn handle_cancel_bookmark_edit(&mut self) -> Task<Message> {
        self.bookmark_edit = BookmarkEditState::default();
        // Restore chat scroll position when closing bookmark editor
        self.scroll_chat_if_visible(false)
    }

    /// Save the current bookmark (add or update)
    pub fn handle_save_bookmark(&mut self) -> Task<Message> {
        if self.bookmark_edit.is_submitting {
            return Task::none();
        }

        if let Some(error) = self.validate_bookmark() {
            self.bookmark_edit.error = Some(error);
            return Task::none();
        }

        self.bookmark_edit.is_submitting = true;

        // Normalize whitespace on identifying fields so lookups don't miss due
        // to user-typed leading/trailing whitespace. Password is left as-typed
        // since users could have intentional whitespace there.
        self.bookmark_edit.bookmark.name = self.bookmark_edit.bookmark.name.trim().to_string();
        self.bookmark_edit.bookmark.address =
            self.bookmark_edit.bookmark.address.trim().to_string();
        self.bookmark_edit.bookmark.username =
            self.bookmark_edit.bookmark.username.trim().to_string();
        self.bookmark_edit.bookmark.nickname =
            self.bookmark_edit.bookmark.nickname.trim().to_string();
        // Normalize the fingerprint pin so the on-disk shape stays consistent
        // regardless of how the user typed it (e.g. stray whitespace won't
        // produce a `Some("…")` value with leading/trailing spaces).
        self.bookmark_edit.bookmark.certificate_fingerprint = normalize_certificate_fingerprint(
            self.bookmark_edit.bookmark.certificate_fingerprint.clone(),
        );

        let bookmark = self.bookmark_edit.bookmark.clone();

        match self.bookmark_edit.mode {
            BookmarkEditMode::Add => {
                self.config.add_bookmark(bookmark);
            }
            BookmarkEditMode::Edit(id) => {
                self.config.update_bookmark(id, bookmark);
            }
            BookmarkEditMode::None => {}
        }

        if let Err(e) = self.config.save() {
            self.bookmark_edit.is_submitting = false;
            self.bookmark_edit.error = Some(t_args(
                "err-failed-save-config",
                &[("error", &e.to_string())],
            ));
            return Task::none();
        }

        self.bookmark_edit = BookmarkEditState::default();
        // Restore chat scroll position when closing bookmark editor
        self.scroll_chat_if_visible(false)
    }

    /// Show the add bookmark dialog
    ///
    /// If there's an active connection, pre-fills the form with connection data.
    /// Otherwise, shows an empty form.
    pub fn handle_show_add_bookmark(&mut self) -> Task<Message> {
        self.bookmark_edit = BookmarkEditState::default();
        self.bookmark_edit.mode = BookmarkEditMode::Add;

        // Pre-fill from active connection if available
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get(&conn_id)
        {
            self.bookmark_edit.bookmark.name = conn
                .server_name
                .clone()
                .unwrap_or_else(|| conn.display_name.clone());
            self.bookmark_edit.bookmark.address = conn.connection_info.address.clone();
            self.bookmark_edit.bookmark.port = conn.connection_info.port;
            self.bookmark_edit.bookmark.username = conn.connection_info.username.clone();
            self.bookmark_edit.bookmark.password = conn.connection_info.password.clone();
            self.bookmark_edit.bookmark.nickname = conn.connection_info.nickname.clone();
            self.bookmark_edit.bookmark.certificate_fingerprint = normalize_certificate_fingerprint(
                Some(conn.connection_info.certificate_fingerprint.clone()),
            );
        }

        self.focused_field = InputId::BookmarkName;
        operation::focus(Id::from(InputId::BookmarkName))
    }

    /// Show the edit bookmark dialog for a specific bookmark
    pub fn handle_show_edit_bookmark(&mut self, id: Uuid) -> Task<Message> {
        if let Some(bookmark) = self.config.get_bookmark(id) {
            self.bookmark_edit.mode = BookmarkEditMode::Edit(id);
            self.bookmark_edit.bookmark = bookmark.clone();
            self.focused_field = InputId::BookmarkName;

            // Move any connection error to the edit dialog (acknowledges and clears it)
            self.bookmark_edit.error = self.bookmark_errors.remove(&id);

            return operation::focus(Id::from(InputId::BookmarkName));
        }
        Task::none()
    }

    // ==================== Bookmark Operations ====================

    /// Connect to a bookmarked server
    pub fn handle_connect_to_bookmark(&mut self, id: Uuid) -> Task<Message> {
        if self.connecting_bookmarks.contains(&id) {
            return Task::none();
        }

        if let Some(bookmark) = self.config.get_bookmark(id) {
            self.connecting_bookmarks.insert(id);

            let connection_id = self.next_connection_id;
            self.next_connection_id += 1;

            let port = bookmark.port;

            let server_address = bookmark.address.clone();
            let username = bookmark.username.clone();
            let password = bookmark.password.clone();
            // Use bookmark nickname, falling back to settings default
            let nickname = if bookmark.nickname.is_empty() {
                self.config.settings.nickname.clone()
            } else {
                Some(bookmark.nickname.clone())
            };
            let locale = get_locale().to_string();
            let avatar = self.config.settings.avatar.clone();
            let display_name = bookmark.name.clone();
            let expected_fingerprint = bookmark.certificate_fingerprint.clone();

            // Build proxy config if enabled
            let proxy = if self.config.settings.proxy.enabled {
                Some(ProxyConfig {
                    address: self.config.settings.proxy.address.clone(),
                    port: self.config.settings.proxy.port,
                    username: self.config.settings.proxy.username.clone(),
                    password: self.config.settings.proxy.password.clone(),
                })
            } else {
                None
            };

            let params = ConnectionParams {
                server_address,
                port,
                username,
                password,
                nickname,
                locale,
                avatar,
                connection_id,
                proxy,
                expected_fingerprint,
            };
            // Clone for the result handler so an accept-after-mismatch can
            // replay the original intent.
            let retry_params = params.clone();

            return Task::perform(
                async move { crate::network::connect_to_server(params).await },
                move |result| Message::BookmarkConnectionResult {
                    result,
                    params: retry_params,
                    bookmark_id: id,
                    display_name,
                },
            );
        }
        Task::none()
    }

    /// Delete a bookmark by ID
    pub fn handle_delete_bookmark(&mut self, id: Uuid) -> Task<Message> {
        self.config.delete_bookmark(id);
        if let Err(e) = self.config.save() {
            self.connection_form.error = Some(t_args(
                "err-failed-save-config",
                &[("error", &e.to_string())],
            ));
        }

        // Clean up bookmark_errors for deleted bookmark
        self.bookmark_errors.remove(&id);

        self.bookmark_edit = BookmarkEditState::default();
        // Restore chat scroll position when closing bookmark editor
        self.scroll_chat_if_visible(false)
    }

    // ==================== Tab Navigation ====================

    /// Handle Tab pressed in bookmark Add/Edit form.
    ///
    /// Queries iced for the actually-focused widget at Tab time
    /// (rather than the manual `focused_field` tracker), then advances
    /// to the next input. The resolved id is forwarded to
    /// [`Self::handle_bookmark_edit_tab_resolved`].
    pub fn handle_bookmark_edit_tab_pressed(&mut self) -> Task<Message> {
        super::focus::dispatch_find_focused(Message::BookmarkEditTabResolved)
    }

    pub fn handle_bookmark_edit_tab_resolved(&mut self, focused: Id) -> Task<Message> {
        // Note: Port is skipped because NumberInput handles its own Tab key.
        const CYCLE: &[InputId] = &[
            InputId::BookmarkName,
            InputId::BookmarkAddress,
            InputId::BookmarkUsername,
            InputId::BookmarkPassword,
            InputId::BookmarkNickname,
            InputId::BookmarkFingerprint,
        ];
        let next = super::focus::next_in_cycle(&focused, CYCLE);
        self.focused_field = next;
        operation::focus(Id::from(next))
    }

    // ==================== Private Helpers ====================

    /// Validate bookmark fields
    fn validate_bookmark(&self) -> Option<String> {
        if self.bookmark_edit.bookmark.name.trim().is_empty() {
            return Some(t("err-name-required"));
        }
        if self.bookmark_edit.bookmark.address.trim().is_empty() {
            return Some(t("err-address-required"));
        }
        // Optional fingerprint pin: empty/None = unpinned, otherwise must
        // match the canonical 95-char uppercase form. The handler stores
        // empty input as None, so a `Some` here always has content.
        if let Some(fp) = self
            .bookmark_edit
            .bookmark
            .certificate_fingerprint
            .as_deref()
        {
            let trimmed = fp.trim();
            if !trimmed.is_empty() && !is_canonical_fingerprint(trimmed) {
                return Some(t("err-fingerprint-invalid"));
            }
        }

        None
    }
}
