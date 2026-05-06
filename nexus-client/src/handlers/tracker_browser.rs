//! Tracker discovery panel input handlers.
//!
//! Covers list-view interactions (dropdown selection, search input,
//! sort) and the Add / Edit / Remove sub-mode flows. Refresh and
//! AcceptFingerprint land in step 5 / step 6.

use iced::Task;
use iced::widget::{Id, operation};
use nexus_common::fingerprint::is_canonical_fingerprint;
use nexus_common::validators::{
    self, MAX_PASSWORD_LENGTH, MAX_TRACKER_NAME_LENGTH, PublicAddressError, TrackerAddressError,
    TrackerNameError,
};
use uuid::Uuid;

use super::focus::{dispatch_find_focused, next_in_cycle};
use crate::NexusApp;
use crate::config::Config;
use crate::i18n::{t, t_args};
use crate::types::{
    ClientTracker, InputId, Message, TrackerBrowserEditInit, TrackerBrowserMode,
    TrackerBrowserSortColumn, normalize_certificate_fingerprint,
};

const TRACKER_BROWSER_ADD_CYCLE: &[InputId] = &[
    InputId::TrackerBrowserAddName,
    InputId::TrackerBrowserAddAddress,
    InputId::TrackerBrowserAddPassword,
    InputId::TrackerBrowserAddFingerprint,
];

const TRACKER_BROWSER_EDIT_CYCLE: &[InputId] = &[
    InputId::TrackerBrowserEditName,
    InputId::TrackerBrowserEditAddress,
    InputId::TrackerBrowserEditPassword,
    InputId::TrackerBrowserEditFingerprint,
];

impl NexusApp {
    /// Dropdown selection changed. Updates the selected tracker and
    /// resets the search input (per spec — search is selection-scoped).
    pub fn handle_tracker_browser_select_tracker(&mut self, id: Uuid) -> Task<Message> {
        if self.tracker_browser.selected_tracker == Some(id) {
            return Task::none();
        }
        self.tracker_browser.selected_tracker = Some(id);
        self.tracker_browser.search_input.clear();
        Task::none()
    }

    /// Search-row input changed. Live-filter only — no submit step.
    pub fn handle_tracker_browser_search_input_changed(&mut self, input: String) -> Task<Message> {
        self.tracker_browser.search_input = input;
        Task::none()
    }

    /// Sortable table column header clicked. Same-column click toggles
    /// direction; different-column click switches to that column with
    /// ascending order. Matches the convention used by every other
    /// sortable table in the app.
    pub fn handle_tracker_browser_sort_changed(
        &mut self,
        column: TrackerBrowserSortColumn,
    ) -> Task<Message> {
        if self.tracker_browser.sort_column == column {
            self.tracker_browser.sort_ascending = !self.tracker_browser.sort_ascending;
        } else {
            self.tracker_browser.sort_column = column;
            self.tracker_browser.sort_ascending = true;
        }
        Task::none()
    }

    // =========================================================================
    // Mode transitions
    // =========================================================================

    /// [+] toolbar button — open the Add subview and focus the Name field.
    pub fn handle_tracker_browser_show_add(&mut self) -> Task<Message> {
        self.tracker_browser.enter_add_mode();
        self.focused_field = InputId::TrackerBrowserAddName;
        operation::focus(Id::from(InputId::TrackerBrowserAddName))
    }

    /// Edit toolbar button — open the Edit subview pre-populated from
    /// the currently-selected tracker. No-op if no tracker is selected.
    pub fn handle_tracker_browser_show_edit(&mut self) -> Task<Message> {
        let Some(id) = self.tracker_browser.selected_tracker else {
            return Task::none();
        };
        let Some(t) = self.config.get_tracker(id) else {
            return Task::none();
        };
        self.tracker_browser
            .enter_edit_mode(TrackerBrowserEditInit {
                id: t.id,
                name: t.name.clone(),
                address: t.address.clone(),
                port: t.port,
                password: t.password.clone(),
                fingerprint: t.certificate_fingerprint.clone(),
            });
        self.focused_field = InputId::TrackerBrowserEditName;
        operation::focus(Id::from(InputId::TrackerBrowserEditName))
    }

