//! Bookmark add/edit form

use super::style::{
    BUTTON_PADDING, ELEMENT_SPACING, ERROR_COLOR, FORM_MAX_WIDTH, FORM_PADDING, INPUT_PADDING,
    SPACER_SIZE_LARGE, SPACER_SIZE_MEDIUM, SPACER_SIZE_SMALL, TEXT_SIZE, TITLE_SIZE,
};
use crate::types::{BookmarkEditMode, InputId, Message};
use iced::widget::{button, checkbox, column, container, row, text, text_input};
use iced::{Center, Element, Fill};

/// Displays form for adding or editing a server bookmark
///
/// Shows validated input fields for server connection details with optional
/// username/password fields and auto-connect checkbox. Validates that required
/// fields (name, address, port) are non-empty before enabling save button.
pub fn bookmark_edit_view<'a>(
    bookmark_edit_mode: &'a BookmarkEditMode,
    bookmark_name: &'a str,
    bookmark_address: &'a str,
    bookmark_port: &'a str,
    bookmark_username: &'a str,
    bookmark_password: &'a str,
    bookmark_auto_connect: bool,
    bookmark_error: &'a Option<String>,
) -> Element<'a, Message> {
    let dialog_title = match bookmark_edit_mode {
        BookmarkEditMode::Add => "Add Server",
        BookmarkEditMode::Edit(_) => "Edit Server",
        BookmarkEditMode::None => "",
    };

    // Validate required fields (username/password are optional)
    let can_save = !bookmark_name.trim().is_empty()
        && !bookmark_address.trim().is_empty()
        && !bookmark_port.trim().is_empty();

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
        text("").size(SPACER_SIZE_LARGE).into(),
    ];

    // Show error if present
    if let Some(error) = bookmark_error {
        column_items.push(text(error).size(TEXT_SIZE).color(ERROR_COLOR).into());
        column_items.push(text("").size(SPACER_SIZE_MEDIUM).into());
    }

    column_items.extend(vec![
        text_input("Server Name", bookmark_name)
            .on_input(Message::BookmarkNameChanged)
            .on_submit(submit_action.clone())
            .id(text_input::Id::from(InputId::BookmarkName))
            .padding(INPUT_PADDING)
            .size(TEXT_SIZE)
            .into(),
        text_input("IPv6 Address", bookmark_address)
            .on_input(Message::BookmarkAddressChanged)
            .on_submit(submit_action.clone())
            .id(text_input::Id::from(InputId::BookmarkAddress))
            .padding(INPUT_PADDING)
            .size(TEXT_SIZE)
            .into(),
        text_input("Port", bookmark_port)
            .on_input(Message::BookmarkPortChanged)
            .on_submit(submit_action.clone())
            .id(text_input::Id::from(InputId::BookmarkPort))
            .padding(INPUT_PADDING)
            .size(TEXT_SIZE)
            .into(),
        text_input("Username (optional)", bookmark_username)
            .on_input(Message::BookmarkUsernameChanged)
            .on_submit(submit_action.clone())
            .id(text_input::Id::from(InputId::BookmarkUsername))
            .padding(INPUT_PADDING)
            .size(TEXT_SIZE)
            .into(),
        text_input("Password (optional)", bookmark_password)
            .on_input(Message::BookmarkPasswordChanged)
            .on_submit(submit_action)
            .id(text_input::Id::from(InputId::BookmarkPassword))
            .secure(true)
            .padding(INPUT_PADDING)
            .size(TEXT_SIZE)
            .into(),
        text("").size(SPACER_SIZE_SMALL).into(),
        checkbox("Auto-connect at startup", bookmark_auto_connect)
            .on_toggle(Message::BookmarkAutoConnectToggled)
            .size(TEXT_SIZE)
            .into(),
        text("").size(SPACER_SIZE_MEDIUM).into(),
        {
            let mut buttons: Vec<Element<'a, Message>> = vec![
                if can_save {
                    button(text("Save").size(TEXT_SIZE))
                        .on_press(Message::SaveBookmark)
                        .padding(BUTTON_PADDING)
                        .into()
                } else {
                    button(text("Save").size(TEXT_SIZE))
                        .padding(BUTTON_PADDING)
                        .into()
                },
                button(text("Cancel").size(TEXT_SIZE))
                    .on_press(Message::CancelBookmarkEdit)
                    .padding(BUTTON_PADDING)
                    .into(),
            ];

            // Add Delete button only when editing (not adding)
            if let BookmarkEditMode::Edit(index) = bookmark_edit_mode {
                buttons.push(
                    button(text("Delete").size(TEXT_SIZE))
                        .on_press(Message::DeleteBookmark(*index))
                        .padding(BUTTON_PADDING)
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
