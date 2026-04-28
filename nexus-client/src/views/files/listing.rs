//! File listing table and context menu

use iced::widget::text::Wrapping;
use iced::widget::{Space, button, container, lazy, row, table};
use iced::{Center, Element, Fill, Right};

use super::helpers::{file_icon_for_extension, format_size, format_timestamp};
use super::{FilePermissions, FileRowData, FileTableDeps};
use crate::i18n::t;
use crate::icon;
use crate::style::{
    CONTEXT_MENU_ITEM_PADDING, CONTEXT_MENU_MIN_WIDTH, CONTEXT_MENU_PADDING,
    CONTEXT_MENU_SEPARATOR_HEIGHT, CONTEXT_MENU_SEPARATOR_MARGIN, FILE_DATE_COLUMN_WIDTH,
    FILE_LIST_ICON_SIZE, FILE_LIST_ICON_SPACING, FILE_SIZE_COLUMN_WIDTH, NO_SPACING,
    SEPARATOR_HEIGHT, SORT_ICON_LEFT_MARGIN, SORT_ICON_RIGHT_MARGIN, SORT_ICON_SIZE,
    SPACER_SIZE_SMALL, TEXT_SIZE, context_menu_container_style, menu_button_danger_style,
    menu_button_style, muted_text_style, separator_style, shaped_text,
    transparent_icon_button_style, upload_folder_style,
};
use crate::types::{FileSortColumn, FilesManagementState, Message};
use crate::widgets::{LazyContextMenu, MenuButton};