    /// Remove toolbar button — open the ConfirmRemove modal for the
    /// currently-selected tracker. No-op if no tracker is selected.
    pub fn handle_tracker_browser_show_remove(&mut self) -> Task<Message> {
        let Some(id) = self.tracker_browser.selected_tracker else {
            return Task::none();
        };
        let Some(t) = self.config.get_tracker(id) else {
            return Task::none();
        };
        let name = t.name.clone();
        self.tracker_browser.enter_confirm_remove_mode(id, name);
        Task::none()
    }

    /// Cancel from any sub-mode. Resets to the list view.
    pub fn handle_tracker_browser_cancel_mode(&mut self) -> Task<Message> {
        self.tracker_browser.reset_to_list();
        Task::none()
    }

    // =========================================================================
    // Add form
    // =========================================================================

    pub fn handle_tracker_browser_add_name_changed(&mut self, name: String) -> Task<Message> {
        self.focused_field = InputId::TrackerBrowserAddName;
        self.tracker_browser.add_name = name;
        Task::none()
    }

    pub fn handle_tracker_browser_add_address_changed(&mut self, address: String) -> Task<Message> {
        self.focused_field = InputId::TrackerBrowserAddAddress;
        self.tracker_browser.add_address = address;
        Task::none()
    }

    pub fn handle_tracker_browser_add_port_changed(&mut self, port: u16) -> Task<Message> {
        self.focused_field = InputId::TrackerBrowserAddPort;
        self.tracker_browser.add_port = port;
        Task::none()
    }

    pub fn handle_tracker_browser_add_password_changed(
        &mut self,
        password: String,
    ) -> Task<Message> {
        self.focused_field = InputId::TrackerBrowserAddPassword;
        self.tracker_browser.add_password = password;
        Task::none()
    }

    pub fn handle_tracker_browser_add_fingerprint_changed(
        &mut self,
        fingerprint: String,
    ) -> Task<Message> {
        self.focused_field = InputId::TrackerBrowserAddFingerprint;
        self.tracker_browser.add_fingerprint = fingerprint;
        Task::none()
    }

    /// Add form submit. Validates → builds `ClientTracker` → persists
    /// → selects the new entry → returns to list. The cache is left
    /// untouched (no entry exists yet for the new id; step 5's
    /// auto-fetch will populate it on first visit).
    pub fn handle_tracker_browser_add_submit(&mut self) -> Task<Message> {
        if self.tracker_browser.is_submitting {
            return Task::none();
        }
        if let Some(error) = validate_form(
            &self.tracker_browser.add_name,
            &self.tracker_browser.add_address,
            self.tracker_browser.add_port,
            &self.tracker_browser.add_fingerprint,
            &self.tracker_browser.add_password,
            &self.config.client_trackers,
            None,
        ) {
            self.tracker_browser.form_error = Some(error);
            return Task::none();
        }

        self.tracker_browser.is_submitting = true;
        let new = ClientTracker {
            id: Uuid::new_v4(),
            name: self.tracker_browser.add_name.trim().to_string(),
            address: self.tracker_browser.add_address.trim().to_string(),
            port: self.tracker_browser.add_port,
            password: optional_string(&self.tracker_browser.add_password),
            certificate_fingerprint: normalize_certificate_fingerprint(Some(
                self.tracker_browser.add_fingerprint.clone(),
            )),
        };
        let new_id = new.id;
        self.config.add_tracker(new);

        match save_config(&self.config) {
            Ok(()) => {
                self.tracker_browser.selected_tracker = Some(new_id);
                self.tracker_browser.reset_to_list();
            }
            Err(error) => {
                // Persistence failed — keep the form open so the user
                // can retry. Roll back the in-memory add to keep the
                // config consistent with disk.
                self.config.delete_tracker(new_id);
                self.tracker_browser.form_error = Some(error);
                self.tracker_browser.is_submitting = false;
            }
        }
        Task::none()
    }

