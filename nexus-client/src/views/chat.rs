//! Chat interface for active server connections

use crate::types::{InputId, Message, ScrollableId, ServerConnection};
use iced::widget::{column, container, row, scrollable, text, text_input, button, Column};
use iced::{Element, Fill};

/// Displays chat messages and input field
pub fn chat_view<'a>(
    conn: &'a ServerConnection,
    message_input: &'a str,
) -> Element<'a, Message> {
    // Check permissions
    let can_send = conn.is_admin || conn.permissions.contains(&"chat_send".to_string());
    let can_receive = conn.is_admin || conn.permissions.contains(&"chat_receive".to_string());
    
    // No tabs in chat view - just the content
    let top_bar = row![].spacing(0).padding(0);

    // Chat messages - only show if user has permission to receive
    let chat_scrollable = if can_receive {
        let mut chat_column = Column::new().spacing(3).padding(8);
        for msg in &conn.chat_messages {
            let time_str = msg.timestamp.format("%H:%M:%S").to_string();
            let display = if msg.username == "System" {
                text(format!("[{}] [SYS] {}", time_str, msg.message))
                    .size(12)
                    .color([0.7, 0.7, 0.7])
                    .font(iced::Font::MONOSPACE)
            } else if msg.username == "Error" {
                text(format!("[{}] [ERR] {}", time_str, msg.message))
                    .size(12)
                    .color([1.0, 0.0, 0.0])
                    .font(iced::Font::MONOSPACE)
            } else if msg.username == "Info" {
                text(format!("[{}] [INFO] {}", time_str, msg.message))
                    .size(12)
                    .color([0.5, 0.8, 1.0])
                    .font(iced::Font::MONOSPACE)
            } else {
                text(format!("[{}] {}: {}", time_str, msg.username, msg.message))
                    .size(12)
                    .font(iced::Font::MONOSPACE)
            };
            chat_column = chat_column.push(display);
        }

        scrollable(chat_column)
            .id(ScrollableId::ChatMessages.into())
            .height(Fill)
    } else {
        // No permission to receive chat
        let no_permission_column = Column::new()
            .spacing(3)
            .padding(8)
            .push(
                text("You do not have permission to view chat messages")
                    .size(12)
                    .color([0.7, 0.7, 0.7])
                    .font(iced::Font::MONOSPACE)
            );
        
        scrollable(no_permission_column)
            .id(ScrollableId::ChatMessages.into())
            .height(Fill)
    };

    // Message input
    let input_row = row![
        if can_send {
            text_input("Type a message...", message_input)
                .on_input(Message::MessageInputChanged)
                .on_submit(Message::SendMessagePressed)
                .id(text_input::Id::from(InputId::ChatInput))
                .padding(8)
                .size(13)
        } else {
            text_input("No permission to send messages", message_input)
                .id(text_input::Id::from(InputId::ChatInput))
                .padding(8)
                .size(13)
        },
        if can_send {
            button(text("Send").size(12))
                .on_press(Message::SendMessagePressed)
                .padding(8)
        } else {
            button(text("Send").size(12))
                .padding(8)
        },
    ]
    .spacing(5);

    container(
        column![top_bar, chat_scrollable, input_row,]
            .spacing(5)
            .padding(5),
    )
    .width(Fill)
    .height(Fill)
    .into()
}