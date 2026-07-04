//! Search results table and context menu

use iced::widget::text::Wrapping;
use iced::widget::{Space, button, container, lazy, row, table};
use iced::{Center, Element, Fill, Length};
use iced_aw::ContextMenu;
use nexus_common::protocol::FileSearchResult;

use super::super::helpers::{
    format_bytes as format_size, format_local_timestamp, sort_icon_or_placeholder,
};
use super::helpers::file_icon_for_extension;
use super::{FilePermissions, SearchResultsDeps};
use crate::i18n::t;
use crate::icon;
use crate::style::{
    CONTEXT_MENU_ITEM_PADDING, CONTEXT_MENU_MIN_WIDTH, CONTEXT_MENU_PADDING,
    CONTEXT_MENU_SEPARATOR_HEIGHT, CONTEXT_MENU_SEPARATOR_MARGIN, FILE_LIST_ICON_SIZE,
    FILE_LIST_ICON_SPACING, NO_SPACING, SCROLLBAR_PADDING, SEPARATOR_HEIGHT, SORT_ICON_LEFT_MARGIN,
    SORT_ICON_RIGHT_MARGIN, SPACER_SIZE_SMALL, TEXT_SIZE, context_menu_container_style,
    menu_button_style, muted_text_style, separator_style, shaped_text,
    transparent_icon_button_style,
};
use crate::types::{FileSortColumn, Message};
use crate::widgets::MenuButton;

/// Extract the parent directory path from a full file path
///
/// Used for displaying the "Path" column in search results, showing where each
/// result is located. Returns the parent directory with a leading slash for display
/// (e.g., "/Documents" for a file at "/Documents/file.txt").
pub(super) fn parent_path(path: &str) -> String {
    // Remove leading slash if present for processing
    let path = path.strip_prefix('/').unwrap_or(path);

    if let Some(pos) = path.rfind('/') {
        // Return everything before the last slash (with leading slash)
        format!("/{}", &path[..pos])
    } else {
        // No parent, return root
        String::new()
    }
}