    /// Validate-then-submit fallback for Enter on an incomplete Add
    /// form. Surfaces the localized validation error in the form
    /// banner without attempting persistence.
    pub fn handle_tracker_browser_validate_add(&mut self) -> Task<Message> {
        if let Some(error) = validate_form(
            &self.tracker_browser.add_name,
            &self.tracker_browser.add_address,
            self.tracker_browser.add_port,
            &self.tracker_browser.add_fingerprint,
            &self.tracker_browser.add_password,
            &self.config.client_trackers,
            None,
        ) {
            self.tracker_browser.form_error = Some(error);
        }
        Task::none()
    }

    // =========================================================================
    // Edit form
    // =========================================================================

    pub fn handle_tracker_browser_edit_name_changed(&mut self, value: String) -> Task<Message> {
        self.focused_field = InputId::TrackerBrowserEditName;
        if let TrackerBrowserMode::Edit { name, .. } = &mut self.tracker_browser.mode {
            *name = value;
        }
        Task::none()
    }

    pub fn handle_tracker_browser_edit_address_changed(&mut self, value: String) -> Task<Message> {
        self.focused_field = InputId::TrackerBrowserEditAddress;
        if let TrackerBrowserMode::Edit { address, .. } = &mut self.tracker_browser.mode {
            *address = value;
        }
        Task::none()
    }

    pub fn handle_tracker_browser_edit_port_changed(&mut self, value: u16) -> Task<Message> {
        self.focused_field = InputId::TrackerBrowserEditPort;
        if let TrackerBrowserMode::Edit { port, .. } = &mut self.tracker_browser.mode {
            *port = value;
        }
        Task::none()
    }

    pub fn handle_tracker_browser_edit_password_changed(&mut self, value: String) -> Task<Message> {
        self.focused_field = InputId::TrackerBrowserEditPassword;
        if let TrackerBrowserMode::Edit { password, .. } = &mut self.tracker_browser.mode {
            *password = value;
        }
        Task::none()
    }

    pub fn handle_tracker_browser_edit_fingerprint_changed(
        &mut self,
        value: String,
    ) -> Task<Message> {
        self.focused_field = InputId::TrackerBrowserEditFingerprint;
        if let TrackerBrowserMode::Edit { fingerprint, .. } = &mut self.tracker_browser.mode {
            *fingerprint = value;
        }
        Task::none()
    }

    /// Edit form submit. Validates, builds replacement, persists,
    /// drops the cache entry (the row's meaningful identity may have
    /// changed — address/port/password/pin all affect what the next
    /// fetch will return), and returns to list.
    pub fn handle_tracker_browser_edit_submit(&mut self) -> Task<Message> {
        if self.tracker_browser.is_submitting {
            return Task::none();
        }

        let TrackerBrowserMode::Edit {
            id,
            name,
            address,
            port,
            password,
            fingerprint,
            ..
        } = &self.tracker_browser.mode
        else {
            return Task::none();
        };
        let id = *id;
        let port = *port;
        let name = name.clone();
        let address = address.clone();
        let password = password.clone();
        let fingerprint = fingerprint.clone();

        if let Some(error) = validate_form(
            &name,
            &address,
            port,
            &fingerprint,
            &password,
            &self.config.client_trackers,
            Some(id),
        ) {
            self.tracker_browser.form_error = Some(error);
            return Task::none();
        }

        self.tracker_browser.is_submitting = true;
        let original = self.config.get_tracker(id).cloned();
        let updated = ClientTracker {
            id,
            name: name.trim().to_string(),
            address: address.trim().to_string(),
            port,
            password: optional_string(&password),
            certificate_fingerprint: normalize_certificate_fingerprint(Some(fingerprint)),
        };
        self.config.update_tracker(id, updated);

        match save_config(&self.config) {
            Ok(()) => {
                // Drop the cache entry — meaningful identity may have
                // changed. Step 5's auto-fetch will populate fresh.
                self.tracker_browser.cache.remove(&id);
                self.tracker_browser.reset_to_list();
            }
            Err(error) => {
                if let Some(orig) = original {
                    self.config.update_tracker(id, orig);
                }
                self.tracker_browser.form_error = Some(error);
                self.tracker_browser.is_submitting = false;
            }
        }
        Task::none()
    }

