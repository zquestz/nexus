//! Tracker discovery panel input handlers.
//!
//! Covers list-view interactions (dropdown selection, search input,
//! sort), the Add / Edit / Remove sub-mode flows, and the Refresh
//! query path. AcceptFingerprint lands in step 7.

use iced::Task;
use iced::widget::Id;
use nexus_common::names::fold_name;
use uuid::Uuid;

use super::focus::{dispatch_find_focused, next_in_cycle};
use crate::NexusApp;
use crate::config::Config;
use crate::i18n::{get_locale, t, t_args};
use crate::network::types::ConnectError;
use crate::network::{
    ProxyConfig, TrackerQueryError, TrackerQueryOk, TrackerQueryParams, query_tracker,
};
use crate::types::{
    ActivePanel, ClientTracker, InputId, Message, TrackerBrowserEditInit, TrackerBrowserMode,
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
    /// Dropdown selection changed. Updates the selected tracker,
    /// resets the search input (search is selection-scoped per
    /// spec), and auto-fetches if the new selection has no cached
    /// result yet. The cache is the source of truth: if entries are
    /// already cached for the new selection, the user sees them
    /// instantly with no re-fetch.
    pub fn handle_tracker_browser_select_tracker(&mut self, id: Uuid) -> Task<Message> {
        if self.tracker_browser.selected_tracker == Some(id) {
            return Task::none();
        }
        self.tracker_browser.selected_tracker = Some(id);
        self.tracker_browser.search_input.clear();
        self.dispatch_tracker_query_if_uncached(id)
    }

    /// Search-row input changed. Live-filter only — no submit step.
    pub fn handle_tracker_browser_search_input_changed(&mut self, input: String) -> Task<Message> {
        self.focused_field = InputId::TrackerBrowserSearch;
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
        self.focus_field(InputId::TrackerBrowserAddName)
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
        self.focus_field(InputId::TrackerBrowserEditName)
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
                // Auto-fetch the newly-added tracker — no cache entry
                // exists yet, so this fires unconditionally.
                return self.dispatch_tracker_query_if_uncached(new_id);
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
                // changed (address / password / pin all affect what
                // the next query returns). Auto-fetch refreshes the
                // view with the new identity's results.
                self.tracker_browser.cache.remove(&id);
                self.tracker_browser.reset_to_list();
                return self.dispatch_tracker_query_if_uncached(id);
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
    /// re-points the dropdown selection at the alphabetically-next
    /// tracker (falling back to the previous if removing the last),
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

        // If the deletion target is the currently-selected tracker,
        // pre-compute its alphabetical neighbor so the dropdown lands
        // on something sensible after the delete (rather than going
        // empty until close/reopen). Try the next entry; fall back to
        // the previous when removing the last one. Computed BEFORE
        // delete so the full sorted list is still available.
        let next_pick: Option<Uuid> = if self.tracker_browser.selected_tracker == Some(id) {
            let mut sorted: Vec<&ClientTracker> = self.config.client_trackers.iter().collect();
            sorted.sort_by_key(|t| fold_name(&t.name));
            sorted.iter().position(|t| t.id == id).and_then(|i| {
                sorted
                    .get(i + 1)
                    .or_else(|| sorted.get(i.checked_sub(1)?))
                    .map(|t| t.id)
            })
        } else {
            None
        };

        // Capture both the row's value and its Vec index so a
        // save-failure rollback can restore it at the same position
        // rather than appending to the end (which would silently churn
        // `config.json`'s on-disk order).
        let original_index = self.config.client_trackers.iter().position(|t| t.id == id);
        let original = self.config.get_tracker(id).cloned();
        self.config.delete_tracker(id);

        match save_config(&self.config) {
            Ok(()) => {
                self.tracker_browser.cache.remove(&id);
                if self.tracker_browser.selected_tracker == Some(id) {
                    self.tracker_browser.selected_tracker = next_pick;
                    self.tracker_browser.search_input.clear();
                }
                self.tracker_browser.reset_to_list();
                // Auto-fetch the auto-selected neighbor (if any) when
                // its cache entry is empty. No-op if the user already
                // visited that tracker this session — they see the
                // cached entries instantly.
                if let Some(neighbor) = next_pick {
                    return self.dispatch_tracker_query_if_uncached(neighbor);
                }
            }
            Err(error) => {
                if let (Some(index), Some(orig)) = (original_index, original) {
                    self.config.insert_tracker(index, orig);
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
    /// Fingerprint → Name. Port (a `NumberInput`) is skipped because
    /// `iced_aw::NumberInput` consumes Tab internally.
    pub fn handle_tracker_browser_add_tab_pressed(&mut self) -> Task<Message> {
        dispatch_find_focused(Message::TrackerBrowserAddTabResolved)
    }

    pub fn handle_tracker_browser_add_tab_resolved(&mut self, focused: Id) -> Task<Message> {
        let next = next_in_cycle(&focused, TRACKER_BROWSER_ADD_CYCLE);
        self.focus_field(next)
    }

    /// Tab cycle in the Edit form. Same shape as Add.
    pub fn handle_tracker_browser_edit_tab_pressed(&mut self) -> Task<Message> {
        dispatch_find_focused(Message::TrackerBrowserEditTabResolved)
    }

    pub fn handle_tracker_browser_edit_tab_resolved(&mut self, focused: Id) -> Task<Message> {
        let next = next_in_cycle(&focused, TRACKER_BROWSER_EDIT_CYCLE);
        self.focus_field(next)
    }

    // =========================================================================
    // Refresh + query result
    // =========================================================================

    /// Refresh toolbar button — force a fresh query for the currently
    /// selected tracker even if the cache already has a result.
    /// No-op if no tracker is selected or a query is already in
    /// flight (the view also disables the button while
    /// `is_fetching`, but keyboard shortcuts could still re-enter).
    pub fn handle_tracker_browser_refresh(&mut self) -> Task<Message> {
        let Some(tracker_id) = self.tracker_browser.selected_tracker else {
            return Task::none();
        };
        self.dispatch_tracker_query(tracker_id)
    }

    /// Build params and `Task::perform` a query for `tracker_id`.
    /// Sets `is_fetching = true` and clears any prior `error` before
    /// dispatching. No-op if the tracker config is missing or a
    /// query is already in flight for this id (stale-drop guard).
    fn dispatch_tracker_query(&mut self, tracker_id: Uuid) -> Task<Message> {
        let Some(tracker) = self.config.get_tracker(tracker_id).cloned() else {
            return Task::none();
        };
        let cache = self.tracker_browser.cache.entry(tracker_id).or_default();
        if cache.is_fetching {
            return Task::none();
        }
        cache.is_fetching = true;
        cache.error = None;

        let params = TrackerQueryParams {
            address: tracker.address,
            port: tracker.port,
            password: tracker.password,
            expected_fingerprint: tracker.certificate_fingerprint,
            locale: get_locale().to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            proxy: build_proxy_config(&self.config),
        };

        Task::perform(query_tracker(params), move |result| {
            Message::TrackerQueryResult { tracker_id, result }
        })
    }

    /// Auto-fetch trigger. Dispatches a query for `tracker_id` only
    /// if the cache has no result yet — neither successful entries
    /// nor a recorded error nor an in-flight request. Used by the
    /// panel-open, dropdown-change, and post-mutation sites where
    /// "fetch on demand if we don't already know" is the desired
    /// semantic, distinct from the user-pressed Refresh which always
    /// forces a fresh query.
    pub(super) fn dispatch_tracker_query_if_uncached(&mut self, tracker_id: Uuid) -> Task<Message> {
        let needs_fetch = match self.tracker_browser.cache.get(&tracker_id) {
            None => true,
            Some(e) => e.entries.is_none() && e.error.is_none() && !e.is_fetching,
        };
        if !needs_fetch {
            return Task::none();
        }
        self.dispatch_tracker_query(tracker_id)
    }

    /// Result of a `query_tracker` task. Stale results (cache entry
    /// no longer `is_fetching`) are dropped silently — see the
    /// concurrency-policy decision in step 5: a faster Refresh or a
    /// dropdown change between dispatch and result invalidates the
    /// in-flight query, and the cache is the source of truth for
    /// "what the panel currently expects."
    pub fn handle_tracker_query_result(
        &mut self,
        tracker_id: Uuid,
        result: Result<TrackerQueryOk, TrackerQueryError>,
    ) -> Task<Message> {
        let Some(cache) = self.tracker_browser.cache.get_mut(&tracker_id) else {
            return Task::none();
        };
        if !cache.is_fetching {
            return Task::none();
        }
        cache.is_fetching = false;

        // Capture the Stage1Mismatch payload so we can transition the
        // panel into `AcceptFingerprint` mode after the cache borrow
        // is dropped. Other variants don't need post-match work.
        let mut stage1_received: Option<String> = None;

        match result {
            Ok(TrackerQueryOk {
                entries,
                observed_fingerprint,
            }) => {
                cache.entries = Some(entries);
                cache.error = None;
                cache.pending_fingerprint = None;

                // First-connect TOFU: if the configured tracker has no
                // pin yet, commit the observed fingerprint. Save
                // failure here logs but doesn't fail the query — the
                // entries are already in the cache.
                if let Some(stored) = self.config.get_tracker(tracker_id)
                    && stored.certificate_fingerprint.is_none()
                {
                    let mut updated = stored.clone();
                    updated.certificate_fingerprint = Some(observed_fingerprint);
                    self.config.update_tracker(tracker_id, updated);
                    let _ = save_config(&self.config);
                }
            }
            Err(TrackerQueryError::Stage1Mismatch { received }) => {
                cache.error = Some(t("err-tracker-query-stage1-mismatch"));
                cache.pending_fingerprint = Some(received.clone());
                stage1_received = Some(received);
            }
            Err(err) => {
                cache.error = Some(translate_tracker_query_error(&err));
                cache.pending_fingerprint = None;
            }
        }

        // Auto-enter AcceptFingerprint mode when a Stage 1 mismatch
        // lands for the currently-selected tracker. If the user
        // switched to a different tracker mid-fetch, the cache keeps
        // `pending_fingerprint` and the toolbar error; a Refresh
        // would re-trigger the dialog from the same code path.
        if let Some(received) = stage1_received
            && self.tracker_browser.selected_tracker == Some(tracker_id)
            && let Some(tracker) = self.config.get_tracker(tracker_id)
        {
            let expected = tracker.certificate_fingerprint.clone();
            let name = tracker.name.clone();
            let address = tracker.address.clone();
            let port = tracker.port;
            self.tracker_browser
                .enter_accept_fingerprint_mode(tracker_id, name, address, port, expected, received);
        }

        Task::none()
    }

    // =========================================================================
    // AcceptFingerprint
    // =========================================================================

    /// Accept the pending fingerprint: commit the observed value as
    /// the new pin, drop the cache entry, return to List mode, and
    /// dispatch a fresh forced query so the user immediately sees
    /// the listing the new pin authorises.
    pub fn handle_tracker_browser_accept_fingerprint_confirm(&mut self) -> Task<Message> {
        if self.tracker_browser.is_accept_fingerprint_submitting {
            return Task::none();
        }

        let TrackerBrowserMode::AcceptFingerprint { id, received, .. } = &self.tracker_browser.mode
        else {
            return Task::none();
        };
        let id = *id;
        let new_pin = received.clone();

        self.tracker_browser.is_accept_fingerprint_submitting = true;

        // Commit the new pin to the tracker config row.
        let Some(existing) = self.config.get_tracker(id).cloned() else {
            // Tracker was removed between dialog open and Accept.
            // Today this branch is unreachable: `handle_tracker_browser_remove_confirm`
            // is the sole deletion path and it already drops its own
            // cache entry. The defensive `cache.remove(&id)` here is a
            // no-op in normal flow but cleans up correctly if a future
            // deletion path lands without updating both sides.
            self.tracker_browser.cache.remove(&id);
            self.tracker_browser.reset_to_list();
            return Task::none();
        };
        // `existing` is cloned into `updated` so the original is still
        // owned for the rollback path below. Restoring from the
        // captured snapshot (rather than reading back the just-updated
        // row and flipping a field) is more robust: any future field
        // added to the Accept-time mutation gets rolled back too.
        let updated = ClientTracker {
            certificate_fingerprint: Some(new_pin),
            ..existing.clone()
        };
        self.config.update_tracker(id, updated);

        if let Err(error) = save_config(&self.config) {
            // Persist failed — keep the dialog open so the user can
            // retry or cancel. Restore the in-memory row to the
            // captured pre-update snapshot so memory matches disk.
            self.config.update_tracker(id, existing);
            self.tracker_browser.accept_fingerprint_error = Some(error);
            self.tracker_browser.is_accept_fingerprint_submitting = false;
            return Task::none();
        }

        // Pin committed. Drop the stale cache entry and re-query so
        // the user sees the listing the new pin authorises.
        self.tracker_browser.cache.remove(&id);
        self.tracker_browser.reset_to_list();
        self.dispatch_tracker_query(id)
    }

    /// Cancel the AcceptFingerprint dialog. Returns to the list view;
    /// the cache's `pending_fingerprint` and toolbar error stay set
    /// so the user can re-enter the dialog later via Refresh, or
    /// remove the tracker if they don't want to accept.
    pub fn handle_tracker_browser_accept_fingerprint_cancel(&mut self) -> Task<Message> {
        if self.tracker_browser.is_accept_fingerprint_submitting {
            return Task::none();
        }
        self.tracker_browser.reset_to_list();
        Task::none()
    }

    // =========================================================================
    // Click-to-connect from a tracker row
    // =========================================================================

    /// Pre-fill the global Connection form from a tracker discovery row.
    /// Triggered by clicking the Name cell or the context-menu Connect
    /// item.
    ///
    /// Form lifecycle:
    ///   1. `clear()` wipes any prior session (fields, error,
    ///      `is_connecting`, `add_bookmark`) so a previous Connect
    ///      attempt can't leak into this one.
    ///   2. The four tracker-supplied fields are written.
    ///   3. `connect_origin = Some(TrackerBrowser)` triggers the
    ///      layout's modal-like gate, rendering the form on top of the
    ///      discovery panel. Cancel restores the discovery panel by
    ///      clearing origin.
    pub fn handle_open_connect_from_tracker(
        &mut self,
        name: String,
        address: String,
        port: u16,
        fingerprint: String,
    ) -> Task<Message> {
        self.connection_form.clear();
        self.connection_form.server_name = name;
        self.connection_form.server_address = address;
        self.connection_form.port = port;
        // Normalize before storing — the protocol guarantees a canonical
        // 95-byte uppercase form so trim is a no-op in practice, but
        // this matches the bookmark-row path's defensive trim and means
        // a malformed tracker can't desync the connect-form fingerprint
        // and the canonical-form validator at submit time.
        self.connection_form.fingerprint =
            normalize_certificate_fingerprint(Some(fingerprint)).unwrap_or_default();
        self.connection_form.connect_origin = Some(ActivePanel::TrackerBrowser);
        // Close any other layout-level overlay so only the connection
        // form is visible. Mirrors `handle_open_connection_form`.
        self.dismiss_bookmark_edit();
        // Auto-focus the first field. Same rule as the manual `+` summon
        // — first field, no guessing — even though name / address /
        // port / fingerprint arrive pre-filled from the tracker row.
        self.focus_field(InputId::ServerName)
    }

    // =========================================================================
    // Row context-menu: Bookmark / Copy URI
    // =========================================================================

    /// Directly create a `ServerBookmark` from a tracker discovery row
    /// (context-menu "Bookmark" item). Username / password / nickname
    /// stay blank — the tracker only supplies discovery info, not
    /// credentials. The user can edit the bookmark later to add them.
    ///
    /// Failure modes surface as toasts (no form opens):
    ///   - Dedup collision (any field of the dedup tuple) → generic
    ///     "Bookmark already exists" toast.
    ///   - `save_config()` failure → roll back the in-memory entry
    ///     and show "Failed to save bookmark".
    ///   - Success → "Bookmark added" toast.
    pub fn handle_tracker_browser_bookmark_row(
        &mut self,
        name: String,
        address: String,
        port: u16,
        fingerprint: String,
    ) -> Task<Message> {
        // Reuse the bookmark form's dedup helper. The four tracker
        // fields plus blank username / nickname form the lookup tuple,
        // matching the convention of every other bookmark-create site.
        if super::bookmarks::check_bookmark_dedup(
            &name,
            &address,
            port,
            "",
            "",
            &self.config.bookmarks,
            None,
        )
        .is_some()
        {
            return Task::done(Message::ShowToast(t("toast-bookmark-already-exists")));
        }

        let bookmark = crate::types::ServerBookmark {
            id: Uuid::new_v4(),
            name,
            address,
            port,
            username: String::new(),
            password: String::new(),
            nickname: String::new(),
            auto_connect: false,
            certificate_fingerprint: normalize_certificate_fingerprint(Some(fingerprint)),
        };
        let new_id = bookmark.id;
        self.config.add_bookmark(bookmark);

        if save_config(&self.config).is_err() {
            // Roll back the in-memory insert so disk and memory stay in sync.
            self.config.delete_bookmark(new_id);
            return Task::done(Message::ShowToast(t("toast-bookmark-save-failed")));
        }

        Task::done(Message::ShowToast(t("toast-bookmark-added")))
    }

    /// Build a `nexus://address:port` URI for a tracker discovery row
    /// and copy it to the clipboard. Routes through
    /// `crate::uri::build_share_uri` so IPv6 hosts get bracketed and
    /// the BBS-default port is dropped — same canonical share-URI
    /// shape the Server Info panel uses. Reuses the existing
    /// `toast-link-copied` toast (same action, same wording).
    pub fn handle_tracker_browser_copy_row_uri(
        &mut self,
        address: String,
        port: u16,
    ) -> Task<Message> {
        let uri = crate::uri::build_share_uri(None, &address, port);
        iced::clipboard::write(uri).chain(Task::done(Message::ShowToast(t("toast-link-copied"))))
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
    if let Err(error) = super::tracker_form_errors::translate_tracker_field_errors(
        name,
        address,
        fingerprint,
        password,
    ) {
        return Some(error);
    }

    // -------------------- Dedup ----------------------
    // Comparison values are computed against the trimmed user input,
    // matching how the entry is normalized at storage time. Mirrors
    // the server-side trackers-table unique indexes:
    //   - LOWER(name)            → case-insensitive on name
    //   - (LOWER(address), port) → case-insensitive on address + exact port
    let name_key = fold_name(name.trim());
    let address_key = address.trim().to_lowercase();

    for entry in existing {
        if Some(entry.id) == excluding_id {
            continue;
        }
        if fold_name(entry.name.trim()) == name_key {
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

/// Trim a freeform optional field (password, fingerprint); empty
/// after trim collapses to `None` so the on-disk / wire shape stays
/// canonical. Shared with `tracker_management.rs` so both forms
/// agree on what "the user left this empty" means.
pub(super) fn optional_string(value: &str) -> Option<String> {
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

/// Build a `ProxyConfig` from the user's settings, or `None` when the
/// proxy is disabled. Mirrors the `handle_connect_pressed` shape so
/// tracker queries follow the same "respect global proxy" contract as
/// BBS connections.
fn build_proxy_config(config: &Config) -> Option<ProxyConfig> {
    config.settings.proxy.enabled.then(|| ProxyConfig {
        address: config.settings.proxy.address.clone(),
        port: config.settings.proxy.port,
        username: config.settings.proxy.username.clone(),
        password: config.settings.proxy.password.clone(),
    })
}

/// Translate a `TrackerQueryError` to a localized error string for
/// display in the panel cache. `Stage1Mismatch` is handled by the
/// caller (it routes to `pending_fingerprint`) and is not expected
/// here — covered for exhaustiveness with the same message shape.
fn translate_tracker_query_error(err: &TrackerQueryError) -> String {
    match err {
        TrackerQueryError::Stage1Mismatch { .. } => t("err-tracker-query-stage1-mismatch"),
        TrackerQueryError::Stage2Mismatch => t("err-tracker-query-stage2-intercepted"),
        // Phase-typed dispatch on the underlying ConnectError:
        //
        // - TCP-phase failures (refused, timeout, DNS) → "unreachable"
        //   (tracker is down or address is wrong).
        // - TLS-phase failure → "refused" (TCP succeeded then peer
        //   dropped during handshake — typically rate-limit /
        //   capacity / refusal).
        // - Other variants (NoCertificates, post-connect Other) fall
        //   through to "unreachable" as a fail-safe; both indicate
        //   something went wrong and the tracker isn't usable.
        TrackerQueryError::Connection(inner) => match inner {
            ConnectError::TlsHandshake { .. } => t("err-tracker-query-refused"),
            // TLS handshake timeout falls in with "unreachable" rather
            // than "refused": a peer that accepted TCP but never
            // completed TLS isn't actively refusing — it's hung or
            // network-dropped. "Refused" is reserved for the explicit
            // TLS-layer error case (rate-limit / capacity), which DOES
            // produce a peer-emitted alert.
            ConnectError::TlsHandshakeTimeout
            | ConnectError::InvalidAddress { .. }
            | ConnectError::CouldNotResolve { .. }
            | ConnectError::DnsTimeout { .. }
            | ConnectError::TcpTimeout
            | ConnectError::TcpFailed { .. }
            | ConnectError::ProxyTimeout
            | ConnectError::ProxyFailed { .. }
            | ConnectError::NoCertificates
            | ConnectError::FingerprintMismatch(_)
            | ConnectError::FingerprintInterception(_)
            | ConnectError::Other(_) => t("err-tracker-query-unreachable"),
        },
        TrackerQueryError::Handshake(e) => {
            t_args("err-tracker-query-handshake-failed", &[("error", e)])
        }
        TrackerQueryError::Protocol(e) => {
            t_args("err-tracker-query-protocol-error", &[("error", e)])
        }
        TrackerQueryError::MalformedResponse(e) => {
            t_args("err-tracker-query-malformed-response", &[("error", e)])
        }
        TrackerQueryError::Rejected { kind, message } => match kind.as_deref() {
            Some("unauthorized") => t("err-tracker-query-unauthorized"),
            Some("rate_limited") => t("err-tracker-query-rate-limited"),
            Some("capacity") => t("err-tracker-query-capacity"),
            Some("invalid") => t("err-tracker-query-invalid"),
            _ => t_args(
                "err-tracker-query-rejected",
                &[("message", message.as_str())],
            ),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tracker(name: &str, address: &str, port: u16) -> ClientTracker {
        ClientTracker {
            id: Uuid::new_v4(),
            name: name.to_string(),
            address: address.to_string(),
            port,
            ..Default::default()
        }
    }

    #[test]
    fn dedup_rejects_duplicate_name_case_insensitive() {
        let existing = vec![tracker("Public", "a.example.com", 7510)];
        let err = validate_form("public", "b.example.com", 7510, "", "", &existing, None)
            .expect("should reject duplicate name");
        assert_eq!(
            err,
            t_args("err-tracker-name-duplicate", &[("name", "public")])
        );
    }

    #[test]
    fn dedup_rejects_duplicate_address_port_case_insensitive() {
        let existing = vec![tracker("First", "tracker.example.com", 7510)];
        let err = validate_form(
            "Second",
            "Tracker.Example.COM",
            7510,
            "",
            "",
            &existing,
            None,
        )
        .expect("should reject duplicate address+port");
        assert_eq!(err, t("err-tracker-address-duplicate"));
    }

    #[test]
    fn dedup_allows_same_address_different_port() {
        let existing = vec![tracker("First", "tracker.example.com", 7510)];
        assert!(
            validate_form(
                "Second",
                "tracker.example.com",
                7520,
                "",
                "",
                &existing,
                None,
            )
            .is_none()
        );
    }

    #[test]
    fn dedup_allows_different_address_same_port() {
        let existing = vec![tracker("First", "alpha.example.com", 7510)];
        assert!(
            validate_form("Second", "beta.example.com", 7510, "", "", &existing, None,).is_none()
        );
    }

    #[test]
    fn dedup_rejects_idn_address_unicode_case_fold() {
        // SQLite's `LOWER()` is ASCII-only and would treat these as
        // distinct. Client-side `to_lowercase()` is Unicode-aware and
        // catches the IDN collision the server's index misses.
        let existing = vec![tracker("First", "MÜNCHEN.de", 7510)];
        let err = validate_form("Second", "münchen.de", 7510, "", "", &existing, None)
            .expect("should reject IDN duplicate");
        assert_eq!(err, t("err-tracker-address-duplicate"));
    }

    #[test]
    fn dedup_excludes_self_on_edit() {
        let mut existing = vec![tracker("Public", "tracker.example.com", 7510)];
        let edit_id = existing[0].id;
        // Add a second row so the loop has more than just self.
        existing.push(tracker("Other", "other.example.com", 7510));
        // Editing the row to the same name+address+port must not
        // trip on its own existing record.
        assert!(
            validate_form(
                "Public",
                "tracker.example.com",
                7510,
                "",
                "",
                &existing,
                Some(edit_id),
            )
            .is_none()
        );
    }

    // =========================================================================
    // Copy URI: IPv6 bracketing via build_share_uri
    // =========================================================================

    #[test]
    fn copy_row_uri_ipv4_uses_default_port_dropping() {
        // BBS default port (7500) is dropped per build_share_uri convention.
        let uri = crate::uri::build_share_uri(None, "203.0.113.1", 7500);
        assert_eq!(uri, "nexus://203.0.113.1");
    }

    #[test]
    fn copy_row_uri_ipv4_non_default_port_preserved() {
        let uri = crate::uri::build_share_uri(None, "203.0.113.1", 7600);
        assert_eq!(uri, "nexus://203.0.113.1:7600");
    }

    #[test]
    fn copy_row_uri_ipv6_brackets_host() {
        // IPv6 input must round-trip with brackets so the URI is parseable.
        // This is the regression guard for a `format!("nexus://{}:{}", ...)`
        // shape that would produce an ambiguous `nexus://2001:db8::1:7500`.
        let uri = crate::uri::build_share_uri(None, "2001:db8::1", 7500);
        // Default port dropped; brackets present.
        assert_eq!(uri, "nexus://[2001:db8::1]");
    }

    #[test]
    fn copy_row_uri_ipv6_with_non_default_port() {
        let uri = crate::uri::build_share_uri(None, "2001:db8::1", 7600);
        assert_eq!(uri, "nexus://[2001:db8::1]:7600");
    }

    // =========================================================================
    // translate_tracker_query_error: per-variant rendering
    // =========================================================================

    #[test]
    fn translate_stage1_mismatch_renders_localized() {
        let err = TrackerQueryError::Stage1Mismatch {
            received: "AA:BB".to_string(),
        };
        let s = translate_tracker_query_error(&err);
        assert!(!s.is_empty());
    }

    #[test]
    fn translate_stage2_mismatch_renders_localized() {
        let err = TrackerQueryError::Stage2Mismatch;
        let s = translate_tracker_query_error(&err);
        assert!(!s.is_empty());
    }

    #[test]
    fn translate_connection_tls_handshake_renders_refused() {
        let err = TrackerQueryError::Connection(ConnectError::TlsHandshake {
            error: "rate limit".to_string(),
        });
        let s = translate_tracker_query_error(&err);
        // Refused branch — locale-keyed.
        assert!(!s.is_empty());
    }

    #[test]
    fn translate_connection_tls_timeout_renders_unreachable() {
        let err = TrackerQueryError::Connection(ConnectError::TlsHandshakeTimeout);
        let s = translate_tracker_query_error(&err);
        // Timeout falls in the unreachable group.
        assert!(!s.is_empty());
    }

    #[test]
    fn translate_connection_tcp_failed_renders_unreachable() {
        let err = TrackerQueryError::Connection(ConnectError::TcpFailed {
            error: "refused".to_string(),
        });
        let s = translate_tracker_query_error(&err);
        assert!(!s.is_empty());
    }

    #[test]
    fn translate_handshake_error_includes_arg() {
        let err = TrackerQueryError::Handshake("server too old".to_string());
        let s = translate_tracker_query_error(&err);
        assert!(s.contains("server too old"));
    }

    #[test]
    fn translate_protocol_error_includes_arg() {
        let err = TrackerQueryError::Protocol("unexpected variant".to_string());
        let s = translate_tracker_query_error(&err);
        assert!(s.contains("unexpected variant"));
    }

    #[test]
    fn translate_malformed_response_includes_arg() {
        let err = TrackerQueryError::MalformedResponse("bad fingerprint".to_string());
        let s = translate_tracker_query_error(&err);
        assert!(s.contains("bad fingerprint"));
    }

    #[test]
    fn translate_rejected_unauthorized_uses_kind_specific_key() {
        let err = TrackerQueryError::Rejected {
            kind: Some("unauthorized".to_string()),
            message: "wrong password".to_string(),
        };
        let s = translate_tracker_query_error(&err);
        // Kind-specific key — message arg is NOT interpolated.
        assert!(!s.contains("wrong password"));
        assert!(!s.is_empty());
    }

    #[test]
    fn translate_rejected_unknown_kind_falls_back_to_message() {
        let err = TrackerQueryError::Rejected {
            kind: Some("future_kind".to_string()),
            message: "explanatory text".to_string(),
        };
        let s = translate_tracker_query_error(&err);
        // Fallback path interpolates the message.
        assert!(s.contains("explanatory text"));
    }

    #[test]
    fn translate_rejected_no_kind_falls_back_to_message() {
        let err = TrackerQueryError::Rejected {
            kind: None,
            message: "generic rejection".to_string(),
        };
        let s = translate_tracker_query_error(&err);
        assert!(s.contains("generic rejection"));
    }
}
