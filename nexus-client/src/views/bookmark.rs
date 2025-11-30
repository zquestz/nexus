//! Bookmark add/edit form

use crate::style::{
    BUTTON_PADDING, ELEMENT_SPACING, FORM_MAX_WIDTH, FORM_PADDING, INPUT_PADDING,
    SPACER_SIZE_MEDIUM, SPACER_SIZE_SMALL, TEXT_SIZE, TITLE_SIZE, form_error_color,
    primary_button_style, primary_checkbox_style, primary_text_input_style, shaped_text,
};
use crate::i18n::t;
use crate::types::{BookmarkEditMode, BookmarkEditState, InputId, Message};
use iced::widget::{button, checkbox, column, container, row, text, text_input};
use iced::{Center, Element, Fill};

/// Displays form for adding or editing a server bookmark
///
/// Shows validated input fields for server connection details with optional
/// username/password/locale fields and auto-connect checkbox. Validates that required
/// fields (name, address, port) are non-empty before enabling save button.
pub fn bookmark_edit_view(state: &BookmarkEditState) -> Element<'_, Message> {
    let dialog_title = match state.mode {
        BookmarkEditMode::Add => t("title-add-bookmark"),
        BookmarkEditMode::Edit(_) => t("title-edit-server"),
        BookmarkEditMode::None => String::new(),
    };

    // Validate required fields (username/password are optional)
    let can_save = !state.bookmark.name.trim().is_empty()
        && !state.bookmark.address.trim().is_empty()
        && !state.bookmark.port.trim().is_empty();

    // Helper for on_submit - avoid action when form is invalid
    // Note: We send a no-op message to prevent submit when invalid
    let submit_action = if can_save {
        Message::SaveBookmark
    } else {
        Message::BookmarkNameChanged(String::new())
    };

    let mut column_items = vec![
        shaped_text(&dialog_title)
            .size(TITLE_SIZE)
            .width(Fill)
            .align_x(Center)
            .into(),
    ];

    // Show error if present
    if let Some(error) = &state.error {
        column_items.push(
            shaped_text(error)
                .size(TEXT_SIZE)
                .width(Fill)
                .align_x(Center)
                .color(form_error_color())
                .into(),
        );
        column_items.push(shaped_text("").size(SPACER_SIZE_SMALL).into());
    } else {
        column_items.push(shaped_text("").size(SPACER_SIZE_MEDIUM).into());
    }

    column_items.extend(vec![
        text_input(&t("placeholder-server-name"), &state.bookmark.name)
            .on_input(Message::BookmarkNameChanged)
            .on_submit(submit_action.clone())
            .id(text_input::Id::from(InputId::BookmarkName))
            .padding(INPUT_PADDING)
            .size(TEXT_SIZE)
            .style(primary_text_input_style())
            .into(),
        text_input(&t("placeholder-server-address"), &state.bookmark.address)
            .on_input(Message::BookmarkAddressChanged)
            .on_submit(submit_action.clone())
            .id(text_input::Id::from(InputId::BookmarkAddress))
            .padding(INPUT_PADDING)
            .size(TEXT_SIZE)
            .style(primary_text_input_style())
            .into(),
        text_input(&t("placeholder-port"), &state.bookmark.port)
            .on_input(Message::BookmarkPortChanged)
            .on_submit(submit_action.clone())
            .id(text_input::Id::from(InputId::BookmarkPort))
            .padding(INPUT_PADDING)
            .size(TEXT_SIZE)
            .style(primary_text_input_style())
            .into(),
        text_input(&t("placeholder-username-optional"), &state.bookmark.username)
            .on_input(Message::BookmarkUsernameChanged)
            .on_submit(submit_action.clone())
            .id(text_input::Id::from(InputId::BookmarkUsername))
            .padding(INPUT_PADDING)
            .size(TEXT_SIZE)
            .style(primary_text_input_style())
            .into(),
        text_input(&t("placeholder-password-optional"), &state.bookmark.password)
            .on_input(Message::BookmarkPasswordChanged)
            .on_submit(submit_action)
            .id(text_input::Id::from(InputId::BookmarkPassword))
            .secure(true)
            .padding(INPUT_PADDING)
            .size(TEXT_SIZE)
            .style(primary_text_input_style())
            .into(),
        shaped_text("").size(SPACER_SIZE_SMALL).into(),
        checkbox(t("label-auto-connect"), state.bookmark.auto_connect)
            .on_toggle(Message::BookmarkAutoConnectToggled)
            .size(TEXT_SIZE)
            .text_shaping(text::Shaping::Advanced)
            .style(primary_checkbox_style())
            .into(),
        shaped_text("").size(SPACER_SIZE_MEDIUM).into(),
        {
            let mut buttons: Vec<Element<'_, Message>> = vec![
                if can_save {
                    button(shaped_text(t("button-save")).size(TEXT_SIZE))
                        .on_press(Message::SaveBookmark)
                        .padding(BUTTON_PADDING)
                        .style(primary_button_style())
                        .into()
                } else {
                    button(shaped_text(t("button-save")).size(TEXT_SIZE))
                        .padding(BUTTON_PADDING)
                        .style(primary_button_style())
                        .into()
                },
                button(shaped_text(t("button-cancel")).size(TEXT_SIZE))
                    .on_press(Message::CancelBookmarkEdit)
                    .padding(BUTTON_PADDING)
                    .style(primary_button_style())
                    .into(),
            ];

            // Add Delete button only when editing (not adding)
            if let BookmarkEditMode::Edit(index) = state.mode {
                buttons.push(
                    button(shaped_text(t("button-delete")).size(TEXT_SIZE))
                        .on_press(Message::DeleteBookmark(index))
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