    pub fn handle_tracker_browser_validate_edit(&mut self) -> Task<Message> {
        let TrackerBrowserMode::Edit {
            id,
            name,
            address,
            port,
            password,
            fingerprint,
            ..
        } = &self.tracker_browser.mode
        else {
            return Task::none();
        };
        let id = *id;
        let port = *port;
        // Clone the inputs we need so the immutable borrow on
        // `self.tracker_browser.mode` ends before we hand the slice
        // to `validate_form` (which would otherwise create
        // overlapping borrows on `self`).
        let name = name.clone();
        let address = address.clone();
        let password = password.clone();
        let fingerprint = fingerprint.clone();
        if let Some(error) = validate_form(
            &name,
            &address,
            port,
            &fingerprint,
            &password,
            &self.config.client_trackers,
            Some(id),
        ) {
            self.tracker_browser.form_error = Some(error);
        }
        Task::none()
    }

    // =========================================================================
    // ConfirmRemove modal
    // =========================================================================

    /// Confirm removal — deletes from config, drops cache entry,
    /// clears the dropdown selection if it pointed at this row,
    /// persists, returns to list.
    pub fn handle_tracker_browser_remove_confirm(&mut self) -> Task<Message> {
        if self.tracker_browser.is_remove_submitting {
            return Task::none();
        }
        let TrackerBrowserMode::ConfirmRemove { id, .. } = &self.tracker_browser.mode else {
            return Task::none();
        };
        let id = *id;
        self.tracker_browser.is_remove_submitting = true;
        let original = self.config.get_tracker(id).cloned();
        self.config.delete_tracker(id);

        match save_config(&self.config) {
            Ok(()) => {
                self.tracker_browser.cache.remove(&id);
                if self.tracker_browser.selected_tracker == Some(id) {
                    self.tracker_browser.selected_tracker = None;
                    self.tracker_browser.search_input.clear();
                }
                self.tracker_browser.reset_to_list();
            }
            Err(error) => {
                if let Some(orig) = original {
                    self.config.add_tracker(orig);
                }
                self.tracker_browser.remove_error = Some(error);
                self.tracker_browser.is_remove_submitting = false;
            }
        }
        Task::none()
    }

    // =========================================================================
    // Tab navigation
    // =========================================================================

    /// Tab cycle in the Add form: Name → Address → Password →
    /// Fingerprint → Name. Port (a `NumberInput`) is skipped — it
    /// consumes Tab internally per CLAUDE.md UI Quirks.
    pub fn handle_tracker_browser_add_tab_pressed(&mut self) -> Task<Message> {
        dispatch_find_focused(Message::TrackerBrowserAddTabResolved)
    }

    pub fn handle_tracker_browser_add_tab_resolved(&mut self, focused: Id) -> Task<Message> {
        let next = next_in_cycle(&focused, TRACKER_BROWSER_ADD_CYCLE);
        self.focused_field = next;
        operation::focus(Id::from(next))
    }

    /// Tab cycle in the Edit form. Same shape as Add.
    pub fn handle_tracker_browser_edit_tab_pressed(&mut self) -> Task<Message> {
        dispatch_find_focused(Message::TrackerBrowserEditTabResolved)
    }

    pub fn handle_tracker_browser_edit_tab_resolved(&mut self, focused: Id) -> Task<Message> {
        let next = next_in_cycle(&focused, TRACKER_BROWSER_EDIT_CYCLE);
        self.focused_field = next;
        operation::focus(Id::from(next))
    }
}

// =============================================================================
// Helpers
// =============================================================================

