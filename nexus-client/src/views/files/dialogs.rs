//! File operation dialogs (delete, overwrite, info, new directory, rename)

use iced::widget::button as btn;
use iced::widget::text::Wrapping;
use iced::widget::{Space, button, column, row, text_input};
use iced::{Center, Element, Fill};
use nexus_common::protocol::FileInfoDetails;

use super::super::helpers::{format_bytes as format_size, format_local_timestamp};
use super::super::layout::scrollable_panel;
use super::helpers::file_icon_for_extension;
use crate::i18n::t;
use crate::icon;
use crate::style::{
    BUTTON_PADDING, CONTENT_MAX_WIDTH, CONTENT_PADDING, ELEMENT_SPACING, FILE_INFO_ICON_SIZE,
    FILE_INFO_ICON_SPACING, INPUT_PADDING, SPACER_SIZE_MEDIUM, SPACER_SIZE_SMALL, TEXT_SIZE,
    TITLE_SIZE, error_text_style, panel_title, shaped_text, shaped_text_wrapped,
};
use crate::types::{FilesManagementState, InputId, Message};

pub(super) fn delete_confirm_dialog<'a>(
    path: &str,
    error: Option<&'a String>,
    is_submitting: bool,
) -> Element<'a, Message> {
    let title = panel_title(t("files-delete-confirm-title"));

    // Extract just the filename from the path for display
    let name = path.rsplit('/').next().unwrap_or(path);
    let display_name = FilesManagementState::display_name(name);

    let message = crate::i18n::t_args("files-delete-confirm-message", &[("name", &display_name)]);

    let buttons = row![
        Space::new().width(Fill),
        button(shaped_text(t("button-cancel")).size(TEXT_SIZE))
            .on_press(Message::FileCancelDelete)
            .padding(BUTTON_PADDING)
            .style(btn::secondary),
        if is_submitting {
            button(shaped_text(t("button-delete")).size(TEXT_SIZE))
                .padding(BUTTON_PADDING)
                .style(btn::danger)
        } else {
            button(shaped_text(t("button-delete")).size(TEXT_SIZE))
                .on_press(Message::FileConfirmDelete)
                .padding(BUTTON_PADDING)
                .style(btn::danger)
        },
    ]
    .spacing(ELEMENT_SPACING);

    let mut form_items: Vec<Element<'_, Message>> = vec![title.into()];

    // Show error if present
    if let Some(err) = error {
        form_items.push(
            shaped_text_wrapped(err)
                .size(TEXT_SIZE)
                .width(Fill)
                .align_x(Center)
                .style(error_text_style)
                .into(),
        );
        form_items.push(Space::new().height(SPACER_SIZE_SMALL).into());
    } else {
        form_items.push(Space::new().height(SPACER_SIZE_MEDIUM).into());
    }

    form_items.extend([
        shaped_text_wrapped(&message)
            .size(TEXT_SIZE)
            .width(Fill)
            .align_x(Center)
            .into(),
        Space::new().height(SPACER_SIZE_MEDIUM).into(),
        buttons.into(),
    ]);

    let form = iced::widget::Column::with_children(form_items)
        .spacing(ELEMENT_SPACING)
        .padding(CONTENT_PADDING)
        .max_width(CONTENT_MAX_WIDTH);

    scrollable_panel(form)
}

/// Build the overwrite confirmation dialog
pub(super) fn overwrite_confirm_dialog<'a>(
    name: &str,
    has_file_delete: bool,
    is_submitting: bool,
    error: Option<&'a String>,
) -> Element<'a, Message> {
    let title = panel_title(t("files-overwrite-title"));

    let display_name = FilesManagementState::display_name(name);
    let message = crate::i18n::t_args("files-overwrite-message", &[("name", &display_name)]);

    // Build buttons based on permissions
    let mut buttons_row = row![Space::new().width(Fill),].spacing(ELEMENT_SPACING);

    buttons_row = buttons_row.push(
        button(shaped_text(t("button-cancel")).size(TEXT_SIZE))
            .on_press(Message::FileOverwriteCancel)
            .padding(BUTTON_PADDING)
            .style(btn::secondary),
    );

    // Only show overwrite button if user has file_delete permission
    if has_file_delete {
        if is_submitting {
            buttons_row = buttons_row.push(
                button(shaped_text(t("button-overwrite")).size(TEXT_SIZE))
                    .padding(BUTTON_PADDING)
                    .style(btn::danger),
            );
        } else {
            buttons_row = buttons_row.push(
                button(shaped_text(t("button-overwrite")).size(TEXT_SIZE))
                    .on_press(Message::FileOverwriteConfirm)
                    .padding(BUTTON_PADDING)
                    .style(btn::danger),
            );
        }
    }

    let mut form_items: Vec<Element<'_, Message>> = vec![title.into()];

    if let Some(err) = error {
        form_items.push(
            shaped_text_wrapped(err)
                .size(TEXT_SIZE)
                .width(Fill)
                .align_x(Center)
                .style(error_text_style)
                .into(),
        );
        form_items.push(Space::new().height(SPACER_SIZE_SMALL).into());
    } else {
        form_items.push(Space::new().height(SPACER_SIZE_MEDIUM).into());
    }

    form_items.extend([
        shaped_text_wrapped(&message)
            .size(TEXT_SIZE)
            .width(Fill)
            .align_x(Center)
            .into(),
        Space::new().height(SPACER_SIZE_MEDIUM).into(),
        buttons_row.into(),
    ]);

    let form = iced::widget::Column::with_children(form_items)
        .spacing(ELEMENT_SPACING)
        .padding(CONTENT_PADDING)
        .max_width(CONTENT_MAX_WIDTH);

    scrollable_panel(form)
}