pub(super) fn lazy_file_table(deps: FileTableDeps) -> Element<'static, Message> {
    lazy(deps, |deps| {
        // Name column header
        let name_sort_icon: Element<'static, Message> = if deps.sort_column == FileSortColumn::Name
        {
            let sort_icon = if deps.sort_ascending {
                icon::down_dir()
            } else {
                icon::up_dir()
            };
            sort_icon
                .size(SORT_ICON_SIZE)
                .style(muted_text_style)
                .into()
        } else {
            Space::new().width(SORT_ICON_SIZE).into()
        };
        let name_header_content: Element<'static, Message> = row![
            shaped_text(t("files-column-name"))
                .size(TEXT_SIZE)
                .wrapping(Wrapping::Word)
                .style(muted_text_style),
            Space::new().width(Fill),
            Space::new().width(SORT_ICON_LEFT_MARGIN),
            name_sort_icon,
            Space::new().width(SORT_ICON_RIGHT_MARGIN),
        ]
        .align_y(Center)
        .into();
        let name_header: Element<'static, Message> = button(name_header_content)
            .padding(NO_SPACING)
            .width(Fill)
            .style(transparent_icon_button_style)
            .on_press(Message::FileSortBy(FileSortColumn::Name))
            .into();

        // Name column - each row has all the data it needs
        let name_column = table::column(name_header, |row: FileRowData| {
            let is_directory = row.entry.dir_type.is_some();
            let display_name = FilesManagementState::display_name(&row.entry.name);

            // Icon based on type
            let icon_element: Element<'static, Message> = if is_directory {
                if row.entry.can_upload {
                    icon::folder()
                        .size(FILE_LIST_ICON_SIZE)
                        .style(upload_folder_style)
                        .into()
                } else {
                    icon::folder().size(FILE_LIST_ICON_SIZE).into()
                }
            } else {
                file_icon_for_extension(&row.entry.name)
                    .size(FILE_LIST_ICON_SIZE)
                    .into()
            };

            // Name text - muted if cut
            let name_text = if row.is_cut {
                shaped_text(display_name)
                    .size(TEXT_SIZE)
                    .wrapping(Wrapping::WordOrGlyph)
                    .style(muted_text_style)
            } else {
                shaped_text(display_name)
                    .size(TEXT_SIZE)
                    .wrapping(Wrapping::WordOrGlyph)
            };

            let name_content: Element<'static, Message> = row![
                icon_element,
                Space::new().width(FILE_LIST_ICON_SPACING),
                name_text,
            ]
            .align_y(iced::Alignment::Start)
            .into();

            // Make rows clickable
            let row_element: Element<'static, Message> = if is_directory {
                button(name_content)
                    .padding(NO_SPACING)
                    .style(transparent_icon_button_style)
                    .on_press(Message::FileNavigate(row.path.clone()))
                    .into()
            } else if row.perms.file_download {
                button(name_content)
                    .padding(NO_SPACING)
                    .style(transparent_icon_button_style)
                    .on_press(Message::FileDownload(row.path.clone()))
                    .into()
            } else {
                name_content
            };

            // Context menu
            let can_delete = row.perms.file_delete || row.bypass_via_ownership;
            let can_rename = row.perms.file_rename || row.bypass_via_ownership;
            let has_any_permission = row.perms.file_info
                || can_delete
                || can_rename
                || row.perms.file_move
                || row.perms.file_copy
                || row.perms.file_download
                || row.perms.file_upload;

            if has_any_permission {
                LazyContextMenu::new(row_element, move || {
                    build_lazy_context_menu(
                        &row.path,
                        &row.entry.name,
                        is_directory,
                        row.entry.can_upload,
                        row.perms,
                        row.has_clipboard,
                        row.bypass_via_ownership,
                    )
                })
                .into()
            } else {
                row_element
            }
        })
        .width(Fill);

        // Size column header
        let size_sort_icon: Element<'static, Message> = if deps.sort_column == FileSortColumn::Size
        {
            let sort_icon = if deps.sort_ascending {
                icon::down_dir()
            } else {
                icon::up_dir()
            };
            sort_icon
                .size(SORT_ICON_SIZE)
                .style(muted_text_style)
                .into()
        } else {
            Space::new().width(SORT_ICON_SIZE).into()
        };
        let size_header_content: Element<'static, Message> = row![
            shaped_text(t("files-column-size"))
                .size(TEXT_SIZE)
                .wrapping(Wrapping::Word)
                .style(muted_text_style),
            Space::new().width(Fill),
            Space::new().width(SORT_ICON_LEFT_MARGIN),
            size_sort_icon,
            Space::new().width(SORT_ICON_RIGHT_MARGIN),
        ]
        .align_y(Center)
        .into();
        let size_header: Element<'static, Message> = button(size_header_content)
            .padding(NO_SPACING)
            .width(Fill)
            .style(transparent_icon_button_style)
            .on_press(Message::FileSortBy(FileSortColumn::Size))
            .into();

        // Size column
        let size_column = table::column(size_header, |row: FileRowData| {
            let size_text = if row.entry.dir_type.is_some() {
                String::new()
            } else {
                format_size(row.entry.size)
            };
            shaped_text(size_text)
                .size(TEXT_SIZE)
                .wrapping(Wrapping::Word)
                .style(muted_text_style)
        })
        .width(FILE_SIZE_COLUMN_WIDTH)
        .align_x(Right);

        // Modified column header
        let modified_sort_icon: Element<'static, Message> =
            if deps.sort_column == FileSortColumn::Modified {
                let sort_icon = if deps.sort_ascending {
                    icon::down_dir()
                } else {
                    icon::up_dir()
                };
                sort_icon
                    .size(SORT_ICON_SIZE)
                    .style(muted_text_style)
                    .into()
            } else {
                Space::new().width(SORT_ICON_SIZE).into()
            };
        let modified_header_content: Element<'static, Message> = row![
            shaped_text(t("files-column-modified"))
                .size(TEXT_SIZE)
                .wrapping(Wrapping::Word)
                .style(muted_text_style),
            Space::new().width(Fill),
            Space::new().width(SORT_ICON_LEFT_MARGIN),
            modified_sort_icon,
            Space::new().width(SORT_ICON_RIGHT_MARGIN),
        ]
        .align_y(Center)
        .into();
        let modified_header: Element<'static, Message> = button(modified_header_content)
            .padding(NO_SPACING)
            .width(Fill)
            .style(transparent_icon_button_style)
            .on_press(Message::FileSortBy(FileSortColumn::Modified))
            .into();

        // Modified column
        let modified_column = table::column(modified_header, |row: FileRowData| {
            let date_text = format_timestamp(row.entry.modified);
            shaped_text(date_text)
                .size(TEXT_SIZE)
                .wrapping(Wrapping::Word)
                .style(muted_text_style)
        })
        .width(FILE_DATE_COLUMN_WIDTH)
        .align_x(Right);

        let columns = [name_column, size_column, modified_column];

        table(columns, deps.rows.clone())
            .width(Fill)
            .padding_x(SPACER_SIZE_SMALL)
            .padding_y(SPACER_SIZE_SMALL)
            .separator_x(NO_SPACING)
            .separator_y(SEPARATOR_HEIGHT)
    })
    .into()
}

