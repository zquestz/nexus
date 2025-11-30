//! Broadcast message panel view

use crate::style::{
    BORDER_WIDTH, BUTTON_PADDING, ELEMENT_SPACING, FORM_MAX_WIDTH, FORM_PADDING, INPUT_PADDING,
    MONOSPACE_FONT, SPACER_SIZE_MEDIUM, SPACER_SIZE_SMALL, TEXT_SIZE, TITLE_SIZE,
    content_background, form_error_color, primary_button_style, primary_text_input_style,
    shaped_text, sidebar_border,
};
use crate::i18n::t;
use crate::types::{InputId, Message, ServerConnection};
use iced::widget::{button, column, container, row, text_input};
use iced::{Background, Center, Element, Fill};

/// Render the broadcast panel
///
/// Shows a form for composing and sending broadcast messages to all connected users.
pub fn broadcast_view(conn: &ServerConnection) -> Element<'_, Message> {
    let title = shaped_text(t("title-broadcast-message"))
        .size(TITLE_SIZE)
        .width(Fill)
        .align_x(Center);

    let can_send = !conn.broadcast_message.trim().is_empty();

    // Validate form on Enter when invalid, submit when valid
    let submit_action = if can_send {
        Message::SendBroadcastPressed
    } else {
        Message::ValidateBroadcast
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
            .on_press(Message::CancelBroadcast)
            .padding(BUTTON_PADDING)
            .style(primary_button_style()),
    ]
    .spacing(ELEMENT_SPACING);

    let mut form_items: Vec<Element<'_, Message>> = vec![title.into()];

    // Show error if present
    if let Some(error) = &conn.broadcast_error {
        form_items.push(
            shaped_text(error)
                .size(TEXT_SIZE)
                .width(Fill)
                .align_x(Center)
                .color(form_error_color())
                .into(),
        );
        form_items.push(shaped_text("").size(SPACER_SIZE_SMALL).into());
    } else {
        form_items.push(shaped_text("").size(SPACER_SIZE_MEDIUM).into());
    }

    form_items.extend([
        message_input.into(),
        shaped_text("").size(SPACER_SIZE_MEDIUM).into(),
        button_row.into(),
    ]);

    let form = iced::widget::Column::with_children(form_items)
        .spacing(ELEMENT_SPACING)
        .padding(FORM_PADDING)
        .max_width(FORM_MAX_WIDTH);

    // Top border separator to match chat view
    let top_separator = container(shaped_text(""))
        .width(Fill)
        .height(BORDER_WIDTH)
        .style(|theme| container::Style {
            background: Some(Background::Color(sidebar_border(theme))),
            ..Default::default()
        });

    // Bottom border separator to match chat view
    let bottom_separator = container(shaped_text(""))
        .width(Fill)
        .height(BORDER_WIDTH)
        .style(|theme| container::Style {
            background: Some(Background::Color(sidebar_border(theme))),
            ..Default::default()
        });

    column![
        top_separator,
        container(form)
            .width(Fill)
            .height(Fill)
            .center(Fill)
            .style(|theme| container::Style {
                background: Some(Background::Color(content_background(theme))),
                ..Default::default()
            }),
        bottom_separator,
    ]
    .width(Fill)
    .height(Fill)
    .into()
}
