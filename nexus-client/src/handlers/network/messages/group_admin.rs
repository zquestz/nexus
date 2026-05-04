//! Group administration response handlers

use iced::Task;
use iced::widget::{Id, operation};
use nexus_common::framing::MessageId;
use nexus_common::protocol::{ClientMessage, GroupInfo};

use crate::NexusApp;
use crate::i18n::t_args;
use crate::types::{
    ActivePanel, ChatMessage, GroupManagementMode, InputId, Message, PendingRequests,
    ResponseRouting,
};

/// Data from a GroupEditResponse message
pub struct GroupEditResponseData {
    pub success: bool,
    pub error: Option<String>,
    pub id: Option<i64>,
    pub name: Option<String>,
    pub is_shared: Option<bool>,
    pub permissions: Option<Vec<String>>,
    pub member_count: Option<u32>,
}

impl NexusApp {
    /// Handle group list response
    ///
    /// Populates the available_groups cache on the user management state.
    /// If tracked via ResponseRouting::PopulateGroupManagementList, the groups are now cached
    /// for display in the Groups tab.
    pub fn handle_group_list_response(
        &mut self,
        connection_id: usize,
        message_id: MessageId,
        success: bool,
        error: Option<String>,
        groups: Option<Vec<GroupInfo>>,
    ) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        // Check if this was a tracked request from group management panel
        let routing = conn.pending_requests.remove(&message_id);

        if success {
            // Always update the available_groups cache
            conn.user_management.available_groups = Some(Ok(groups.unwrap_or_default()));
            return Task::none();
        }

        // On error, show in the appropriate place
        if matches!(routing, Some(ResponseRouting::PopulateGroupManagementList)) {
            // List-fetch failure — store inline so the Groups tab content
            // shows it in place of the table (mirrors the users-list shape).
            conn.user_management.available_groups = Some(Err(error.unwrap_or_default()));
        } else {
            return self.add_active_tab_message(
                connection_id,
                ChatMessage::error(error.unwrap_or_default()),
            );
        }

