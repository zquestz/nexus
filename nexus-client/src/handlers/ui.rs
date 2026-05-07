//! UI panel management and toggles

use iced::Task;
use iced::widget::markdown;
use nexus_common::protocol::ClientMessage;

use crate::NexusApp;
use crate::types::{ActivePanel, InputId, Message, PendingRequests};

impl NexusApp {
    // ==================== Active Panel Helpers ====================

    /// Get the effective active panel.
    ///
    /// When connected, returns the connection's active panel.
    /// When not connected, returns the app-wide panel from ui_state.
    pub fn active_panel(&self) -> ActivePanel {
        self.active_connection
            .and_then(|id| self.connections.get(&id))
            .map(|conn| conn.active_panel)
            .unwrap_or(self.ui_state.active_panel)
    }

    /// Set the active panel.
    ///
    /// When connected, stores in the connection (all panels are per-connection).
    /// When not connected, stores in ui_state (only Settings/About make sense).
    pub fn set_active_panel(&mut self, panel: ActivePanel) {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            conn.active_panel = panel;
        } else {
            // Not connected - only Settings/About/None make sense
            self.ui_state.active_panel = panel;
        }
    }

    // ==================== About ====================

    /// Show About panel (does nothing if already shown)
    pub fn handle_show_about(&mut self) -> Task<Message> {
        if self.active_panel() == ActivePanel::About {
            return Task::none();
        }

        self.set_active_panel(ActivePanel::About);
        Task::none()
    }

    /// Close About panel
    pub fn handle_close_about(&mut self) -> Task<Message> {
        self.handle_show_chat_view()
    }

    /// Open a URL in the default browser or handle nexus:// URIs internally
    pub fn handle_open_url(&mut self, url: markdown::Uri) -> Task<Message> {
        let url_str = url.as_str();

        // Check if this is a nexus:// URI - handle internally
        if crate::uri::is_nexus_uri(url_str) {
            if let Ok(parsed) = crate::uri::parse(url_str) {
                return self.handle_nexus_uri(parsed);
            }
            // Failed to parse - just ignore
            return Task::none();
        }

        // Regular URL - open in browser
        let _ = open::that(url_str);
        Task::none()
    }

    // ==================== Server Info ====================

    /// Show Server Info panel.
    ///
    /// If the user has `tracker_list` permission, kick off a `TrackerList`
    /// fetch in the background so the Trackers tab is populated when the
    /// admin switches to it.
    pub fn handle_show_server_info(&mut self) -> Task<Message> {
        if self.active_panel() == ActivePanel::ServerInfo {
            return Task::none();
        }

        self.set_active_panel(ActivePanel::ServerInfo);

        // Reset tracker management mode and prefetch the list if allowed.
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            conn.tracker_management.reset_to_list();
            if conn.has_permission(crate::views::constants::PERMISSION_TRACKER_LIST) {
                conn.tracker_management.all_trackers = None;
                match conn.send(ClientMessage::TrackerList) {
                    Ok(message_id) => {
                        conn.pending_requests.track(
                            message_id,
                            crate::types::ResponseRouting::PopulateTrackerManagementList,
                        );
                    }
                    Err(e) => {
                        conn.tracker_management.all_trackers =
                            Some(Err(format!("{}: {}", crate::i18n::t("err-send-failed"), e)));
                    }
                }
            }
        }

        Task::none()
    }

    /// Close Server Info panel
    ///
    /// Also clears any active edit state.
    pub fn handle_close_server_info(&mut self) -> Task<Message> {
        // Clear edit state if present
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            conn.server_info_edit = None;
        }
        self.handle_show_chat_view()
    }

    // ==================== User Info ====================

    /// Close User Info panel and return to previous panel if set
    pub fn handle_close_user_info(&mut self) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            // Check if we should return to a different panel (e.g., ConnectionMonitor)
            if let Some(return_panel) = conn.user_info_return_panel.take() {
                conn.active_panel = return_panel;
                return Task::none();
            }
        }
        self.handle_show_chat_view()
    }

    // ==================== Transfers ====================

    /// Toggle Transfers panel
    ///
    /// Transfers is a global panel (not per-connection) that shows all
    /// file transfers across all connections. It can be opened even when
    /// not connected to any server. Clicking the toolbar button while
    /// Transfers is already active is a no-op (matches the behavior of
    /// every other toolbar panel — Settings / Broadcast / News / Files /
    /// Connection Monitor / User Management).
    pub fn handle_toggle_transfers(&mut self) -> Task<Message> {
        if self.active_panel() == ActivePanel::Transfers {
            return Task::none();
        }
        self.set_active_panel(ActivePanel::Transfers);
        Task::none()
    }

    /// Close Transfers panel
    pub fn handle_close_transfers(&mut self) -> Task<Message> {
        self.handle_show_chat_view()
    }

    // ==================== Tracker Browser ====================

    /// Toggle the tracker discovery panel.
    ///
    /// The tracker discovery panel is global (not per-connection) and
    /// always available regardless of connection state — every user
    /// manages their own client tracker list. Clicking the toolbar
    /// button while the panel is already active is a no-op (matches
    /// every other toolbar panel).
    ///
    /// On open, if no tracker is currently selected (first open in the
    /// session, or the previously-selected tracker has been removed),
    /// pick the alphabetically-first configured tracker so the panel
    /// surfaces something immediately. Last-selected is preserved
    /// across close/reopen within the session.
    pub fn handle_toggle_tracker_browser(&mut self) -> Task<Message> {
        if self.active_panel() == ActivePanel::TrackerBrowser {
            return Task::none();
        }
        // Drop a stale selection (the row may have been removed
        // while the panel was closed).
        if let Some(id) = self.tracker_browser.selected_tracker
            && self.config.get_tracker(id).is_none()
        {
            self.tracker_browser.selected_tracker = None;
        }
        // `search_input` is cleared only when toggling open *and*
        // auto-selecting a tracker (so the user lands on a meaningful
        // selection rather than a filtered-out list). Toggling back
        // open with an existing selection preserves the prior search
        // term — different from the Remove path, which always clears
        // because the row backing any active filter context is gone.
        if self.tracker_browser.selected_tracker.is_none()
            && let Some(id) = self
                .config
                .client_trackers
                .iter()
                .min_by_key(|t| t.name.to_lowercase())
                .map(|t| t.id)
        {
            self.tracker_browser.selected_tracker = Some(id);
            self.tracker_browser.search_input.clear();
        }
        self.set_active_panel(ActivePanel::TrackerBrowser);
        // Auto-focus the search input on open, mirroring the
        // auto-focus convention used by other forms (Group Create
        // → name, User Create → username, etc.).
        self.focused_field = InputId::TrackerBrowserSearch;
        iced::widget::operation::focus(iced::widget::Id::from(InputId::TrackerBrowserSearch))
    }

    // ==================== Sidebar Toggles ====================

    /// Toggle bookmarks sidebar visibility
    pub fn handle_toggle_bookmarks(&mut self) -> Task<Message> {
        self.ui_state.show_bookmarks = !self.ui_state.show_bookmarks;
        self.scroll_chat_if_visible(false)
    }

    /// Toggle user list sidebar visibility
    pub fn handle_toggle_user_list(&mut self) -> Task<Message> {
        self.ui_state.show_user_list = !self.ui_state.show_user_list;
        self.scroll_chat_if_visible(false)
    }
}
