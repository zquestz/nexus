//! Connection Monitor panel handlers

use iced::Task;
use nexus_common::protocol::ClientMessage;

use crate::NexusApp;
use crate::i18n::t;
use crate::types::{
    ActivePanel, ConnectionMonitorSortColumn, ConnectionMonitorTab, DisconnectAction,
    DisconnectDialogState, Message, PendingRequests, ResponseRouting, TransferSortColumn,
};
use crate::views::constants::{PERMISSION_BAN_CREATE, PERMISSION_USER_INFO, PERMISSION_USER_KICK};

impl NexusApp {
    /// Toggle the Connection Monitor panel
    ///
    /// When opening, requests connection data from the server.
    pub fn handle_toggle_connection_monitor(&mut self) -> Task<Message> {
        if self.active_panel() == ActivePanel::ConnectionMonitor {
            return Task::none();
        }

        self.set_active_panel(ActivePanel::ConnectionMonitor);

        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        // Reset state and set loading
        conn.connection_monitor.reset();
        conn.connection_monitor.loading = true;

        // Request connection list from server
        if let Err(e) = conn.send(ClientMessage::ConnectionMonitor) {
            conn.connection_monitor.loading = false;
            conn.connection_monitor.connections =
                Some(Err(format!("{}: {}", t("err-send-failed"), e)));
        }

        Task::none()
    }

    /// Close the Connection Monitor panel
    pub fn handle_close_connection_monitor(&mut self) -> Task<Message> {
        self.handle_show_chat_view()
    }

    /// Refresh the Connection Monitor data
    pub fn handle_refresh_connection_monitor(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        // Set loading state (keep existing data visible during refresh)
        conn.connection_monitor.loading = true;

        // Request connection list from server
        if let Err(e) = conn.send(ClientMessage::ConnectionMonitor) {
            conn.connection_monitor.loading = false;
            conn.connection_monitor.connections =
                Some(Err(format!("{}: {}", t("err-send-failed"), e)));
        }

        Task::none()
    }

    /// Handle Connection Monitor response from server
    pub fn handle_connection_monitor_response(
        &mut self,
        connection_id: usize,
        success: bool,
        error: Option<String>,
        connections: Option<Vec<nexus_common::protocol::ConnectionInfo>>,
        transfers: Option<Vec<nexus_common::protocol::TransferInfo>>,
    ) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        conn.connection_monitor.loading = false;

        if success {
            conn.connection_monitor.connections = Some(Ok(connections.unwrap_or_default()));
            conn.connection_monitor.transfers = Some(Ok(transfers.unwrap_or_default()));
        } else {
            let err = error.unwrap_or_else(|| t("err-unknown").to_string());
            conn.connection_monitor.connections = Some(Err(err.clone()));
            conn.connection_monitor.transfers = Some(Err(err));
        }

        Task::none()
    }

    /// Copy a value to the clipboard
    pub fn handle_connection_monitor_copy(&mut self, value: String) -> Task<Message> {
        let toast_text = t("toast-copied");
        iced::clipboard::write(value).chain(Task::done(Message::ShowToast(toast_text)))
    }

    /// Open User Info panel for the selected user
    pub fn handle_connection_monitor_info(&mut self, nickname: String) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            // Check permission (admins always have access)
            if !conn.has_permission(PERMISSION_USER_INFO) {
                return Task::none();
            }

            // Clear previous data and password change state, set return panel, then open
            conn.user_info_data = None;
            conn.password_change_state = None;
            conn.user_info_return_panel = Some(ActivePanel::ConnectionMonitor);
            conn.active_panel = ActivePanel::UserInfo;

            // Send UserInfo request to server and track it
            match conn.send(ClientMessage::UserInfo {
                nickname: nickname.clone(),
            }) {
                Ok(message_id) => {
                    conn.pending_requests
                        .track(message_id, ResponseRouting::PopulateUserInfoPanel(nickname));
                }
                Err(e) => {
                    let error_msg = format!("{}: {}", t("err-send-failed"), e);
                    conn.user_info_data = Some(Err(error_msg));
                }
            }
        }
        Task::none()
    }

    /// Open Disconnect Dialog with Kick pre-selected
    pub fn handle_connection_monitor_kick(&mut self, nickname: String) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            // Check permission
            if !conn.has_permission(PERMISSION_USER_KICK) {
                return Task::none();
            }

            conn.disconnect_dialog = Some(DisconnectDialogState::with_action(
                nickname,
                DisconnectAction::Kick,
            ));
        }
        Task::none()
    }

    /// Open Disconnect Dialog with Ban pre-selected
    pub fn handle_connection_monitor_ban(&mut self, nickname: String) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
        {
            // Check permission
            if !conn.has_permission(PERMISSION_BAN_CREATE) {
                return Task::none();
            }

            conn.disconnect_dialog = Some(DisconnectDialogState::with_action(
                nickname,
                DisconnectAction::Ban,
            ));
        }
        Task::none()
    }

    /// Handle sort column change for connections
    pub fn handle_connection_monitor_sort_by(
        &mut self,
        column: ConnectionMonitorSortColumn,
    ) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        // If clicking same column, toggle direction; otherwise set new column ascending
        if conn.connection_monitor.sort_column == column {
            conn.connection_monitor.sort_ascending = !conn.connection_monitor.sort_ascending;
        } else {
            conn.connection_monitor.sort_column = column;
            conn.connection_monitor.sort_ascending = true;
        }

        Task::none()
    }

    /// Handle tab selection
    pub fn handle_connection_monitor_tab_selected(
        &mut self,
        tab: ConnectionMonitorTab,
    ) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        conn.connection_monitor.active_tab = tab;

        Task::none()
    }

    /// Handle sort column change for transfers
    pub fn handle_connection_monitor_transfer_sort_by(
        &mut self,
        column: TransferSortColumn,
    ) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        // If clicking same column, toggle direction; otherwise set new column ascending
        if conn.connection_monitor.transfer_sort_column == column {
            conn.connection_monitor.transfer_sort_ascending =
                !conn.connection_monitor.transfer_sort_ascending;
        } else {
            conn.connection_monitor.transfer_sort_column = column;
            conn.connection_monitor.transfer_sort_ascending = true;
        }

        Task::none()
    }
}
