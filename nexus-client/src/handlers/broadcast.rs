//! Broadcast message handlers

use crate::types::{InputId, Message};
use crate::NexusApp;
use iced::Task;
use iced::widget::text_input;
use nexus_common::protocol::ClientMessage;

impl NexusApp {
    /// Handle broadcast message input change
    pub fn handle_broadcast_message_changed(&mut self, input: String) -> Task<Message> {
        if let Some(conn_id) = self.active_connection {
            if let Some(conn) = self.connections.get_mut(&conn_id) {
                conn.broadcast_message = input;
            }
        }
        Task::none()
    }

    /// Handle send broadcast button press
    pub fn handle_send_broadcast_pressed(&mut self) -> Task<Message> {
        if let Some(conn_id) = self.active_connection {
            if let Some(conn) = self.connections.get_mut(&conn_id) {
                if !conn.broadcast_message.trim().is_empty() {
                    let msg = ClientMessage::UserBroadcast {
                        message: conn.broadcast_message.clone(),
                    };
                    let _ = conn.tx.send(msg);
                    conn.broadcast_message.clear();
                    
                    // Close broadcast panel and return focus to chat
                    self.ui_state.show_broadcast = false;
                    return text_input::focus(text_input::Id::from(InputId::ChatInput));
                }
            }
        }
        Task::none()
    }

    /// Handle toggle broadcast panel
    pub fn handle_toggle_broadcast(&mut self) -> Task<Message> {
        self.ui_state.show_broadcast = !self.ui_state.show_broadcast;
        
        // Close other admin panels when opening broadcast
        if self.ui_state.show_broadcast {
            self.ui_state.show_add_user = false;
            self.ui_state.show_delete_user = false;
            
            // Focus broadcast input when opening
            if self.active_connection.is_some() {
                return text_input::focus(text_input::Id::from(InputId::BroadcastMessage));
            }
        } else {
            // Return focus to chat when closing
            if self.active_connection.is_some() {
                return text_input::focus(text_input::Id::from(InputId::ChatInput));
            }
        }
        
        Task::none()
    }
}