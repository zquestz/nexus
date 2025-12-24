//! Bookmark add/edit form

use super::layout::scrollable_panel;
use crate::i18n::t;
use crate::style::{
    BUTTON_PADDING, ELEMENT_SPACING, FORM_MAX_WIDTH, FORM_PADDING, INPUT_PADDING,
    SPACER_SIZE_MEDIUM, SPACER_SIZE_SMALL, TEXT_SIZE, error_text_style, panel_title, shaped_text,
    shaped_text_wrapped,
};
use crate::types::{BookmarkEditMode, BookmarkEditState, InputId, Message};
use iced::widget::button as btn;
use iced::widget::{Id, Row, Space, button, checkbox, column, row, text, text_input};
use iced::{Center, Element, Fill};
use iced_aw::NumberInput;

// ============================================================================
// Bookmark Edit View
// ============================================================================

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
    // Port is always valid since it's a u16
    let can_save =
        !state.bookmark.name.trim().is_empty() && !state.bookmark.address.trim().is_empty();

    // Helper for on_submit - avoid action when form is invalid
    // Note: We send a no-op message to prevent submit when invalid
    let submit_action = if can_save {
        Message::SaveBookmark
    } else {
        Message::BookmarkNameChanged(String::new())
    };

    let mut column_items: Vec<Element<'_, Message>> = vec![panel_title(dialog_title).into()];

    // Show error if present
    if let Some(error) = &state.error {
        column_items.push(
            shaped_text_wrapped(error)
                .size(TEXT_SIZE)
                .width(Fill)
                .align_x(Center)
                .style(error_text_style)
                .into(),
        );
        column_items.push(Space::new().height(SPACER_SIZE_SMALL).into());
    } else {
        column_items.push(Space::new().height(SPACER_SIZE_MEDIUM).into());
    }

    column_items.extend([
        text_input(&t("placeholder-server-name"), &state.bookmark.name)
            .on_input(Message::BookmarkNameChanged)
            .on_submit(submit_action.clone())
            .id(Id::from(InputId::BookmarkName))
            .padding(INPUT_PADDING)
            .size(TEXT_SIZE)
            .into(),
        text_input(&t("placeholder-server-address"), &state.bookmark.address)
            .on_input(Message::BookmarkAddressChanged)
            .on_submit(submit_action.clone())
            .id(Id::from(InputId::BookmarkAddress))
            .padding(INPUT_PADDING)
            .size(TEXT_SIZE)
            .into(),
        {
            let port_label = shaped_text(t("label-port")).size(TEXT_SIZE);
            let port_input: Element<'_, Message> = NumberInput::new(
                &state.bookmark.port,
                1..=65535,
                Message::BookmarkPortChanged,
            )
            .id(Id::from(InputId::BookmarkPort))
            .padding(INPUT_PADDING)
            .into();
            Row::new()
                .push(port_label)
                .push(port_input)
                .spacing(ELEMENT_SPACING)
                .align_y(Center)
                .into()
        },
        text_input(
            &t("placeholder-username-optional"),
            &state.bookmark.username,
        )
        .on_input(Message::BookmarkUsernameChanged)
        .on_submit(submit_action.clone())
        .id(Id::from(InputId::BookmarkUsername))
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE)
        .into(),
        text_input(
            &t("placeholder-password-optional"),
            &state.bookmark.password,
        )
        .on_input(Message::BookmarkPasswordChanged)
        .on_submit(submit_action.clone())
        .id(Id::from(InputId::BookmarkPassword))
        .secure(true)
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE)
        .into(),
        text_input(
            &t("placeholder-nickname-optional"),
            &state.bookmark.nickname,
        )
        .on_input(Message::BookmarkNicknameChanged)
        .on_submit(submit_action)
        .id(Id::from(InputId::BookmarkNickname))
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE)
        .into(),
        Space::new().height(SPACER_SIZE_SMALL).into(),
        checkbox(state.bookmark.auto_connect)
            .label(t("label-auto-connect"))
            .on_toggle(Message::BookmarkAutoConnectToggled)
            .size(TEXT_SIZE)
            .text_shaping(text::Shaping::Advanced)
            .into(),
        Space::new().height(SPACER_SIZE_MEDIUM).into(),
        {
            let mut buttons: Vec<Element<'_, Message>> = vec![
                button(shaped_text(t("button-cancel")).size(TEXT_SIZE))
                    .on_press(Message::CancelBookmarkEdit)
                    .padding(BUTTON_PADDING)
                    .style(btn::secondary)
                    .into(),
            ];

            // Add Delete button in middle when editing (not adding)
            if let BookmarkEditMode::Edit(index) = state.mode {
                buttons.push(
                    button(shaped_text(t("button-delete")).size(TEXT_SIZE))
                        .on_press(Message::DeleteBookmark(index))
                        .padding(BUTTON_PADDING)
                        .style(btn::danger)
                        .into(),
                );
            }

            // Save is the primary action, goes on the right
            buttons.push(if can_save {
                button(shaped_text(t("button-save")).size(TEXT_SIZE))
                    .on_press(Message::SaveBookmark)
                    .padding(BUTTON_PADDING)
                    .into()
            } else {
                button(shaped_text(t("button-save")).size(TEXT_SIZE))
                    .padding(BUTTON_PADDING)
                    .into()
            });

            {
                let mut row_items: Vec<Element<'_, Message>> =
                    vec![Space::new().width(Fill).into()];
                row_items.extend(buttons);
                row(row_items).spacing(ELEMENT_SPACING).into()
            }
        },
    ]);

    let content = column(column_items)
        .spacing(ELEMENT_SPACING)
        .padding(FORM_PADDING)
        .max_width(FORM_MAX_WIDTH);

    scrollable_panel(content)
}
