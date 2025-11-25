//! Broadcast message panel view

use super::style::{
    BUTTON_PADDING, ELEMENT_SPACING, FORM_MAX_WIDTH, FORM_PADDING, INPUT_PADDING,
    SPACER_SIZE_LARGE, SPACER_SIZE_MEDIUM, TEXT_SIZE, TITLE_SIZE, primary_button_style,
    primary_text_input_style,
};
use crate::types::{InputId, Message, ServerConnection};
use iced::widget::{button, column, container, row, text, text_input};
use iced::{Center, Element, Fill};

/// Render the broadcast panel
///
/// Shows a form for composing and sending broadcast messages to all connected users.
pub fn broadcast_view(conn: &ServerConnection) -> Element<'_, Message> {
    let title = text("Broadcast Message")
        .size(TITLE_SIZE)
        .width(Fill)
        .align_x(Center);

    let can_send = !conn.broadcast_message.trim().is_empty();

    // No-op message when form is invalid (Iced requires a Message for on_submit)
    let submit_action = if can_send {
        Message::SendBroadcastPressed
    } else {
        Message::BroadcastMessageChanged(String::new())
    };

    let message_input = text_input("Enter broadcast message...", &conn.broadcast_message)
        .id(text_input::Id::from(InputId::BroadcastMessage))
        .on_input(Message::BroadcastMessageChanged)
        .on_submit(submit_action)
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE)
        .style(primary_text_input_style());

    let button_row = row![
        if can_send {
            button(text("Send").size(TEXT_SIZE))
                .on_press(Message::SendBroadcastPressed)
                .padding(BUTTON_PADDING)
                .style(primary_button_style())
        } else {
            button(text("Send").size(TEXT_SIZE))
                .padding(BUTTON_PADDING)
                .style(primary_button_style())
        },
        button(text("Cancel").size(TEXT_SIZE))
            .on_press(Message::ToggleBroadcast)
            .padding(BUTTON_PADDING)
            .style(primary_button_style()),
    ]
    .spacing(ELEMENT_SPACING);

    let form = column![
        title,
        text("").size(SPACER_SIZE_LARGE),
        message_input,
        text("").size(SPACER_SIZE_MEDIUM),
        button_row
    ]
    .spacing(ELEMENT_SPACING)
    .padding(FORM_PADDING)
    .max_width(FORM_MAX_WIDTH);

    container(form).width(Fill).height(Fill).center(Fill).into()
}
