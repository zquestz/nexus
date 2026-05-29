//! Bookmark management

use iced::Task;
use iced::widget::Id;
use nexus_common::names::fold_name;
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
        self.focused_field = InputId::BookmarkName;
        Task::none()
    }

    /// Handle bookmark password field change
    pub fn handle_bookmark_password_changed(&mut self, password: String) -> Task<Message> {
        self.bookmark_edit.bookmark.password = password;
        self.focused_field = InputId::BookmarkPassword;
        Task::none()
    }

    /// Handle bookmark port field change
    pub fn handle_bookmark_port_changed(&mut self, port: u16) -> Task<Message> {
        self.bookmark_edit.bookmark.port = port;
        self.focused_field = InputId::BookmarkPort;
        Task::none()
    }

    /// Handle bookmark username field change
    pub fn handle_bookmark_username_changed(&mut self, username: String) -> Task<Message> {
        self.bookmark_edit.bookmark.username = username;
        self.focused_field = InputId::BookmarkUsername;
        Task::none()
    }

    /// Handle bookmark nickname field change
    pub fn handle_bookmark_nickname_changed(&mut self, nickname: String) -> Task<Message> {
        self.bookmark_edit.bookmark.nickname = nickname;
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
        self.focused_field = InputId::BookmarkFingerprint;
        Task::none()
    }

    // ==================== Dialog Actions ====================

    /// Close the bookmark editor overlay. Wipes the dialog state so
    /// re-opening starts fresh.
    ///
    /// Used by:
    ///   - `set_active_panel` — every panel switch dismisses overlays.
    ///   - `handle_successful_connection` — connection completed,
    ///     overlay no longer relevant.
    ///   - `handle_open_connection_form` /
    ///     `handle_open_connect_from_tracker` — connection form is
    ///     the new gesture; close the bookmark editor so the
    ///     connection form is the only overlay visible.
    ///
    /// Does NOT touch `focused_field`. Callers know what's being
    /// shown next and set focus via `focus_field` afterward. A
    /// blanket reset here would be wrong — there's no single right
    /// target across all callers.
    pub(crate) fn dismiss_bookmark_edit(&mut self) {
        self.bookmark_edit = BookmarkEditState::default();
    }

    /// Cancel bookmark editing and close the dialog
    pub fn handle_cancel_bookmark_edit(&mut self) -> Task<Message> {
        self.bookmark_edit = BookmarkEditState::default();
        // Restore chat scroll position when closing bookmark editor
        self.scroll_chat_if_visible(false)
    }

    /// Validate the bookmark form (Enter when can_save is false).
    /// Sets the form-level error so the user sees what's wrong
    /// without trying to submit.
    pub fn handle_validate_bookmark_edit(&mut self) -> Task<Message> {
        if let Some(error) = validate_bookmark_form(&self.bookmark_edit, &self.config.bookmarks) {
            self.bookmark_edit.error = Some(error);
        }
        Task::none()
    }

    /// Save the current bookmark (add or update)
    pub fn handle_save_bookmark(&mut self) -> Task<Message> {
        if self.bookmark_edit.is_submitting || self.bookmark_edit.edit_stale {
            return Task::none();
        }

        // Field-shape + dedup validation. Shared with
        // `handle_validate_bookmark_edit` so the validate-then-submit
        // path produces the same error as actually submitting an
        // invalid form.
        if let Some(error) = validate_bookmark_form(&self.bookmark_edit, &self.config.bookmarks) {
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
    ///
    /// No-op if the form is already in Add mode — re-pressing the
    /// "Add bookmark" button while typing into a fresh Add form is
    /// almost always an accidental double-click; wiping in-flight
    /// state would surprise the user. Edit mode is a different
    /// gesture (clicked a row's pencil) and is allowed to switch
    /// into Add — same as clicking a different row's pencil while
    /// editing switches the Edit target.
    pub fn handle_show_add_bookmark(&mut self) -> Task<Message> {
        if matches!(self.bookmark_edit.mode, BookmarkEditMode::Add) {
            // Refocus the first field — clicking the "Add bookmark"
            // button itself moves iced focus off the currently-focused
            // input, so a literal `Task::none()` would leave the
            // cursor stranded on the button. First-field is
            // deterministic regardless of whether the user got to
            // their previous field via Tab or via click.
            return self.focus_field(InputId::BookmarkName);
        }
        // Close the connection form if it was summoned — the user
        // picked a different overlay; same principle as a toolbar
        // panel switch dismissing both overlays.
        self.dismiss_connection_form();
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

        self.focus_field(InputId::BookmarkName)
    }

    /// Show the edit bookmark dialog for a specific bookmark
    pub fn handle_show_edit_bookmark(&mut self, id: Uuid) -> Task<Message> {
        // `cloned()` releases the `&self.config` borrow before
        // `dismiss_connection_form` and the subsequent `&mut self`
        // writes need it.
        if let Some(bookmark) = self.config.get_bookmark(id).cloned() {
            // Close the connection form if it was summoned — the
            // user picked a different overlay.
            self.dismiss_connection_form();
            self.bookmark_edit = BookmarkEditState::default();
            self.bookmark_edit.mode = BookmarkEditMode::Edit(id);
            self.bookmark_edit.bookmark = bookmark;

            // Move any connection error to the edit dialog (acknowledges and clears it)
            self.bookmark_edit.error = self.bookmark_errors.remove(&id);

            return self.focus_field(InputId::BookmarkName);
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
        self.focus_field(next)
    }
}

/// Run client-side validation on the bookmark form. Returns the first
/// localized error encountered (field-shape error first, then dedup
/// collision), or `None` if the form passes both gates.
///
/// Shared between `handle_save_bookmark` and
/// `handle_validate_bookmark_edit` so the validate-then-submit path
/// surfaces the same error the actual submit would produce.
fn validate_bookmark_form(
    state: &crate::types::BookmarkEditState,
    bookmarks: &[crate::types::ServerBookmark],
) -> Option<String> {
    let bookmark = &state.bookmark;
    if let Err(error) = super::server_form_errors::translate_server_form_errors(
        &bookmark.name,
        &bookmark.address,
        &bookmark.username,
        &bookmark.nickname,
        &bookmark.password,
        bookmark.certificate_fingerprint.as_deref().unwrap_or(""),
    ) {
        return Some(error);
    }
    let edit_id = match state.mode {
        BookmarkEditMode::Edit(id) => Some(id),
        _ => None,
    };
    check_bookmark_dedup(
        &bookmark.name,
        &bookmark.address,
        bookmark.port,
        &bookmark.username,
        &bookmark.nickname,
        bookmarks,
        edit_id,
    )
}

/// Check whether a candidate bookmark would collide with the existing
/// list. Returns the localized error string on collision, or `None`
/// if the candidate is unique.
///
/// Comparison values are computed against the trimmed candidate using
/// Unicode-aware case folding (matches the tracker form dedup
/// pattern). The endpoint key is the
/// `(address, port, username, nickname)` tuple so two bookmarks that
/// resolve to the same login *and the same display identity* can't
/// coexist — distinct nicknames against a shared account remain
/// distinct bookmarks. Password is intentionally not part of the key.
///
/// `excluding_id` is `Some(id)` for Edit-mode submissions so the row
/// being edited doesn't trip on itself.
///
/// Used by both `handle_save_bookmark` (Add/Edit form) and
/// `handle_connect_pressed` (when "Add bookmark" is checked) so the
/// auto-add path enforces the same dedup contract as the manual form.
pub(super) fn check_bookmark_dedup(
    name: &str,
    address: &str,
    port: u16,
    username: &str,
    nickname: &str,
    existing: &[crate::types::ServerBookmark],
    excluding_id: Option<Uuid>,
) -> Option<String> {
    let name_key = fold_name(name.trim());
    let address_key = address.trim().to_lowercase();
    let username_key = fold_name(username.trim());
    let nickname_key = fold_name(nickname.trim());
    for entry in existing {
        if Some(entry.id) == excluding_id {
            continue;
        }
        if fold_name(entry.name.trim()) == name_key {
            return Some(t_args(
                "err-bookmark-name-duplicate",
                &[("name", name.trim())],
            ));
        }
        if entry.address.trim().to_lowercase() == address_key
            && entry.port == port
            && fold_name(entry.username.trim()) == username_key
            && fold_name(entry.nickname.trim()) == nickname_key
        {
            return Some(t("err-bookmark-endpoint-duplicate"));
        }
    }
    None
}