        Task::none()
    }

    /// Handle group create response
    ///
    /// If tracked via ResponseRouting::GroupManagementCreateResult, returns to list view
    /// and refreshes the group list on success.
    pub fn handle_group_create_response(
        &mut self,
        connection_id: usize,
        message_id: MessageId,
        success: bool,
        error: Option<String>,
        _id: Option<i64>,
        name: Option<String>,
    ) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        // Check if this was a tracked request from group management panel
        let routing = conn.pending_requests.remove(&message_id);

        if success {
            // Show success message in chat with group name
            let message = t_args("msg-group-created", &[("name", &name.unwrap_or_default())]);
            let task = self.add_active_tab_message(connection_id, ChatMessage::system(message));

            // If from group management panel, return to list and refresh
            if matches!(routing, Some(ResponseRouting::GroupManagementCreateResult)) {
                if let Some(conn) = self.connections.get_mut(&connection_id) {
                    conn.user_management.group_management.reset_to_list();
                }
                return Task::batch([task, self.refresh_group_management_list_for(connection_id)]);
            }

            return task;
        }

        // On error, show in the appropriate place
        if matches!(routing, Some(ResponseRouting::GroupManagementCreateResult)) {
            if let Some(conn) = self.connections.get_mut(&connection_id) {
                conn.user_management.group_management.create_error =
                    Some(error.unwrap_or_default());
                conn.user_management.group_management.is_submitting = false;
            }
        } else {
            return self.add_active_tab_message(
                connection_id,
                ChatMessage::error(error.unwrap_or_default()),
            );
        }

        Task::none()
    }

    /// Handle group edit response (loading group details for editing)
    ///
    /// If tracked via ResponseRouting::PopulateGroupManagementEdit, populates the edit form.
    pub fn handle_group_edit_response(
        &mut self,
        connection_id: usize,
        message_id: MessageId,
        data: GroupEditResponseData,
    ) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        // Check if this was a tracked request from group management panel
        let routing = conn.pending_requests.remove(&message_id);

        if data.success {
            // If from group management panel, load the group details into edit form
            if matches!(routing, Some(ResponseRouting::PopulateGroupManagementEdit)) {
                conn.user_management.group_management.enter_edit_mode(
                    data.id.unwrap_or(0),
                    data.name.unwrap_or_default(),
                    data.is_shared.unwrap_or(false),
                    data.member_count.unwrap_or(0),
                    data.permissions.unwrap_or_default(),
                );
                self.focused_field = InputId::EditGroupName;
                return operation::focus(Id::from(InputId::EditGroupName));
            }
        } else {
            // On error, show in the appropriate place
            if matches!(routing, Some(ResponseRouting::PopulateGroupManagementEdit)) {
                conn.user_management
                    .set_group_list_error(data.error.unwrap_or_default());
            } else {
                return self.add_active_tab_message(
                    connection_id,
                    ChatMessage::error(data.error.unwrap_or_default()),
                );
            }
        }

        Task::none()
    }

    /// Handle group update response
    ///
    /// If tracked via ResponseRouting::GroupManagementUpdateResult, returns to list view
    /// and refreshes the group list on success.
    pub fn handle_group_update_response(
        &mut self,
        connection_id: usize,
        message_id: MessageId,
        success: bool,
        error: Option<String>,
        _id: Option<i64>,
        name: Option<String>,
    ) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        // Check if this was a tracked request from group management panel
        let routing = conn.pending_requests.remove(&message_id);

        if success {
            // Show success message in chat with group name
            let message = t_args("msg-group-updated", &[("name", &name.unwrap_or_default())]);
            let task = self.add_active_tab_message(connection_id, ChatMessage::system(message));

            // If from group management panel, return to list and refresh
            if matches!(routing, Some(ResponseRouting::GroupManagementUpdateResult)) {
                if let Some(conn) = self.connections.get_mut(&connection_id) {
                    conn.user_management.group_management.reset_to_list();
                }
                return Task::batch([task, self.refresh_group_management_list_for(connection_id)]);
            }

            return task;
        }

        // On error, show in the appropriate place
        if matches!(routing, Some(ResponseRouting::GroupManagementUpdateResult)) {
            if let Some(conn) = self.connections.get_mut(&connection_id) {
                conn.user_management.group_management.edit_error = Some(error.unwrap_or_default());
                conn.user_management.group_management.is_submitting = false;
            }
        } else {
            return self.add_active_tab_message(
                connection_id,
                ChatMessage::error(error.unwrap_or_default()),
            );
        }

        Task::none()
    }

    /// Handle group delete response
    ///
    /// If tracked via ResponseRouting::GroupManagementDeleteResult, closes the delete
    /// dialog on success and refreshes the group list, or shows error in dialog on failure.
    pub fn handle_group_delete_response(
        &mut self,
        connection_id: usize,
        message_id: MessageId,
        success: bool,
        error: Option<String>,
        _id: Option<i64>,
        name: Option<String>,
    ) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        // Check if this was a tracked request from group management panel
        let routing = conn.pending_requests.remove(&message_id);

        if success {
            // Show success message in chat with group name
            let message = t_args("msg-group-deleted", &[("name", &name.unwrap_or_default())]);
            let task = self.add_active_tab_message(connection_id, ChatMessage::system(message));

            // If from group management panel, close dialog and refresh the list
            if matches!(routing, Some(ResponseRouting::GroupManagementDeleteResult)) {
                if let Some(conn) = self.connections.get_mut(&connection_id) {
                    conn.user_management.group_management.mode = GroupManagementMode::List;
                    conn.user_management.group_management.delete_error = None;
                    conn.user_management.group_management.is_delete_submitting = false;
                }
                return Task::batch([task, self.refresh_group_management_list_for(connection_id)]);
            }

            return task;
        }

        // On error, show in the delete dialog (keep it open for retry)
        if matches!(routing, Some(ResponseRouting::GroupManagementDeleteResult)) {
            if let Some(conn) = self.connections.get_mut(&connection_id) {
                conn.user_management.group_management.delete_error =
                    Some(error.unwrap_or_default());
                conn.user_management.group_management.is_delete_submitting = false;
            }
        } else {
            return self.add_active_tab_message(
                connection_id,
                ChatMessage::error(error.unwrap_or_default()),
            );
        }

        Task::none()
    }

    // ==================== Helper Functions ====================

    /// Refresh group management list for a specific connection
    ///
    /// Sends a GroupList request tracked as PopulateGroupManagementList to update
    /// the available_groups cache displayed in the Groups tab.
    fn refresh_group_management_list_for(&mut self, connection_id: usize) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        // Only refresh if we're in the user management panel
        if conn.active_panel != ActivePanel::UserManagement {
            return Task::none();
        }

        // Request group list from server
        match conn.send(ClientMessage::GroupList) {
            Ok(message_id) => {
                conn.pending_requests
                    .track(message_id, ResponseRouting::PopulateGroupManagementList);
            }
            Err(e) => {
                conn.user_management.available_groups = Some(Err(e.to_string()));
            }
        }

        Task::none()
    }
}
