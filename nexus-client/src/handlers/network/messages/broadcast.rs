//! Broadcast message handlers

use crate::types::{ActivePanel, ChatMessage, InputId, Message, ScrollableId};
use crate::NexusApp;
use iced::widget::{scrollable, text_input};
use iced::Task;

impl NexusApp {
    /// Handle incoming server broadcast message
    pub fn handle_server_broadcast(
        &mut self,
        connection_id: usize,
        username: String,
        message: String,
    ) -> Task<Message> {
        self.add_chat_message(connection_id, ChatMessage::broadcast(username, message))
    }

    /// Handle user broadcast response (success/failure of sending a broadcast)
    pub fn handle_user_broadcast_response(
        &mut self,
        connection_id: usize,
        success: bool,
        error: Option<String>,
    ) -> Task<Message> {
        if let Some(conn) = self.connections.get_mut(&connection_id) {
            if success {
                // Close broadcast panel and clear any error
                self.ui_state.active_panel = ActivePanel::None;
                conn.broadcast_error = None;

                // Scroll to bottom if auto-scroll is enabled to show the broadcast message
                if self.active_connection == Some(connection_id) && conn.chat_auto_scroll {
                    return Task::batch([
                        scrollable::snap_to(
                            ScrollableId::ChatMessages.into(),
                            scrollable::RelativeOffset::END,
                        ),
                        text_input::focus(text_input::Id::from(InputId::ChatInput)),
                    ]);
                }

                // Focus chat input even if not scrolling
                return text_input::focus(text_input::Id::from(InputId::ChatInput));
            } else {
                conn.broadcast_error = Some(error.unwrap_or_default());
            }
        }
        Task::none()
    }
}