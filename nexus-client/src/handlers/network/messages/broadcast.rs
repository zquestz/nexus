//! Broadcast message handlers

use iced::Task;
use nexus_common::names::fold_name;

use crate::NexusApp;
use crate::config::events::EventType;
use crate::events::{EventContext, emit_event};
use crate::types::{ChatMessage, Message};

impl NexusApp {
    /// Handle incoming server broadcast message
    ///
    /// The protocol sends `username` for broadcasts. Since shared accounts cannot
    /// broadcast, the sender's username always equals their nickname, so we can
    /// store it directly in the ChatMessage.nickname field for display.
    pub fn handle_server_broadcast(
        &mut self,
        connection_id: usize,
        username: String,
        message: String,
    ) -> Task<Message> {
        // Check if we sent this broadcast (suppress notification but allow sound)
        let is_from_self = self
            .connections
            .get(&connection_id)
            .map(|conn| fold_name(&conn.connection_info.username) == fold_name(&username))
            .unwrap_or(false);

        emit_event(
            self,
            EventType::Broadcast,
            EventContext::new()
                .with_connection_id(connection_id)
                .with_username(&username)
                .with_message(&message)
                .with_is_from_self(is_from_self),
        );

        // username == nickname for broadcasters (shared accounts can't broadcast)
        // Show in active tab (channel/PM) with console fallback
        self.add_active_tab_message(connection_id, ChatMessage::broadcast(username, message))
    }

    /// Handle user broadcast response (success/failure of sending a broadcast)
    pub fn handle_user_broadcast_response(
        &mut self,
        connection_id: usize,
        success: bool,
        error: Option<String>,
    ) -> Task<Message> {
        let Some(conn) = self.connections.get_mut(&connection_id) else {
            return Task::none();
        };

        if success {
            conn.broadcast_error = None;
            return self.handle_show_chat_view();
        }

        conn.broadcast_error = Some(error.unwrap_or_default());
        Task::none()
    }
}