/// Validate the user-supplied tracker form fields. Returns the first
/// localized error encountered, or `None` if the form is acceptable.
///
/// Field-level rules mirror `nexus-server/src/handlers/tracker_management.rs::validate_tracker_form`
/// so the discovery panel rejects the same inputs the BBS-admin panel
/// rejects (whitespace in addresses, brackets, scheme prefixes, etc.).
///
/// Dedup rules mirror the server's trackers-table unique indexes
/// (`(LOWER(address), port)` and `LOWER(name)`) — same comparison
/// semantics, same surface error i18n keys. `existing` is the
/// current configured-trackers list; `excluding_id` is the row
/// being edited (or `None` for Add) so an Edit doesn't conflict
/// with itself.
fn validate_form(
    name: &str,
    address: &str,
    port: u16,
    fingerprint: &str,
    password: &str,
    existing: &[ClientTracker],
    excluding_id: Option<Uuid>,
) -> Option<String> {
    if let Err(e) = validators::validate_tracker_name(name) {
        return Some(match e {
            TrackerNameError::Empty => t("err-tracker-name-empty"),
            TrackerNameError::TooLong => t_args(
                "err-tracker-name-too-long",
                &[("max", &MAX_TRACKER_NAME_LENGTH.to_string())],
            ),
            TrackerNameError::ContainsNewlines => t("err-tracker-name-contains-newlines"),
            TrackerNameError::InvalidCharacters => t("err-tracker-name-invalid-characters"),
        });
    }

    if let Err(e) = validators::validate_tracker_address(address) {
        return Some(match e {
            TrackerAddressError::Empty => t("err-tracker-address-empty"),
            TrackerAddressError::Invalid(inner) => match inner {
                PublicAddressError::TooLong => t_args(
                    "err-tracker-address-too-long",
                    &[("max", &validators::MAX_PUBLIC_ADDRESS_LENGTH.to_string())],
                ),
                PublicAddressError::ContainsScheme => t("err-tracker-address-contains-scheme"),
                PublicAddressError::ContainsBrackets => t("err-tracker-address-contains-brackets"),
                PublicAddressError::ContainsPath => t("err-tracker-address-contains-path"),
                PublicAddressError::ContainsUserinfo => t("err-tracker-address-contains-userinfo"),
                PublicAddressError::ContainsWhitespace => {
                    t("err-tracker-address-contains-whitespace")
                }
                PublicAddressError::ContainsPort => t("err-tracker-address-contains-port"),
                PublicAddressError::ContainsZoneId => t("err-tracker-address-contains-zone-id"),
                PublicAddressError::InvalidFormat => t("err-tracker-address-invalid-format"),
            },
        });
    }

    let fingerprint_trimmed = fingerprint.trim();
    if !fingerprint_trimmed.is_empty() && !is_canonical_fingerprint(fingerprint_trimmed) {
        return Some(t("err-tracker-fingerprint-invalid"));
    }

    if password.len() > MAX_PASSWORD_LENGTH {
        return Some(t_args(
            "err-tracker-password-too-long",
            &[("max", &MAX_PASSWORD_LENGTH.to_string())],
        ));
    }

    // -------------------- Dedup ----------------------
    // Comparison values are computed against the trimmed user input,
    // matching how the entry is normalized at storage time. Mirrors
    // the server-side trackers-table unique indexes:
    //   - LOWER(name)            → case-insensitive on name
    //   - (LOWER(address), port) → case-insensitive on address + exact port
    let name_key = name.trim().to_lowercase();
    let address_key = address.trim().to_lowercase();

    for entry in existing {
        if Some(entry.id) == excluding_id {
            continue;
        }
        if entry.name.trim().to_lowercase() == name_key {
            return Some(t_args(
                "err-tracker-name-duplicate",
                &[("name", name.trim())],
            ));
        }
        if entry.address.trim().to_lowercase() == address_key && entry.port == port {
            return Some(t("err-tracker-address-duplicate"));
        }
    }

    None
}

/// Trim a freeform optional field (password); empty after trim
/// collapses to `None` so the on-disk shape stays canonical.
fn optional_string(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Persist the config to disk. Wraps `Config::save` so the handlers
/// have a single error-handling path.
fn save_config(config: &Config) -> Result<(), String> {
    config.save()
}