/// Build the file info dialog
pub(super) fn file_info_dialog(info: &FileInfoDetails) -> Element<'_, Message> {
    let mut content = column![].spacing(ELEMENT_SPACING);

    // Header: Icon + Name side by side (like user info)
    let icon_element: Element<'_, Message> = if info.is_directory {
        icon::folder().size(FILE_INFO_ICON_SIZE).into()
    } else {
        file_icon_for_extension(&info.name)
            .size(FILE_INFO_ICON_SIZE)
            .into()
    };

    // Name (use display_name to strip folder type suffixes)
    let display_name = FilesManagementState::display_name(&info.name);
    let name_text = shaped_text(display_name)
        .size(TITLE_SIZE)
        .wrapping(Wrapping::WordOrGlyph);

    let header_row = row![icon_element, name_text]
        .spacing(FILE_INFO_ICON_SPACING)
        .align_y(Center);

    content = content.push(header_row);
    content = content.push(Space::new().height(SPACER_SIZE_MEDIUM));

    // Type (File/Directory)
    let type_value = if info.is_directory {
        t("files-info-directory")
    } else {
        t("files-info-file")
    };
    content = content.push(info_row(t("files-info-type"), type_value));

    // Symlink (only shown if true)
    if info.is_symlink {
        content = content.push(info_row(t("files-info-symlink"), t("files-info-yes")));
    }

    // Size
    content = content.push(info_row(t("files-info-size"), format_size(info.size)));

    // Item count (directories only)
    if info.is_directory {
        let items_value = match info.item_count {
            Some(count) => count.to_string(),
            None => t("files-info-na"),
        };
        content = content.push(info_row(t("files-info-items"), items_value));
    }

    // MIME type (show N/A for directories)
    let mime_value = match &info.mime_type {
        Some(mime) => mime.clone(),
        None => t("files-info-na"),
    };
    content = content.push(info_row(t("files-info-mime-type"), mime_value));

    // Created (show N/A if not available)
    let created_value = match info.created {
        Some(ts) => {
            let s = format_local_timestamp(ts);
            if s.is_empty() { t("files-info-na") } else { s }
        }
        None => t("files-info-na"),
    };
    content = content.push(info_row(t("files-info-created"), created_value));

    // Modified (always available)
    let modified_value = format_local_timestamp(info.modified);
    let modified_value = if modified_value.is_empty() {
        t("files-info-na")
    } else {
        modified_value
    };
    content = content.push(info_row(t("files-info-modified"), modified_value));

    // BLAKE3 hash (files only) - use WordOrGlyph wrapping for long hash without spaces
    if let Some(hash) = &info.blake3 {
        let sha_row = row![
            shaped_text(t("files-info-blake3")).size(TEXT_SIZE),
            Space::new().width(ELEMENT_SPACING),
            shaped_text(hash.clone())
                .size(TEXT_SIZE)
                .wrapping(Wrapping::WordOrGlyph),
        ]
        .align_y(Center);
        content = content.push(sha_row);
    }

    content = content.push(Space::new().height(SPACER_SIZE_MEDIUM));

    // Close button
    let buttons = row![
        Space::new().width(Fill),
        button(shaped_text(t("button-close")).size(TEXT_SIZE))
            .on_press(Message::CloseFileInfo)
            .padding(BUTTON_PADDING),
    ]
    .spacing(ELEMENT_SPACING);

    content = content.push(buttons);

    let form = content
        .padding(CONTENT_PADDING)
        .max_width(CONTENT_MAX_WIDTH);

    scrollable_panel(form)
}