/// Build search results table
pub(super) fn lazy_search_results_table(deps: SearchResultsDeps) -> Element<'static, Message> {
    lazy(deps, |deps| {
        // Name column header (clickable for sorting)
        let name_sort_icon = sort_icon_or_placeholder(
            deps.sort_column == FileSortColumn::Name,
            deps.sort_ascending,
        );
        let name_header_content: Element<'static, Message> = row![
            shaped_text(t("files-column-name"))
                .size(TEXT_SIZE)
                .style(muted_text_style)
                .wrapping(Wrapping::Word),
            Space::new().width(Fill),
            Space::new().width(SORT_ICON_LEFT_MARGIN),
            name_sort_icon,
            Space::new().width(SORT_ICON_RIGHT_MARGIN),
        ]
        .align_y(Center)
        .into();
        let name_header: Element<'static, Message> = button(name_header_content)
            .padding(NO_SPACING)
            .width(Length::Shrink)
            .style(transparent_icon_button_style)
            .on_press(Message::FileSearchSortBy(FileSortColumn::Name))
            .into();

        let perms = deps.perms;

        // Name column with icon - clickable to navigate
        let name_column = table::column(
            name_header,
            move |result: FileSearchResult| -> Element<'static, Message> {
                let is_directory = result.is_directory;

                // Icon based on type
                let icon_element: Element<'static, Message> = if is_directory {
                    icon::folder().size(FILE_LIST_ICON_SIZE).into()
                } else {
                    file_icon_for_extension(&result.name)
                        .size(FILE_LIST_ICON_SIZE)
                        .into()
                };

                let name_text = shaped_text(result.name.clone())
                    .size(TEXT_SIZE)
                    .wrapping(Wrapping::WordOrGlyph);

                let name_content: Element<'static, Message> = row![
                    icon_element,
                    Space::new().width(FILE_LIST_ICON_SPACING),
                    name_text,
                ]
                .align_y(iced::Alignment::Start)
                .into();

                // Left-click: directory → open in new tab, file → download.
                // Files without download permission render as plain text.
                let row_element: Element<'static, Message> = if is_directory {
                    button(name_content)
                        .padding(NO_SPACING)
                        .style(transparent_icon_button_style)
                        .on_press(Message::FileSearchResultClicked(result.clone()))
                        .into()
                } else if perms.file_download {
                    button(name_content)
                        .padding(NO_SPACING)
                        .style(transparent_icon_button_style)
                        .on_press(Message::FileSearchResultDownload(result.clone()))
                        .into()
                } else {
                    name_content
                };

                // Build context menu - Open is always available.
                ContextMenu::new(row_element, move || {
                    build_lazy_search_context_menu(result.clone(), perms)
                })
                .into()
            },
        )
        .width(Length::FillPortion(1));

        // Path column header (clickable for sorting)
        let path_sort_icon = sort_icon_or_placeholder(
            deps.sort_column == FileSortColumn::Path,
            deps.sort_ascending,
        );
        let path_header_content: Element<'static, Message> = row![
            shaped_text(t("files-column-path"))
                .size(TEXT_SIZE)
                .style(muted_text_style)
                .wrapping(Wrapping::Word),
            Space::new().width(Fill),
            Space::new().width(SORT_ICON_LEFT_MARGIN),
            path_sort_icon,
            Space::new().width(SORT_ICON_RIGHT_MARGIN),
        ]
        .align_y(Center)
        .into();
        let path_header: Element<'static, Message> = button(path_header_content)
            .padding(NO_SPACING)
            .width(Length::Shrink)
            .style(transparent_icon_button_style)
            .on_press(Message::FileSearchSortBy(FileSortColumn::Path))
            .into();

        // Path column - shows parent directory
        let path_column = table::column(
            path_header,
            |result: FileSearchResult| -> Element<'static, Message> {
                let display_path = parent_path(&result.path);

                // Show "/" for root, otherwise show the path
                let display = if display_path.is_empty() {
                    "/".to_string()
                } else {
                    display_path
                };

                shaped_text(display)
                    .size(TEXT_SIZE)
                    .style(muted_text_style)
                    .wrapping(Wrapping::WordOrGlyph)
                    .into()
            },
        )
        .width(Length::FillPortion(1));

        // Size column header (clickable for sorting)
        let size_sort_icon = sort_icon_or_placeholder(
            deps.sort_column == FileSortColumn::Size,
            deps.sort_ascending,
        );
        let size_header_content: Element<'static, Message> = row![
            shaped_text(t("files-column-size"))
                .size(TEXT_SIZE)
                .style(muted_text_style)
                .wrapping(Wrapping::Word),
            Space::new().width(Fill),
            Space::new().width(SORT_ICON_LEFT_MARGIN),
            size_sort_icon,
            Space::new().width(SORT_ICON_RIGHT_MARGIN),
        ]
        .align_y(Center)
        .into();
        let size_header: Element<'static, Message> = button(size_header_content)
            .padding(NO_SPACING)
            .width(Length::Shrink)
            .style(transparent_icon_button_style)
            .on_press(Message::FileSearchSortBy(FileSortColumn::Size))
            .into();

        // Size column
        let size_column = table::column(
            size_header,
            |result: FileSearchResult| -> Element<'static, Message> {
                let size_text = if result.is_directory {
                    String::from("—")
                } else {
                    format_size(result.size)
                };
                shaped_text(size_text)
                    .size(TEXT_SIZE)
                    .style(muted_text_style)
                    .wrapping(Wrapping::Word)
                    .into()
            },
        )
        .width(Length::Shrink);

        // Modified column header (clickable for sorting)
        let modified_sort_icon = sort_icon_or_placeholder(
            deps.sort_column == FileSortColumn::Modified,
            deps.sort_ascending,
        );
        let modified_header_content: Element<'static, Message> = row![
            shaped_text(t("files-column-modified"))
                .size(TEXT_SIZE)
                .style(muted_text_style)
                .wrapping(Wrapping::Word),
            Space::new().width(Fill),
            Space::new().width(SORT_ICON_LEFT_MARGIN),
            modified_sort_icon,
            Space::new().width(SORT_ICON_RIGHT_MARGIN),
            // Trailing gap so the rightmost column doesn't abut the scrollbar.
            Space::new().width(SCROLLBAR_PADDING),
        ]
        .align_y(Center)
        .into();
        let modified_header: Element<'static, Message> = button(modified_header_content)
            .padding(NO_SPACING)
            .width(Length::Shrink)
            .style(transparent_icon_button_style)
            .on_press(Message::FileSearchSortBy(FileSortColumn::Modified))
            .into();

        // Modified column. Trailing Space matches the header so the rightmost
        // column doesn't abut the scrollbar.
        let modified_column = table::column(
            modified_header,
            |result: FileSearchResult| -> Element<'static, Message> {
                let date_text = format_local_timestamp(result.modified);
                row![
                    shaped_text(date_text)
                        .size(TEXT_SIZE)
                        .style(muted_text_style)
                        .wrapping(Wrapping::Word),
                    Space::new().width(SCROLLBAR_PADDING),
                ]
                .align_y(Center)
                .into()
            },
        )
        .width(Length::Shrink);

        let columns = [name_column, path_column, size_column, modified_column];

        table(columns, deps.results.clone())
            .width(Fill)
            .padding_x(SPACER_SIZE_SMALL)
            .padding_y(SPACER_SIZE_SMALL)
            .separator_x(NO_SPACING)
            .separator_y(SEPARATOR_HEIGHT)
    })
    .into()
}

