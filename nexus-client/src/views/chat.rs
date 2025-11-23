//! Chat interface for active server connections

use crate::types::{InputId, Message, ScrollableId, ServerConnection};
use iced::widget::{column, container, row, scrollable, text, text_input, button, Column};
use iced::{Element, Fill};

/// Displays chat messages and input field
pub fn chat_view<'a>(
    conn: &'a ServerConnection,
    message_input: &'a str,
    _show_add_user: bool,
    _show_delete_user: bool,
) -> Element<'a, Message> {
    // No tabs in chat view - just the content
    let top_bar = row![].spacing(0).padding(0);

    // Chat messages
    let mut chat_column = Column::new().spacing(3).padding(8);
    for msg in &conn.chat_messages {
        let time_str = msg.timestamp.format("%H:%M:%S").to_string();
        let display = if msg.username == "System" {
            text(format!("[{}] [SYS] {}", time_str, msg.message))
                .size(12)
                .color([0.7, 0.7, 0.7])
        } else if msg.username == "Error" {
            text(format!("[{}] [ERR] {}", time_str, msg.message))
                .size(12)
                .color([1.0, 0.0, 0.0])
        } else if msg.username == "Info" {
            text(format!("[{}] [INFO] {}", time_str, msg.message))
                .size(12)
                .color([0.5, 0.8, 1.0])
        } else {
            text(format!("[{}] {}: {}", time_str, msg.username, msg.message)).size(12)
        };
        chat_column = chat_column.push(display);
    }

    let chat_scrollable = scrollable(chat_column)
        .id(ScrollableId::ChatMessages.into())
        .height(Fill);

    // Message input
    let input_row = row![
        text_input("Type a message...", message_input)
            .on_input(Message::MessageInputChanged)
            .on_submit(Message::SendMessagePressed)
            .id(text_input::Id::from(InputId::ChatInput))
            .padding(8)
            .size(13),
        button(text("Send").size(12))
            .on_press(Message::SendMessagePressed)
            .padding(8),
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