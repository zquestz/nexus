//! Broadcast message panel view

use crate::types::{InputId, Message, ServerConnection};
use iced::widget::{button, column, container, row, text, text_input};
use iced::{Center, Element, Fill};

/// Render the broadcast panel
///
/// Shows a form for composing and sending broadcast messages to all connected users.
pub fn broadcast_view<'a>(conn: &'a ServerConnection) -> Element<'a, Message> {
    let title = text("Broadcast Message")
        .size(20)
        .width(Fill)
        .align_x(Center);

    let can_send = !conn.broadcast_message.trim().is_empty();

    let message_input = text_input("Enter broadcast message...", &conn.broadcast_message)
        .id(text_input::Id::from(InputId::BroadcastMessage))
        .on_input(Message::BroadcastMessageChanged)
        .on_submit(if can_send {
            Message::SendBroadcastPressed
        } else {
            Message::BroadcastMessageChanged(String::new())
        })
        .padding(8)
        .size(14);

    let button_row = row![
        if can_send {
            button(text("Send").size(14))
                .on_press(Message::SendBroadcastPressed)
                .padding(10)
        } else {
            button(text("Send").size(14)).padding(10)
        },
        button(text("Cancel").size(14))
            .on_press(Message::ToggleBroadcast)
            .padding(10),
    ]
    .spacing(10);

    let form = column![
        title,
        text("").size(15),
        message_input,
        text("").size(10),
        button_row
    ]
    .spacing(10)
    .padding(20)
    .max_width(400);

    container(form)
        .width(Fill)
        .height(Fill)
        .center(Fill)
        .into()
}
