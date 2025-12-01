//! Broadcast message panel view

use crate::i18n::t;
use crate::style::{
    BUTTON_PADDING, ELEMENT_SPACING, FORM_MAX_WIDTH, FORM_PADDING, INPUT_PADDING, MONOSPACE_FONT,
    SPACER_SIZE_MEDIUM, SPACER_SIZE_SMALL, TEXT_SIZE, TITLE_SIZE, content_background_style,
    error_text_style, separator_style, shaped_text, shaped_text_wrapped,
};
use crate::types::{InputId, Message, ServerConnection};
use iced::widget::{Space, button, column, container, row, text_input};
use iced::{Center, Element, Fill};

// ============================================================================
// Helper Functions
// ============================================================================

/// Create a horizontal separator line
fn separator<'a>() -> Element<'a, Message> {
    container(Space::new(Fill, 1.0))
        .width(Fill)
        .height(1.0)
        .style(separator_style)
        .into()
}

// ============================================================================
// Broadcast View
// ============================================================================

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
        .font(MONOSPACE_FONT);

    let buttons = row![
        if can_send {
            button(shaped_text(t("button-send")).size(TEXT_SIZE))
                .on_press(Message::SendBroadcastPressed)
                .padding(BUTTON_PADDING)
        } else {
            button(shaped_text(t("button-send")).size(TEXT_SIZE)).padding(BUTTON_PADDING)
        },
        button(shaped_text(t("button-cancel")).size(TEXT_SIZE))
            .on_press(Message::CancelBroadcast)
            .padding(BUTTON_PADDING),
    ]
    .spacing(ELEMENT_SPACING);

    let mut form_items: Vec<Element<'_, Message>> = vec![title.into()];

    // Show error if present
    if let Some(error) = &conn.broadcast_error {
        form_items.push(
            shaped_text_wrapped(error)
                .size(TEXT_SIZE)
                .width(Fill)
                .align_x(Center)
                .style(error_text_style)
                .into(),
        );
        form_items.push(Space::with_height(SPACER_SIZE_SMALL).into());
    } else {
        form_items.push(Space::with_height(SPACER_SIZE_MEDIUM).into());
    }

    form_items.extend([
        message_input.into(),
        Space::with_height(SPACER_SIZE_MEDIUM).into(),
        buttons.into(),
    ]);

    let form = iced::widget::Column::with_children(form_items)
        .spacing(ELEMENT_SPACING)
        .padding(FORM_PADDING)
        .max_width(FORM_MAX_WIDTH);

    column![
        separator(),
        container(form)
            .width(Fill)
            .height(Fill)
            .center(Fill)
            .style(content_background_style),
        separator(),
    ]
    .width(Fill)
    .height(Fill)
    .into()
}