/// Build context menu for search results (lazy version with owned data)
fn build_lazy_search_context_menu(
    result: FileSearchResult,
    perms: FilePermissions,
) -> Element<'static, Message> {
    let mut menu_items: Vec<Element<'_, Message>> = vec![];

    // Download (if permission)
    if perms.file_download {
        menu_items.push(
            MenuButton::new(shaped_text(t("context-menu-download")).size(TEXT_SIZE))
                .padding(CONTEXT_MENU_ITEM_PADDING)
                .width(Fill)
                .style(menu_button_style)
                .on_press(Message::FileSearchResultDownload(result.clone()))
                .into(),
        );
    }

    // Search results do not carry folder type, so generic `file_upload`
    // cannot prove this directory accepts uploads. Only show the action
    // when the user has the explicit anywhere bypass.
    if perms.file_upload_anywhere && result.is_directory {
        menu_items.push(
            MenuButton::new(shaped_text(t("context-menu-upload")).size(TEXT_SIZE))
                .padding(CONTEXT_MENU_ITEM_PADDING)
                .width(Fill)
                .style(menu_button_style)
                .on_press(Message::FileUpload(result.path.clone()))
                .into(),
        );
    }

    // Share (always available - no special permission needed)
    menu_items.push(
        MenuButton::new(shaped_text(t("files-share")).size(TEXT_SIZE))
            .padding(CONTEXT_MENU_ITEM_PADDING)
            .width(Fill)
            .style(menu_button_style)
            .on_press(Message::FileShare(result.path.clone()))
            .into(),
    );

    // Separator before Info/Open section
    menu_items.push(
        container(Space::new())
            .width(Fill)
            .height(CONTEXT_MENU_SEPARATOR_HEIGHT)
            .style(separator_style)
            .into(),
    );

    // Info (if permission)
    if perms.file_info {
        menu_items.push(
            MenuButton::new(shaped_text(t("files-info")).size(TEXT_SIZE))
                .padding(CONTEXT_MENU_ITEM_PADDING)
                .width(Fill)
                .style(menu_button_style)
                .on_press(Message::FileSearchResultInfo(result.clone()))
                .into(),
        );
    }

    // Open (directory result opens itself, file result opens its parent directory)
    menu_items.push(
        MenuButton::new(shaped_text(t("context-menu-open")).size(TEXT_SIZE))
            .padding(CONTEXT_MENU_ITEM_PADDING)
            .width(Fill)
            .style(menu_button_style)
            .on_press(Message::FileSearchResultOpen(result.clone()))
            .into(),
    );

    container(
        iced::widget::Column::with_children(menu_items).spacing(CONTEXT_MENU_SEPARATOR_MARGIN),
    )
    .width(CONTEXT_MENU_MIN_WIDTH)
    .padding(CONTEXT_MENU_PADDING)
    .style(context_menu_container_style)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // parent_path Tests
    // =========================================================================

    #[test]
    fn test_parent_path_file_in_root() {
        assert_eq!(parent_path("/file.txt"), "");
    }

    #[test]
    fn test_parent_path_file_in_subdirectory() {
        assert_eq!(parent_path("/Documents/file.txt"), "/Documents");
    }

    #[test]
    fn test_parent_path_file_deeply_nested() {
        assert_eq!(
            parent_path("/Documents/Work/2024/report.pdf"),
            "/Documents/Work/2024"
        );
    }

    #[test]
    fn test_parent_path_directory_in_root() {
        assert_eq!(parent_path("/Documents"), "");
    }

    #[test]
    fn test_parent_path_directory_nested() {
        assert_eq!(parent_path("/Documents/Work"), "/Documents");
    }

    #[test]
    fn test_parent_path_no_leading_slash() {
        assert_eq!(parent_path("Documents/file.txt"), "/Documents");
    }

    #[test]
    fn test_parent_path_single_segment_no_slash() {
        assert_eq!(parent_path("file.txt"), "");
    }

    #[test]
    fn test_parent_path_empty() {
        assert_eq!(parent_path(""), "");
    }
}
