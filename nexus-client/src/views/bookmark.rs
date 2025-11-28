//! Bookmark add/edit form

use super::constants::{BUTTON_CANCEL, BUTTON_DELETE, PLACEHOLDER_PORT};
use super::style::{
    BUTTON_PADDING, ELEMENT_SPACING, FORM_MAX_WIDTH, FORM_PADDING, INPUT_PADDING,
    SPACER_SIZE_MEDIUM, SPACER_SIZE_SMALL, TEXT_SIZE, TITLE_SIZE, form_error_color,
    primary_button_style, primary_checkbox_style, primary_text_input_style,
};
use crate::types::{BookmarkEditMode, BookmarkFormData, InputId, Message};
use iced::widget::{button, checkbox, column, container, row, text, text_input};
use iced::{Center, Element, Fill};

// UI text constants
const TITLE_ADD_SERVER: &str = "Add Server";
const TITLE_EDIT_SERVER: &str = "Edit Server";
const PLACEHOLDER_SERVER_NAME: &str = "Server Name";
const PLACEHOLDER_IPV6_ADDRESS: &str = "IPv6 Address";
const PLACEHOLDER_USERNAME_OPTIONAL: &str = "Username (optional)";
const PLACEHOLDER_PASSWORD_OPTIONAL: &str = "Password (optional)";
const LABEL_AUTO_CONNECT: &str = "Auto-connect at startup";
const BUTTON_SAVE: &str = "Save";

/// Displays form for adding or editing a server bookmark
///
/// Shows validated input fields for server connection details with optional
/// username/password/locale fields and auto-connect checkbox. Validates that required
/// fields (name, address, port) are non-empty before enabling save button.
pub fn bookmark_edit_view<'a>(form: BookmarkFormData<'a>) -> Element<'a, Message> {
    let dialog_title = match form.mode {
        BookmarkEditMode::Add => TITLE_ADD_SERVER,
        BookmarkEditMode::Edit(_) => TITLE_EDIT_SERVER,
        BookmarkEditMode::None => "",
    };

    // Validate required fields (username/password are optional)
    let can_save = !form.name.trim().is_empty()
        && !form.address.trim().is_empty()
        && !form.port.trim().is_empty();

    // Helper for on_submit - avoid action when form is invalid
    // Note: We send a no-op message to prevent submit when invalid
    let submit_action = if can_save {
        Message::SaveBookmark
    } else {
        Message::BookmarkNameChanged(String::new())
    };

    let mut column_items = vec![
        text(dialog_title)
            .size(TITLE_SIZE)
            .width(Fill)
            .align_x(Center)
            .into(),
    ];

    // Show error if present
    if let Some(error) = form.error {
        column_items.push(text(error).size(TEXT_SIZE).color(form_error_color()).into());
        column_items.push(text("").size(SPACER_SIZE_MEDIUM).into());
    }

    column_items.extend(vec![
        text_input(PLACEHOLDER_SERVER_NAME, form.name)
            .on_input(Message::BookmarkNameChanged)
            .on_submit(submit_action.clone())
            .id(text_input::Id::from(InputId::BookmarkName))
            .padding(INPUT_PADDING)
            .size(TEXT_SIZE)
            .style(primary_text_input_style())
            .into(),
        text_input(PLACEHOLDER_IPV6_ADDRESS, form.address)
            .on_input(Message::BookmarkAddressChanged)
            .on_submit(submit_action.clone())
            .id(text_input::Id::from(InputId::BookmarkAddress))
            .padding(INPUT_PADDING)
            .size(TEXT_SIZE)
            .style(primary_text_input_style())
            .into(),
        text_input(PLACEHOLDER_PORT, form.port)
            .on_input(Message::BookmarkPortChanged)
            .on_submit(submit_action.clone())
            .id(text_input::Id::from(InputId::BookmarkPort))
            .padding(INPUT_PADDING)
            .size(TEXT_SIZE)
            .style(primary_text_input_style())
            .into(),
        text_input(PLACEHOLDER_USERNAME_OPTIONAL, form.username)
            .on_input(Message::BookmarkUsernameChanged)
            .on_submit(submit_action.clone())
            .id(text_input::Id::from(InputId::BookmarkUsername))
            .padding(INPUT_PADDING)
            .size(TEXT_SIZE)
            .style(primary_text_input_style())
            .into(),
        text_input(PLACEHOLDER_PASSWORD_OPTIONAL, form.password)
            .on_input(Message::BookmarkPasswordChanged)
            .on_submit(submit_action)
            .id(text_input::Id::from(InputId::BookmarkPassword))
            .secure(true)
            .padding(INPUT_PADDING)
            .size(TEXT_SIZE)
            .style(primary_text_input_style())
            .into(),
        text("").size(SPACER_SIZE_SMALL).into(),
        checkbox(LABEL_AUTO_CONNECT, form.auto_connect)
            .on_toggle(Message::BookmarkAutoConnectToggled)
            .size(TEXT_SIZE)
            .style(primary_checkbox_style())
            .into(),
        text("").size(SPACER_SIZE_MEDIUM).into(),
        {
            let mut buttons: Vec<Element<'a, Message>> = vec![
                if can_save {
                    button(text(BUTTON_SAVE).size(TEXT_SIZE))
                        .on_press(Message::SaveBookmark)
                        .padding(BUTTON_PADDING)
                        .style(primary_button_style())
                        .into()
                } else {
                    button(text(BUTTON_SAVE).size(TEXT_SIZE))
                        .padding(BUTTON_PADDING)
                        .style(primary_button_style())
                        .into()
                },
                button(text(BUTTON_CANCEL).size(TEXT_SIZE))
                    .on_press(Message::CancelBookmarkEdit)
                    .padding(BUTTON_PADDING)
                    .style(primary_button_style())
                    .into(),
            ];

            // Add Delete button only when editing (not adding)
            if let BookmarkEditMode::Edit(index) = form.mode {
                buttons.push(
                    button(text(BUTTON_DELETE).size(TEXT_SIZE))
                        .on_press(Message::DeleteBookmark(*index))
                        .padding(BUTTON_PADDING)
                        .style(primary_button_style())
                        .into(),
                );
            }

            row(buttons).spacing(ELEMENT_SPACING).into()
        },
    ]);

    let content = column(column_items)
        .spacing(ELEMENT_SPACING)
        .padding(FORM_PADDING)
        .max_width(FORM_MAX_WIDTH);

    container(content)
        .width(Fill)
        .height(Fill)
        .center(Fill)
        .into()
}
