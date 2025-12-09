//! Server info edit handlers

use iced::Task;
use iced::widget::{Id, operation};
use nexus_common::protocol::ClientMessage;
use nexus_common::validators::{
    self, MAX_SERVER_DESCRIPTION_LENGTH, MAX_SERVER_NAME_LENGTH, ServerDescriptionError,
    ServerNameError,
};

use crate::NexusApp;
use crate::i18n::{t, t_args};
use crate::types::{InputId, Message, ServerInfoEditState};

impl NexusApp {
    // ==================== Panel Actions ====================

    /// Enter server info edit mode
    ///
    /// Creates a new edit state with the current server info values.
    /// Only admins can access this feature.
    pub fn handle_edit_server_info_pressed(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        // Only admins can edit server info
        if !conn.is_admin {
            return Task::none();
        }

        // Create edit state with current values
        conn.server_info_edit = Some(ServerInfoEditState::new(
            conn.server_name.as_deref(),
            conn.server_description.as_deref(),
            conn.max_connections_per_ip,
        ));

        // Focus the name input
        operation::focus(Id::from(InputId::EditServerInfoName))
    }

    /// Cancel server info edit mode
    ///
    /// Clears the edit state and returns to the display view.
    pub fn handle_cancel_edit_server_info(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        conn.server_info_edit = None;
        Task::none()
    }

    /// Save server info changes
    ///
    /// Validates the form and sends `ServerInfoUpdate` to the server.
    pub fn handle_update_server_info_pressed(&mut self) -> Task<Message> {
        let Some(conn_id) = self.active_connection else {
            return Task::none();
        };
        let Some(conn) = self.connections.get_mut(&conn_id) else {
            return Task::none();
        };

        // Get edit state
        let Some(edit_state) = &conn.server_info_edit else {
            return Task::none();
        };

        // Validate server name
        if let Err(e) = validators::validate_server_name(&edit_state.name) {
            let error_msg = match e {
                ServerNameError::Empty => t("err-server-name-empty"),
                ServerNameError::TooLong => t_args(
                    "err-server-name-too-long",
                    &[("max", &MAX_SERVER_NAME_LENGTH.to_string())],
                ),
                ServerNameError::ContainsNewlines => t("err-server-name-contains-newlines"),
                ServerNameError::InvalidCharacters => t("err-server-name-invalid-characters"),
            };
            if let Some(edit) = &mut conn.server_info_edit {
                edit.error = Some(error_msg);
            }
            return Task::none();
        }

        // Validate server description
        if let Err(e) = validators::validate_server_description(&edit_state.description) {
            let error_msg = match e {
                ServerDescriptionError::TooLong => t_args(
                    "err-server-description-too-long",
                    &[("max", &MAX_SERVER_DESCRIPTION_LENGTH.to_string())],
                ),
                ServerDescriptionError::ContainsNewlines => {
                    t("err-server-description-contains-newlines")
                }
                ServerDescriptionError::InvalidCharacters => {
                    t("err-server-description-invalid-characters")
                }
            };
            if let Some(edit) = &mut conn.server_info_edit {
                edit.error = Some(error_msg);
            }
            return Task::none();
        }

        // Check if there are any changes
        if !edit_state.has_changes(
            conn.server_name.as_deref(),
            conn.server_description.as_deref(),
            conn.max_connections_per_ip,
        ) {
            // No changes, just close the edit view
            conn.server_info_edit = None;
            return Task::none();
        }

        // Build the update message with only changed fields
        let name = if edit_state.name != conn.server_name.as_deref().unwrap_or("") {
            Some(edit_state.name.clone())
        } else {
            None
        };

        let description =
            if edit_state.description != conn.server_description.as_deref().unwrap_or("") {
                Some(edit_state.description.clone())
            } else {
                None
            };

        let max_connections_per_ip =
            if edit_state.max_connections_per_ip != conn.max_connections_per_ip {
                edit_state.max_connections_per_ip
            } else {
                None
            };

        let msg = ClientMessage::ServerInfoUpdate {
            name,
            description,
            max_connections_per_ip,
        };

        if let Err(e) = conn.tx.send(msg) {
            if let Some(edit) = &mut conn.server_info_edit {
                edit.error = Some(t_args(
                    "err-failed-send-update",
                    &[("error", &e.to_string())],
                ));
            }
            return Task::none();
        }

        // Keep edit mode open until we get a response
        // The response handler will close it on success
        Task::none()
    }

    // ==================== Form Field Handlers ====================

    /// Handle server info name field change
    pub fn handle_edit_server_info_name_changed(&mut self, name: String) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
            && let Some(edit_state) = &mut conn.server_info_edit
        {
            edit_state.name = name;
        }
        Task::none()
    }

    /// Handle server info description field change
    pub fn handle_edit_server_info_description_changed(
        &mut self,
        description: String,
    ) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
            && let Some(edit_state) = &mut conn.server_info_edit
        {
            edit_state.description = description;
        }
        Task::none()
    }

    /// Handle server info max connections field change
    pub fn handle_edit_server_info_max_connections_changed(
        &mut self,
        max_connections: u32,
    ) -> Task<Message> {
        if let Some(conn_id) = self.active_connection
            && let Some(conn) = self.connections.get_mut(&conn_id)
            && let Some(edit_state) = &mut conn.server_info_edit
        {
            edit_state.max_connections_per_ip = Some(max_connections);
        }
        Task::none()
    }
}
