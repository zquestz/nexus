//! UI panel management and toggles

use crate::NexusApp;
use crate::types::{ActivePanel, InputId, Message};
use iced::Task;
use iced::widget::{Id, markdown, operation};

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

    /// Open a URL in the default browser
    pub fn handle_open_url(&mut self, url: markdown::Uri) -> Task<Message> {
        let _ = open::that(url.as_str());
        Task::none()
    }

    // ==================== Server Info ====================

    /// Show Server Info panel
    pub fn handle_show_server_info(&mut self) -> Task<Message> {
        if self.active_panel() == ActivePanel::ServerInfo {
            return Task::none();
        }

        self.set_active_panel(ActivePanel::ServerInfo);
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

    /// Close User Info panel
    pub fn handle_close_user_info(&mut self) -> Task<Message> {
        self.handle_show_chat_view()
    }

    // ==================== Panel Toggles ====================

    /// Show Add User panel (does nothing if already shown)
    pub fn handle_toggle_add_user(&mut self) -> Task<Message> {
        if self.active_panel() == ActivePanel::AddUser {
            return Task::none();
        }

        self.set_active_panel(ActivePanel::AddUser);

        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        conn.user_management.clear_add_user();
        self.focused_field = InputId::AdminUsername;
        operation::focus(Id::from(InputId::AdminUsername))
    }

    /// Show Edit User panel (does nothing if already shown)
    ///
    /// If `username` is provided, pre-fills the username field.
    pub fn handle_toggle_edit_user(&mut self, username: Option<String>) -> Task<Message> {
        if self.active_panel() == ActivePanel::EditUser {
            return Task::none();
        }

        self.set_active_panel(ActivePanel::EditUser);

        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        conn.user_management.start_editing(username);
        self.focused_field = InputId::EditUsername;
        operation::focus(Id::from(InputId::EditUsername))
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
