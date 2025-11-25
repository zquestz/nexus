//! Chat interface for active server connections

use super::style::{
    CHAT_INPUT_SIZE, CHAT_MESSAGE_SIZE, CHAT_SPACING,
    INPUT_PADDING, SMALL_PADDING, SMALL_SPACING, primary_button_style,
    primary_text_input_style, system_text_color, info_text_color, error_message_color,
    chat_text_color,
};
use crate::types::{InputId, Message, ScrollableId, ServerConnection};
use iced::widget::{Column, button, column, container, row, scrollable, text, text_input};
use iced::{Element, Fill};

// Permission constants
const PERMISSION_CHAT_SEND: &str = "chat_send";
const PERMISSION_CHAT_RECEIVE: &str = "chat_receive";

/// Displays chat messages and input field
///
/// Shows chat history with timestamps and different colors for system/error/info
/// messages. Checks permissions before allowing send/receive operations.
pub fn chat_view<'a>(conn: &'a ServerConnection, message_input: &'a str) -> Element<'a, Message> {
    // Check permissions
    let can_send = conn.is_admin || conn.permissions.iter().any(|p| p == PERMISSION_CHAT_SEND);
    let can_receive = conn.is_admin
        || conn
            .permissions
            .iter()
            .any(|p| p == PERMISSION_CHAT_RECEIVE);

    // Chat messages - only show if user has permission to receive
    let chat_scrollable = if can_receive {
        let mut chat_column = Column::new().spacing(CHAT_SPACING).padding(INPUT_PADDING);
        for msg in &conn.chat_messages {
            let time_str = msg.timestamp.format("%H:%M:%S").to_string();
            let display = if msg.username == "System" {
                text(format!("[{}] [SYS] {}", time_str, msg.message))
                    .size(CHAT_MESSAGE_SIZE)
                    .style(|theme| iced::widget::text::Style {
                        color: Some(system_text_color(theme)),
                    })
                    .font(iced::Font::MONOSPACE)
            } else if msg.username == "Error" {
                text(format!("[{}] [ERR] {}", time_str, msg.message))
                    .size(CHAT_MESSAGE_SIZE)
                    .color(error_message_color())
                    .font(iced::Font::MONOSPACE)
            } else if msg.username == "Info" {
                text(format!("[{}] [INFO] {}", time_str, msg.message))
                    .size(CHAT_MESSAGE_SIZE)
                    .style(|theme| iced::widget::text::Style {
                        color: Some(info_text_color(theme)),
                    })
                    .font(iced::Font::MONOSPACE)
            } else {
                text(format!("[{}] {}: {}", time_str, msg.username, msg.message))
                    .size(CHAT_MESSAGE_SIZE)
                    .style(|theme| iced::widget::text::Style {
                        color: Some(chat_text_color(theme)),
                    })
                    .font(iced::Font::MONOSPACE)
            };
            chat_column = chat_column.push(display);
        }

        scrollable(chat_column)
            .id(ScrollableId::ChatMessages.into())
            .width(Fill)
            .height(Fill)
    } else {
        // No permission to receive chat
        let no_permission_column = Column::new()
            .spacing(CHAT_SPACING)
            .padding(INPUT_PADDING)
            .push(
                text("You do not have permission to view chat messages")
                    .size(CHAT_MESSAGE_SIZE)
                    .style(|theme| iced::widget::text::Style {
                        color: Some(system_text_color(theme)),
                    })
                    .font(iced::Font::MONOSPACE),
            );

        scrollable(no_permission_column)
            .id(ScrollableId::ChatMessages.into())
            .width(Fill)
            .height(Fill)
    };

    // Message input
    let input_row = row![
        if can_send {
            text_input("Type a message...", message_input)
                .on_input(Message::MessageInputChanged)
                .on_submit(Message::SendMessagePressed)
                .id(text_input::Id::from(InputId::ChatInput))
                .padding(INPUT_PADDING)
                .size(CHAT_INPUT_SIZE)
                .style(primary_text_input_style())
                .width(Fill)
        } else {
            text_input("No permission to send messages", message_input)
                .id(text_input::Id::from(InputId::ChatInput))
                .padding(INPUT_PADDING)
                .size(CHAT_INPUT_SIZE)
                .style(primary_text_input_style())
                .width(Fill)
        },
        if can_send {
            button(text("Send").size(CHAT_MESSAGE_SIZE))
                .on_press(Message::SendMessagePressed)
                .padding(INPUT_PADDING)
                .style(primary_button_style())
        } else {
            button(text("Send").size(CHAT_MESSAGE_SIZE))
                .padding(INPUT_PADDING)
                .style(primary_button_style())
        },
    ]
    .spacing(SMALL_SPACING)
    .width(Fill);

    container(
        column![chat_scrollable, input_row,]
            .spacing(SMALL_SPACING)
            .padding(SMALL_PADDING),
    )
    .width(Fill)
    .height(Fill)
    .into()
}
