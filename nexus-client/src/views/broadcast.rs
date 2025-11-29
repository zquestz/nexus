//! Broadcast message panel view

use super::style::{
    BUTTON_PADDING, ELEMENT_SPACING, FORM_MAX_WIDTH, FORM_PADDING, INPUT_PADDING, MONOSPACE_FONT,
    SPACER_SIZE_MEDIUM, TEXT_SIZE, TITLE_SIZE, primary_button_style, primary_text_input_style,
    shaped_text,
};
use crate::i18n::t;
use crate::types::{InputId, Message, ServerConnection};
use iced::widget::{button, column, container, row, text_input};
use iced::{Center, Element, Fill};

/// Render the broadcast panel
///
/// Shows a form for composing and sending broadcast messages to all connected users.
pub fn broadcast_view(conn: &ServerConnection) -> Element<'_, Message> {
    let title = shaped_text(t("title-broadcast-message"))
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

    let message_input = text_input(&t("placeholder-broadcast-message"), &conn.broadcast_message)
        .id(text_input::Id::from(InputId::BroadcastMessage))
        .on_input(Message::BroadcastMessageChanged)
        .on_submit(submit_action)
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE)
        .font(MONOSPACE_FONT)
        .style(primary_text_input_style());

    let button_row = row![
        if can_send {
            button(shaped_text(t("button-send")).size(TEXT_SIZE))
                .on_press(Message::SendBroadcastPressed)
                .padding(BUTTON_PADDING)
                .style(primary_button_style())
        } else {
            button(shaped_text(t("button-send")).size(TEXT_SIZE))
                .padding(BUTTON_PADDING)
                .style(primary_button_style())
        },
        button(shaped_text(t("button-cancel")).size(TEXT_SIZE))
            .on_press(Message::ToggleBroadcast)
            .padding(BUTTON_PADDING)
            .style(primary_button_style()),
    ]
    .spacing(ELEMENT_SPACING);

    let form = column![
        title,
        shaped_text("").size(SPACER_SIZE_MEDIUM),
        message_input,
        shaped_text("").size(SPACER_SIZE_MEDIUM),
        button_row
    ]
    .spacing(ELEMENT_SPACING)
    .padding(FORM_PADDING)
    .max_width(FORM_MAX_WIDTH);

    container(form).width(Fill).height(Fill).center(Fill).into()
}