/// Build context menu for lazy file table (takes owned data)
fn build_lazy_context_menu(
    entry_path: &str,
    entry_name: &str,
    is_dir: bool,
    can_upload: bool,
    perms: FilePermissions,
    has_clipboard: bool,
    bypass_via_ownership: bool,
) -> Element<'static, Message> {
    let mut menu_items: Vec<Element<'_, Message>> = vec![];
    let mut has_clipboard_section = false;

    // Download
    if perms.file_download {
        let download_message = if is_dir {
            Message::FileDownloadAll(entry_path.to_string())
        } else {
            Message::FileDownload(entry_path.to_string())
        };
        menu_items.push(
            MenuButton::new(shaped_text(t("context-menu-download")).size(TEXT_SIZE))
                .padding(CONTEXT_MENU_ITEM_PADDING)
                .width(Fill)
                .style(menu_button_style)
                .on_press(download_message)
                .into(),
        );
    }

    // Upload (compose folder flag with the file_upload_anywhere bypass)
    if is_dir && perms.effective_can_upload(can_upload) {
        menu_items.push(
            MenuButton::new(shaped_text(t("context-menu-upload")).size(TEXT_SIZE))
                .padding(CONTEXT_MENU_ITEM_PADDING)
                .width(Fill)
                .style(menu_button_style)
                .on_press(Message::FileUpload(entry_path.to_string()))
                .into(),
        );
    }

    // Share (always available - no special permission needed)
    menu_items.push(
        MenuButton::new(shaped_text(t("files-share")).size(TEXT_SIZE))
            .padding(CONTEXT_MENU_ITEM_PADDING)
            .width(Fill)
            .style(menu_button_style)
            .on_press(Message::FileShare(entry_path.to_string()))
            .into(),
    );

    // Clipboard separator
    let will_have_clipboard = perms.file_move || perms.file_copy;
    if will_have_clipboard {
        menu_items.push(
            container(Space::new())
                .width(Fill)
                .height(CONTEXT_MENU_SEPARATOR_HEIGHT)
                .style(separator_style)
                .into(),
        );
    }

    // Cut
    if perms.file_move {
        menu_items.push(
            MenuButton::new(shaped_text(t("files-cut")).size(TEXT_SIZE))
                .padding(CONTEXT_MENU_ITEM_PADDING)
                .width(Fill)
                .style(menu_button_style)
                .on_press(Message::FileCut(
                    entry_path.to_string(),
                    entry_name.to_string(),
                ))
                .into(),
        );
        has_clipboard_section = true;
    }

    // Copy
    if perms.file_copy {
        menu_items.push(
            MenuButton::new(shaped_text(t("files-copy")).size(TEXT_SIZE))
                .padding(CONTEXT_MENU_ITEM_PADDING)
                .width(Fill)
                .style(menu_button_style)
                .on_press(Message::FileCopyToClipboard(
                    entry_path.to_string(),
                    entry_name.to_string(),
                ))
                .into(),
        );
        has_clipboard_section = true;
    }

    // Paste
    if is_dir && has_clipboard && (perms.file_move || perms.file_copy) {
        menu_items.push(
            MenuButton::new(shaped_text(t("files-paste")).size(TEXT_SIZE))
                .padding(CONTEXT_MENU_ITEM_PADDING)
                .width(Fill)
                .style(menu_button_style)
                .on_press(Message::FilePasteInto(entry_path.to_string()))
                .into(),
        );
        has_clipboard_section = true;
    }

    // Normal actions separator
    if has_clipboard_section && (perms.file_info || perms.file_rename) {
        menu_items.push(
            container(Space::new())
                .width(Fill)
                .height(CONTEXT_MENU_SEPARATOR_HEIGHT)
                .style(separator_style)
                .into(),
        );
    }

    // Info
    if perms.file_info {
        menu_items.push(
            MenuButton::new(shaped_text(t("files-info")).size(TEXT_SIZE))
                .padding(CONTEXT_MENU_ITEM_PADDING)
                .width(Fill)
                .style(menu_button_style)
                .on_press(Message::FileInfoClicked(entry_name.to_string()))
                .into(),
        );
    }

    // Rename and Delete are both granted by either the global permission
    // or by ownership of the enclosing `[NEXUS-DB-username]` drop box.
    // Rename is safe under the bypass because the destination is always
    // in the same parent directory — it can't escape the drop box.
    let can_rename = perms.file_rename || bypass_via_ownership;
    let can_delete = perms.file_delete || bypass_via_ownership;

    // Rename
    if can_rename {
        menu_items.push(
            MenuButton::new(shaped_text(t("files-rename")).size(TEXT_SIZE))
                .padding(CONTEXT_MENU_ITEM_PADDING)
                .width(Fill)
                .style(menu_button_style)
                .on_press(Message::FileRenameClicked(entry_name.to_string()))
                .into(),
        );
    }

    // Delete separator (Share is always present, so there's always content before Delete)
    if can_delete {
        menu_items.push(
            container(Space::new())
                .width(Fill)
                .height(CONTEXT_MENU_SEPARATOR_HEIGHT)
                .style(separator_style)
                .into(),
        );
    }

    // Delete
    if can_delete {
        menu_items.push(
            MenuButton::new(shaped_text(t("files-delete")).size(TEXT_SIZE))
                .padding(CONTEXT_MENU_ITEM_PADDING)
                .width(Fill)
                .style(menu_button_danger_style)
                .on_press(Message::FileDeleteClicked(entry_path.to_string()))
                .into(),
        );
    }

    container(
        iced::widget::Column::with_children(menu_items).spacing(CONTEXT_MENU_SEPARATOR_MARGIN),
    )
    .width(CONTEXT_MENU_MIN_WIDTH)
    .padding(CONTEXT_MENU_PADDING)
    .style(context_menu_container_style)
    .into()
}