/// Build a single info row with label and value (matches user info style)
fn info_row<'a>(label: String, value: String) -> iced::widget::Row<'a, Message> {
    row![
        shaped_text(label).size(TEXT_SIZE),
        Space::new().width(ELEMENT_SPACING),
        shaped_text_wrapped(value).size(TEXT_SIZE),
    ]
    .align_y(Center)
}

/// Build the new directory dialog (matches broadcast view layout)
pub(super) fn new_directory_dialog<'a>(
    name: &str,
    error: Option<&String>,
    is_submitting: bool,
) -> Element<'a, Message> {
    let title = panel_title(t("files-create-directory-title"));

    let name_valid = !name.is_empty() && error.is_none();

    let name_input = text_input(&t("files-directory-name-placeholder"), name)
        .id(InputId::NewDirectoryName)
        .on_input(Message::FileNewDirectoryNameChanged)
        .on_submit(Message::FileNewDirectorySubmit)
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE);

    let buttons = row![
        Space::new().width(Fill),
        button(shaped_text(t("button-cancel")).size(TEXT_SIZE))
            .on_press(Message::FileNewDirectoryCancel)
            .padding(BUTTON_PADDING)
            .style(btn::secondary),
        if name_valid && !is_submitting {
            button(shaped_text(t("button-create")).size(TEXT_SIZE))
                .on_press(Message::FileNewDirectorySubmit)
                .padding(BUTTON_PADDING)
        } else {
            button(shaped_text(t("button-create")).size(TEXT_SIZE)).padding(BUTTON_PADDING)
        },
    ]
    .spacing(ELEMENT_SPACING);

    let mut form_items: Vec<Element<'_, Message>> = vec![title.into()];

    // Show error if present
    if let Some(err) = error {
        form_items.push(
            shaped_text_wrapped(err)
                .size(TEXT_SIZE)
                .width(Fill)
                .align_x(Center)
                .style(error_text_style)
                .into(),
        );
        form_items.push(Space::new().height(SPACER_SIZE_SMALL).into());
    } else {
        form_items.push(Space::new().height(SPACER_SIZE_MEDIUM).into());
    }

    form_items.extend([
        name_input.into(),
        Space::new().height(SPACER_SIZE_MEDIUM).into(),
        buttons.into(),
    ]);

    let form = iced::widget::Column::with_children(form_items)
        .spacing(ELEMENT_SPACING)
        .padding(CONTENT_PADDING)
        .max_width(CONTENT_MAX_WIDTH);

    scrollable_panel(form)
}

/// Rename dialog for files and directories
pub(super) fn rename_dialog<'a>(
    path: &str,
    name: &str,
    error: Option<&String>,
    is_submitting: bool,
) -> Element<'a, Message> {
    // Extract filename from path for display
    let filename = path.rsplit('/').next().unwrap_or(path);
    let display_filename = FilesManagementState::display_name(filename);
    let title_text = crate::i18n::t_args("files-rename-title", &[("name", &display_filename)]);
    let title = panel_title(title_text);

    let name_valid = !name.is_empty() && error.is_none();

    let name_input = text_input(&t("files-rename-placeholder"), name)
        .id(InputId::RenameName)
        .on_input(Message::FileRenameNameChanged)
        .on_submit(Message::FileRenameSubmit)
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE);

    let buttons = row![
        Space::new().width(Fill),
        button(shaped_text(t("button-cancel")).size(TEXT_SIZE))
            .on_press(Message::FileRenameCancel)
            .padding(BUTTON_PADDING)
            .style(btn::secondary),
        if name_valid && !is_submitting {
            button(shaped_text(t("files-rename")).size(TEXT_SIZE))
                .on_press(Message::FileRenameSubmit)
                .padding(BUTTON_PADDING)
        } else {
            button(shaped_text(t("files-rename")).size(TEXT_SIZE)).padding(BUTTON_PADDING)
        },
    ]
    .spacing(ELEMENT_SPACING);

    let mut form_items: Vec<Element<'_, Message>> = vec![title.into()];

    // Show error if present
    if let Some(err) = error {
        form_items.push(
            shaped_text_wrapped(err)
                .size(TEXT_SIZE)
                .width(Fill)
                .align_x(Center)
                .style(error_text_style)
                .into(),
        );
        form_items.push(Space::new().height(SPACER_SIZE_SMALL).into());
    } else {
        form_items.push(Space::new().height(SPACER_SIZE_MEDIUM).into());
    }

    form_items.extend([
        name_input.into(),
        Space::new().height(SPACER_SIZE_MEDIUM).into(),
        buttons.into(),
    ]);

    let form = iced::widget::Column::with_children(form_items)
        .spacing(ELEMENT_SPACING)
        .padding(CONTENT_PADDING)
        .max_width(CONTENT_MAX_WIDTH);

    scrollable_panel(form)
}
