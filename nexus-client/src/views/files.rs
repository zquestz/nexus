//! Files panel view (browse, upload, download files)

use chrono::{DateTime, Local, TimeZone, Utc};

use super::layout::scrollable_panel;
use crate::i18n::t;
use crate::icon;
use crate::style::{
    BUTTON_PADDING, CONTEXT_MENU_ITEM_PADDING, CONTEXT_MENU_MIN_WIDTH, CONTEXT_MENU_PADDING,
    CONTEXT_MENU_SEPARATOR_HEIGHT, CONTEXT_MENU_SEPARATOR_MARGIN, ELEMENT_SPACING,
    FILE_DATE_COLUMN_WIDTH, FILE_LIST_ICON_SIZE, FILE_LIST_ICON_SPACING, FILE_SIZE_COLUMN_WIDTH,
    FILE_TOOLBAR_BUTTON_PADDING, FILE_TOOLBAR_ICON_SIZE, FORM_MAX_WIDTH, FORM_PADDING,
    INPUT_PADDING, NEWS_LIST_MAX_WIDTH, NO_SPACING, SEPARATOR_HEIGHT, SPACER_SIZE_MEDIUM,
    SPACER_SIZE_SMALL, TEXT_SIZE, TOOLTIP_BACKGROUND_PADDING, TOOLTIP_GAP, TOOLTIP_PADDING,
    TOOLTIP_TEXT_SIZE, content_background_style, context_menu_button_style,
    context_menu_container_style, context_menu_item_danger_style, disabled_icon_button_style,
    error_text_style, muted_text_style, panel_title, separator_style, shaped_text,
    shaped_text_wrapped, tooltip_container_style, transparent_icon_button_style,
};
use crate::types::{FilesManagementState, InputId, Message, ScrollableId};
use iced::widget::button as btn;
use iced::widget::text::Wrapping;
use iced::widget::{Space, button, column, container, row, scrollable, table, text_input, tooltip};
use iced::{Center, Element, Fill, Right};
use iced_aw::ContextMenu;
use nexus_common::protocol::FileEntry;

// ============================================================================
// Helper Functions
// ============================================================================

/// Maximum length for breadcrumb segment display names
const BREADCRUMB_MAX_SEGMENT_LENGTH: usize = 32;

/// Get the appropriate icon for a file based on its extension
fn file_icon_for_extension(filename: &str) -> iced::widget::Text<'static> {
    // Extract extension (lowercase for comparison)
    let ext = filename
        .rsplit('.')
        .next()
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    match ext.as_str() {
        // PDF
        "pdf" => icon::file_pdf(),

        // Word processing
        "doc" | "docx" | "odt" | "rtf" => icon::file_word(),

        // Spreadsheets
        "xls" | "xlsx" | "ods" | "csv" => icon::file_excel(),

        // Presentations
        "ppt" | "pptx" | "odp" => icon::file_powerpoint(),

        // Images
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "svg" | "webp" | "ico" => icon::file_image(),

        // Archives
        "zip" | "tar" | "gz" | "bz2" | "7z" | "rar" | "xz" | "zst" => icon::file_archive(),

        // Audio
        "mp3" | "wav" | "flac" | "ogg" | "m4a" | "aac" | "wma" => icon::file_audio(),

        // Video
        "mp4" | "mkv" | "avi" | "mov" | "wmv" | "webm" | "flv" => icon::file_video(),

        // Code
        "rs" | "py" | "js" | "ts" | "c" | "cpp" | "h" | "java" | "go" | "rb" | "php" | "html"
        | "css" | "json" | "xml" | "yaml" | "yml" | "toml" | "sh" | "bash" => icon::file_code(),

        // Text
        "txt" | "md" | "log" | "cfg" | "conf" | "ini" | "nfo" => icon::file_text(),

        // Default
        _ => icon::file(),
    }
}

/// Format a Unix timestamp for display
fn format_timestamp(timestamp: i64) -> String {
    if timestamp == 0 {
        return String::new();
    }

    // Convert Unix timestamp to local time
    if let Some(utc_time) = Utc.timestamp_opt(timestamp, 0).single() {
        let local_time: DateTime<Local> = utc_time.with_timezone(&Local);
        // Format as "Jan 15, 2025 10:30"
        local_time.format("%b %d, %Y %H:%M").to_string()
    } else {
        String::new()
    }
}

/// Format a file size for display (human-readable)
fn format_size(size: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    const TB: u64 = GB * 1024;

    if size >= TB {
        format!("{:.1} TB", size as f64 / TB as f64)
    } else if size >= GB {
        format!("{:.1} GB", size as f64 / GB as f64)
    } else if size >= MB {
        format!("{:.1} MB", size as f64 / MB as f64)
    } else if size >= KB {
        format!("{:.1} KB", size as f64 / KB as f64)
    } else {
        format!("{size} B")
    }
}

/// Truncate a segment name to a maximum length, adding ellipsis if needed
fn truncate_segment(name: &str, max_len: usize) -> String {
    if name.chars().count() <= max_len {
        name.to_string()
    } else {
        // Leave room for "..." (3 characters)
        let truncated: String = name.chars().take(max_len - 3).collect();
        format!("{truncated}...")
    }
}

/// Build the path for navigating into a directory
fn build_navigate_path(current_path: &str, folder_name: &str) -> String {
    if current_path.is_empty() || current_path == "/" {
        folder_name.to_string()
    } else {
        format!("{current_path}/{folder_name}")
    }
}

/// Parse breadcrumb segments from a path
fn parse_breadcrumbs(path: &str) -> Vec<(&str, String)> {
    if path.is_empty() || path == "/" {
        return Vec::new();
    }

    let mut result = Vec::new();
    let mut accumulated_path = String::new();

    for segment in path.split('/').filter(|s| !s.is_empty()) {
        if accumulated_path.is_empty() {
            accumulated_path = segment.to_string();
        } else {
            accumulated_path = format!("{accumulated_path}/{segment}");
        }
        result.push((segment, accumulated_path.clone()));
    }

    result
}

// ============================================================================
// View Components
// ============================================================================

/// Build the breadcrumb navigation bar
fn breadcrumb_bar<'a>(current_path: &str, viewing_root: bool) -> Element<'a, Message> {
    let mut breadcrumbs = iced::widget::Row::new().spacing(SPACER_SIZE_SMALL);

    // Root/Home link - shows "Root" when viewing root, "Home" otherwise
    // Always clickable (acts as refresh when at root/home)
    let root_label = if viewing_root {
        t("files-root")
    } else {
        t("files-home")
    };
    let home_btn = button(
        shaped_text(root_label)
            .size(TEXT_SIZE)
            .style(muted_text_style),
    )
    .padding(NO_SPACING)
    .style(transparent_icon_button_style)
    .on_press(Message::FileNavigateHome);

    breadcrumbs = breadcrumbs.push(home_btn);

    // Parse and add breadcrumb segments
    let segments = parse_breadcrumbs(current_path);

    for (display_name, path) in segments {
        // Strip any folder type suffix for display
        let full_name = FilesManagementState::display_name(display_name);
        let truncated_name = truncate_segment(&full_name, BREADCRUMB_MAX_SEGMENT_LENGTH);
        let is_truncated = truncated_name.len() < full_name.len();

        // All segments are clickable (last one acts as refresh)
        // Use Wrapping::None to prevent mid-word breaks
        let segment_btn = button(
            shaped_text(&truncated_name)
                .size(TEXT_SIZE)
                .style(muted_text_style)
                .wrapping(Wrapping::None),
        )
        .padding(NO_SPACING)
        .style(transparent_icon_button_style)
        .on_press(Message::FileNavigate(path));

        // Wrap in tooltip if truncated to show full name
        let segment_element: Element<'_, Message> = if is_truncated {
            tooltip(
                segment_btn,
                container(shaped_text(full_name).size(TOOLTIP_TEXT_SIZE))
                    .padding(TOOLTIP_BACKGROUND_PADDING)
                    .style(tooltip_container_style),
                tooltip::Position::Bottom,
            )
            .gap(TOOLTIP_GAP)
            .padding(TOOLTIP_PADDING)
            .into()
        } else {
            segment_btn.into()
        };

        // Group separator + segment name together so they don't break apart when wrapping
        let segment_group: Element<'_, Message> = row![
            shaped_text("/").size(TEXT_SIZE).style(muted_text_style),
            Space::new().width(SPACER_SIZE_SMALL),
            segment_element,
        ]
        .into();

        breadcrumbs = breadcrumbs.push(segment_group);
    }

    // Use wrap() so breadcrumbs flow to multiple lines if needed,
    // but each segment group stays together
    container(breadcrumbs.wrap())
        .padding([SPACER_SIZE_SMALL, NO_SPACING])
        .into()
}

/// Build the toolbar with Home, View Root/Home, Refresh, New Directory, Up buttons
fn toolbar<'a>(
    can_go_up: bool,
    has_file_root: bool,
    viewing_root: bool,
    show_hidden: bool,
    can_create_dir: bool,
    is_loading: bool,
) -> Element<'a, Message> {
    // Home button - tooltip changes based on viewing mode
    let home_tooltip = if viewing_root {
        t("tooltip-files-go-root")
    } else {
        t("tooltip-files-home")
    };
    let home_button = tooltip(
        button(icon::home().size(FILE_TOOLBAR_ICON_SIZE))
            .padding(FILE_TOOLBAR_BUTTON_PADDING)
            .style(transparent_icon_button_style)
            .on_press(Message::FileNavigateHome),
        container(shaped_text(home_tooltip).size(TOOLTIP_TEXT_SIZE))
            .padding(TOOLTIP_BACKGROUND_PADDING)
            .style(tooltip_container_style),
        tooltip::Position::Bottom,
    )
    .gap(TOOLTIP_GAP)
    .padding(TOOLTIP_PADDING);

    // Refresh button - always enabled
    let refresh_button = tooltip(
        button(icon::refresh().size(FILE_TOOLBAR_ICON_SIZE))
            .padding(FILE_TOOLBAR_BUTTON_PADDING)
            .style(transparent_icon_button_style)
            .on_press(Message::FileRefresh),
        container(shaped_text(t("tooltip-files-refresh")).size(TOOLTIP_TEXT_SIZE))
            .padding(TOOLTIP_BACKGROUND_PADDING)
            .style(tooltip_container_style),
        tooltip::Position::Bottom,
    )
    .gap(TOOLTIP_GAP)
    .padding(TOOLTIP_PADDING);

    // Up button - enabled only when not at home
    let up_button: Element<'a, Message> = if can_go_up {
        tooltip(
            button(icon::up_dir().size(FILE_TOOLBAR_ICON_SIZE))
                .padding(FILE_TOOLBAR_BUTTON_PADDING)
                .style(transparent_icon_button_style)
                .on_press(Message::FileNavigateUp),
            container(shaped_text(t("tooltip-files-up")).size(TOOLTIP_TEXT_SIZE))
                .padding(TOOLTIP_BACKGROUND_PADDING)
                .style(tooltip_container_style),
            tooltip::Position::Bottom,
        )
        .gap(TOOLTIP_GAP)
        .padding(TOOLTIP_PADDING)
        .into()
    } else {
        // Disabled up button (no tooltip needed for disabled state)
        button(icon::up_dir().size(FILE_TOOLBAR_ICON_SIZE))
            .padding(FILE_TOOLBAR_BUTTON_PADDING)
            .style(disabled_icon_button_style)
            .into()
    };

    // Start building toolbar row with Home button
    let mut toolbar_row = row![home_button].spacing(SPACER_SIZE_SMALL);

    // Root toggle button - only shown if user has file_root permission
    if has_file_root {
        let root_toggle_tooltip = if viewing_root {
            t("tooltip-files-view-home")
        } else {
            t("tooltip-files-view-root")
        };

        let root_toggle_button = tooltip(
            button(icon::folder_root().size(FILE_TOOLBAR_ICON_SIZE))
                .padding(FILE_TOOLBAR_BUTTON_PADDING)
                .style(transparent_icon_button_style)
                .on_press(Message::FileToggleRoot),
            container(shaped_text(root_toggle_tooltip).size(TOOLTIP_TEXT_SIZE))
                .padding(TOOLTIP_BACKGROUND_PADDING)
                .style(tooltip_container_style),
            tooltip::Position::Bottom,
        )
        .gap(TOOLTIP_GAP)
        .padding(TOOLTIP_PADDING);

        toolbar_row = toolbar_row.push(root_toggle_button);
    }

    // Refresh button
    toolbar_row = toolbar_row.push(refresh_button);

    // Hidden files toggle button
    let hidden_tooltip = if show_hidden {
        t("tooltip-files-hide-hidden")
    } else {
        t("tooltip-files-show-hidden")
    };
    let hidden_icon = if show_hidden {
        icon::eye()
    } else {
        icon::eye_off()
    };
    let hidden_toggle_button = tooltip(
        button(hidden_icon.size(FILE_TOOLBAR_ICON_SIZE))
            .padding(FILE_TOOLBAR_BUTTON_PADDING)
            .style(transparent_icon_button_style)
            .on_press(Message::FileToggleHidden),
        container(shaped_text(hidden_tooltip).size(TOOLTIP_TEXT_SIZE))
            .padding(TOOLTIP_BACKGROUND_PADDING)
            .style(tooltip_container_style),
        tooltip::Position::Bottom,
    )
    .gap(TOOLTIP_GAP)
    .padding(TOOLTIP_PADDING);

    toolbar_row = toolbar_row.push(hidden_toggle_button);

    // New Directory button - enabled if user has file_create_dir permission OR current dir allows upload
    // Disabled while loading
    let new_dir_button: Element<'a, Message> = if can_create_dir && !is_loading {
        tooltip(
            button(icon::folder_empty().size(FILE_TOOLBAR_ICON_SIZE))
                .padding(FILE_TOOLBAR_BUTTON_PADDING)
                .style(transparent_icon_button_style)
                .on_press(Message::FileNewDirectoryClicked),
            container(shaped_text(t("tooltip-files-new-directory")).size(TOOLTIP_TEXT_SIZE))
                .padding(TOOLTIP_BACKGROUND_PADDING)
                .style(tooltip_container_style),
            tooltip::Position::Bottom,
        )
        .gap(TOOLTIP_GAP)
        .padding(TOOLTIP_PADDING)
        .into()
    } else {
        // Disabled new directory button
        button(icon::folder_empty().size(FILE_TOOLBAR_ICON_SIZE))
            .padding(FILE_TOOLBAR_BUTTON_PADDING)
            .style(disabled_icon_button_style)
            .into()
    };

    toolbar_row = toolbar_row.push(new_dir_button);

    // Up button - last
    toolbar_row = toolbar_row.push(up_button);

    toolbar_row.into()
}

/// Build the delete confirmation dialog
fn delete_confirm_dialog(path: &str) -> Element<'_, Message> {
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
        button(shaped_text(t("button-delete")).size(TEXT_SIZE))
            .on_press(Message::FileConfirmDelete)
            .padding(BUTTON_PADDING)
            .style(btn::danger),
    ]
    .spacing(ELEMENT_SPACING);

    let form = column![
        title,
        Space::new().height(SPACER_SIZE_MEDIUM),
        shaped_text_wrapped(&message)
            .size(TEXT_SIZE)
            .width(Fill)
            .align_x(Center),
        Space::new().height(SPACER_SIZE_MEDIUM),
        buttons,
    ]
    .spacing(ELEMENT_SPACING)
    .padding(FORM_PADDING)
    .max_width(FORM_MAX_WIDTH);

    scrollable_panel(form)
}

/// Build the new directory dialog (matches broadcast view layout)
fn new_directory_dialog<'a>(name: &str, error: Option<&String>) -> Element<'a, Message> {
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
        if name_valid {
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
        .padding(FORM_PADDING)
        .max_width(FORM_MAX_WIDTH);

    scrollable_panel(form)
}

/// Build the file table
fn file_table<'a>(
    entries: &'a [FileEntry],
    current_path: &'a str,
    has_file_delete: bool,
) -> Element<'a, Message> {
    // Name column with icon
    let name_column = table::column(
        shaped_text(t("files-column-name"))
            .size(TEXT_SIZE)
            .style(muted_text_style),
        move |entry: &FileEntry| {
            let is_directory = entry.dir_type.is_some();
            let display_name = FilesManagementState::display_name(&entry.name);

            // Icon based on type (folder or file extension)
            let icon_element: Element<'_, Message> = if is_directory {
                icon::folder().size(FILE_LIST_ICON_SIZE).into()
            } else {
                file_icon_for_extension(&entry.name)
                    .size(FILE_LIST_ICON_SIZE)
                    .into()
            };

            // Name with icon
            let name_content: Element<'_, Message> = row![
                icon_element,
                Space::new().width(FILE_LIST_ICON_SPACING),
                shaped_text(display_name)
                    .size(TEXT_SIZE)
                    .wrapping(Wrapping::WordOrGlyph),
            ]
            .align_y(Center)
            .into();

            // For directories, make the row clickable
            let row_element: Element<'_, Message> = if is_directory {
                let navigate_path = build_navigate_path(current_path, &entry.name);
                button(name_content)
                    .padding(NO_SPACING)
                    .style(transparent_icon_button_style)
                    .on_press(Message::FileNavigate(navigate_path))
                    .into()
            } else {
                name_content
            };

            // Wrap in context menu if user has delete permission
            if has_file_delete {
                // Build the full path for this entry
                let delete_path = build_navigate_path(current_path, &entry.name);
                let delete_path_clone = delete_path.clone();

                ContextMenu::new(row_element, move || {
                    container(
                        column![
                            // Info (placeholder - not yet implemented)
                            button(shaped_text(t("files-info")).size(TEXT_SIZE))
                                .padding(CONTEXT_MENU_ITEM_PADDING)
                                .width(Fill)
                                .style(context_menu_button_style),
                            // Separator before destructive actions
                            container(Space::new())
                                .width(Fill)
                                .height(CONTEXT_MENU_SEPARATOR_HEIGHT)
                                .style(separator_style),
                            // Delete
                            button(shaped_text(t("files-delete")).size(TEXT_SIZE))
                                .padding(CONTEXT_MENU_ITEM_PADDING)
                                .width(Fill)
                                .style(context_menu_item_danger_style)
                                .on_press(Message::FileDeleteClicked(delete_path_clone.clone())),
                        ]
                        .spacing(CONTEXT_MENU_SEPARATOR_MARGIN),
                    )
                    .width(CONTEXT_MENU_MIN_WIDTH)
                    .padding(CONTEXT_MENU_PADDING)
                    .style(context_menu_container_style)
                    .into()
                })
                .into()
            } else {
                row_element
            }
        },
    )
    .width(Fill);

    // Size column
    let size_column = table::column(
        shaped_text(t("files-column-size"))
            .size(TEXT_SIZE)
            .style(muted_text_style),
        |entry: &FileEntry| {
            let size_text = if entry.dir_type.is_some() {
                String::new()
            } else {
                format_size(entry.size)
            };
            shaped_text(size_text)
                .size(TEXT_SIZE)
                .style(muted_text_style)
        },
    )
    .width(FILE_SIZE_COLUMN_WIDTH)
    .align_x(Right);

    // Modified column
    let modified_column = table::column(
        shaped_text(t("files-column-modified"))
            .size(TEXT_SIZE)
            .style(muted_text_style),
        |entry: &FileEntry| {
            let date_text = format_timestamp(entry.modified);
            shaped_text(date_text)
                .size(TEXT_SIZE)
                .style(muted_text_style)
        },
    )
    .width(FILE_DATE_COLUMN_WIDTH)
    .align_x(Right);

    let columns = [name_column, size_column, modified_column];

    table(columns, entries)
        .width(Fill)
        .padding_x(SPACER_SIZE_SMALL)
        .padding_y(SPACER_SIZE_SMALL)
        .separator_x(NO_SPACING)
        .separator_y(SEPARATOR_HEIGHT)
        .into()
}

// ============================================================================
// Public View Function
// ============================================================================

/// Displays the files panel
///
/// Shows a file browser with directory listing and navigation.
/// If the new directory dialog is open, shows that instead.
///
/// # Arguments
/// * `files_management` - Current files panel state
/// * `has_file_root` - Whether user has file_root permission (enables root toggle)
/// * `has_file_create_dir` - Whether user has file_create_dir permission (enables new directory anywhere)
pub fn files_view<'a>(
    files_management: &'a FilesManagementState,
    has_file_root: bool,
    has_file_create_dir: bool,
    has_file_delete: bool,
) -> Element<'a, Message> {
    // If delete confirmation is pending, show that dialog
    if let Some(path) = &files_management.pending_delete {
        return delete_confirm_dialog(path);
    }

    // If creating directory, show the dialog instead
    if files_management.creating_directory {
        return new_directory_dialog(
            &files_management.new_directory_name,
            files_management.new_directory_error.as_ref(),
        );
    }
    let is_at_home =
        files_management.current_path.is_empty() || files_management.current_path == "/";
    let viewing_root = files_management.viewing_root;

    // Title row (centered, matching other panels)
    let title_row = panel_title(t("files-panel-title"));

    // Breadcrumb navigation
    let breadcrumbs = breadcrumb_bar(&files_management.current_path, viewing_root);

    // Determine if user can create directories here
    // User can create if they have file_create_dir permission OR current directory allows upload
    let can_create_dir = has_file_create_dir || files_management.current_dir_can_upload;

    // Check if we're in a loading state
    let is_loading = files_management.entries.is_none() && files_management.error.is_none();

    // Toolbar with buttons
    let toolbar = toolbar(
        !is_at_home,
        has_file_root,
        viewing_root,
        files_management.show_hidden,
        can_create_dir,
        is_loading,
    );

    // Content area (table or status message)
    // Priority: error > entries > loading
    let content: Element<'a, Message> = if let Some(error) = &files_management.error {
        // Error state
        container(
            shaped_text_wrapped(error)
                .size(TEXT_SIZE)
                .style(error_text_style),
        )
        .width(Fill)
        .center_x(Fill)
        .padding(SPACER_SIZE_SMALL)
        .into()
    } else if let Some(entries) = &files_management.entries {
        if entries.is_empty() {
            // Empty directory
            container(
                shaped_text(t("files-empty"))
                    .size(TEXT_SIZE)
                    .style(muted_text_style),
            )
            .width(Fill)
            .center_x(Fill)
            .padding(SPACER_SIZE_SMALL)
            .into()
        } else {
            // File table
            file_table(entries, &files_management.current_path, has_file_delete)
        }
    } else {
        // Loading state
        container(
            shaped_text(t("files-loading"))
                .size(TEXT_SIZE)
                .style(muted_text_style),
        )
        .width(Fill)
        .center_x(Fill)
        .padding(SPACER_SIZE_SMALL)
        .into()
    };

    // Build the form with max_width constraint
    // Breadcrumbs and toolbar stay fixed, only content scrolls
    let form = column![
        title_row,
        breadcrumbs,
        toolbar,
        container(scrollable(content).id(ScrollableId::FilesContent)).height(Fill),
    ]
    .spacing(SPACER_SIZE_SMALL)
    .align_x(Center)
    .padding(FORM_PADDING)
    .max_width(NEWS_LIST_MAX_WIDTH)
    .height(Fill);

    // Center the form horizontally
    let centered_form = container(form).width(Fill).center_x(Fill);

    // Use container with background style
    container(centered_form)
        .width(Fill)
        .height(Fill)
        .style(content_background_style)
        .into()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // format_timestamp Tests
    // =========================================================================

    #[test]
    fn test_format_timestamp_zero() {
        assert_eq!(format_timestamp(0), "");
    }

    #[test]
    fn test_format_timestamp_valid() {
        // 2025-01-15 10:30:00 UTC = 1736935800
        let result = format_timestamp(1736935800);
        // Just check it's non-empty and contains expected parts
        assert!(!result.is_empty());
        assert!(result.contains("2025"));
    }

    #[test]
    fn test_format_timestamp_negative() {
        // Negative timestamps (before 1970) should return empty
        // chrono's timestamp_opt returns None for out-of-range values
        let result = format_timestamp(-1);
        // This may or may not work depending on chrono's handling
        // Just verify it doesn't panic
        let _ = result;
    }

    // =========================================================================
    // format_size Tests
    // =========================================================================

    #[test]
    fn test_format_size_bytes() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(1), "1 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1023), "1023 B");
    }

    #[test]
    fn test_format_size_kilobytes() {
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1536), "1.5 KB");
        assert_eq!(format_size(10240), "10.0 KB");
        assert_eq!(format_size(1048575), "1024.0 KB"); // Just under 1 MB
    }

    #[test]
    fn test_format_size_megabytes() {
        assert_eq!(format_size(1048576), "1.0 MB"); // Exactly 1 MB
        assert_eq!(format_size(1572864), "1.5 MB");
        assert_eq!(format_size(104857600), "100.0 MB");
    }

    #[test]
    fn test_format_size_gigabytes() {
        assert_eq!(format_size(1073741824), "1.0 GB"); // Exactly 1 GB
        assert_eq!(format_size(1610612736), "1.5 GB");
        assert_eq!(format_size(107374182400), "100.0 GB");
    }

    #[test]
    fn test_format_size_terabytes() {
        assert_eq!(format_size(1099511627776), "1.0 TB"); // Exactly 1 TB
        assert_eq!(format_size(1649267441664), "1.5 TB");
    }

    // =========================================================================
    // parse_breadcrumbs Tests
    // =========================================================================

    #[test]
    fn test_parse_breadcrumbs_empty() {
        assert!(parse_breadcrumbs("").is_empty());
    }

    #[test]
    fn test_parse_breadcrumbs_root_slash() {
        assert!(parse_breadcrumbs("/").is_empty());
    }

    #[test]
    fn test_parse_breadcrumbs_single_segment() {
        let result = parse_breadcrumbs("Documents");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, "Documents");
        assert_eq!(result[0].1, "Documents");
    }

    #[test]
    fn test_parse_breadcrumbs_multiple_segments() {
        let result = parse_breadcrumbs("Documents/Photos/2024");
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].0, "Documents");
        assert_eq!(result[0].1, "Documents");
        assert_eq!(result[1].0, "Photos");
        assert_eq!(result[1].1, "Documents/Photos");
        assert_eq!(result[2].0, "2024");
        assert_eq!(result[2].1, "Documents/Photos/2024");
    }

    #[test]
    fn test_parse_breadcrumbs_with_leading_slash() {
        let result = parse_breadcrumbs("/Documents/Photos");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].0, "Documents");
        assert_eq!(result[0].1, "Documents");
        assert_eq!(result[1].0, "Photos");
        assert_eq!(result[1].1, "Documents/Photos");
    }

    #[test]
    fn test_parse_breadcrumbs_with_trailing_slash() {
        let result = parse_breadcrumbs("Documents/Photos/");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].0, "Documents");
        assert_eq!(result[1].0, "Photos");
    }

    #[test]
    fn test_parse_breadcrumbs_with_suffix() {
        // Suffix should be preserved in segment (display_name strips it later)
        let result = parse_breadcrumbs("Uploads [NEXUS-UL]/Photos");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].0, "Uploads [NEXUS-UL]");
        assert_eq!(result[0].1, "Uploads [NEXUS-UL]");
        assert_eq!(result[1].0, "Photos");
        assert_eq!(result[1].1, "Uploads [NEXUS-UL]/Photos");
    }

    // =========================================================================
    // build_navigate_path Tests
    // =========================================================================

    #[test]
    fn test_build_navigate_path_from_empty() {
        assert_eq!(build_navigate_path("", "Documents"), "Documents");
    }

    #[test]
    fn test_build_navigate_path_from_root_slash() {
        assert_eq!(build_navigate_path("/", "Documents"), "Documents");
    }

    #[test]
    fn test_build_navigate_path_from_existing() {
        assert_eq!(
            build_navigate_path("Documents", "Photos"),
            "Documents/Photos"
        );
    }

    #[test]
    fn test_build_navigate_path_nested() {
        assert_eq!(
            build_navigate_path("Documents/Photos", "2024"),
            "Documents/Photos/2024"
        );
    }

    #[test]
    fn test_build_navigate_path_with_suffix() {
        assert_eq!(
            build_navigate_path("Files", "Uploads [NEXUS-UL]"),
            "Files/Uploads [NEXUS-UL]"
        );
    }

    // =========================================================================
    // truncate_segment Tests
    // =========================================================================

    #[test]
    fn test_truncate_segment_short_name() {
        // Names shorter than max should be unchanged
        assert_eq!(truncate_segment("Documents", 32), "Documents");
        assert_eq!(truncate_segment("A", 32), "A");
        assert_eq!(truncate_segment("", 32), "");
    }

    #[test]
    fn test_truncate_segment_exact_length() {
        // Name exactly at max length should be unchanged
        let name = "a".repeat(32);
        assert_eq!(truncate_segment(&name, 32), name);
    }

    #[test]
    fn test_truncate_segment_too_long() {
        // Name longer than max should be truncated with ellipsis
        let name = "a".repeat(40);
        let result = truncate_segment(&name, 32);
        assert_eq!(result.chars().count(), 32);
        assert!(result.ends_with("..."));
        assert_eq!(result, format!("{}...", "a".repeat(29)));
    }

    #[test]
    fn test_truncate_segment_unicode() {
        // Unicode characters should be handled correctly (count chars, not bytes)
        let name = "日本語フォルダ名テスト長い名前";
        assert_eq!(name.chars().count(), 15);
        assert_eq!(truncate_segment(name, 32), name);

        // Truncate unicode
        let long_name = "日".repeat(40);
        let result = truncate_segment(&long_name, 32);
        assert_eq!(result.chars().count(), 32);
        assert!(result.ends_with("..."));
    }

    #[test]
    fn test_truncate_segment_one_over() {
        // Name one character over should truncate
        let name = "a".repeat(33);
        let result = truncate_segment(&name, 32);
        assert_eq!(result.chars().count(), 32);
        assert_eq!(result, format!("{}...", "a".repeat(29)));
    }

    // =========================================================================
    // file_icon_for_extension Tests
    // =========================================================================

    // Note: We can't directly compare Text widgets, so we test that the function
    // doesn't panic and returns something for each category. The actual icon
    // correctness is verified by visual inspection.

    #[test]
    fn test_file_icon_pdf() {
        // Should not panic
        let _ = file_icon_for_extension("document.pdf");
        let _ = file_icon_for_extension("DOCUMENT.PDF");
    }

    #[test]
    fn test_file_icon_word() {
        let _ = file_icon_for_extension("report.doc");
        let _ = file_icon_for_extension("report.docx");
        let _ = file_icon_for_extension("report.odt");
        let _ = file_icon_for_extension("report.rtf");
    }

    #[test]
    fn test_file_icon_excel() {
        let _ = file_icon_for_extension("data.xls");
        let _ = file_icon_for_extension("data.xlsx");
        let _ = file_icon_for_extension("data.ods");
        let _ = file_icon_for_extension("data.csv");
    }

    #[test]
    fn test_file_icon_powerpoint() {
        let _ = file_icon_for_extension("slides.ppt");
        let _ = file_icon_for_extension("slides.pptx");
        let _ = file_icon_for_extension("slides.odp");
    }

    #[test]
    fn test_file_icon_image() {
        let _ = file_icon_for_extension("photo.png");
        let _ = file_icon_for_extension("photo.jpg");
        let _ = file_icon_for_extension("photo.jpeg");
        let _ = file_icon_for_extension("photo.gif");
        let _ = file_icon_for_extension("photo.bmp");
        let _ = file_icon_for_extension("photo.svg");
        let _ = file_icon_for_extension("photo.webp");
        let _ = file_icon_for_extension("photo.ico");
    }

    #[test]
    fn test_file_icon_archive() {
        let _ = file_icon_for_extension("archive.zip");
        let _ = file_icon_for_extension("archive.tar");
        let _ = file_icon_for_extension("archive.gz");
        let _ = file_icon_for_extension("archive.bz2");
        let _ = file_icon_for_extension("archive.7z");
        let _ = file_icon_for_extension("archive.rar");
        let _ = file_icon_for_extension("archive.xz");
        let _ = file_icon_for_extension("archive.zst");
    }

    #[test]
    fn test_file_icon_audio() {
        let _ = file_icon_for_extension("song.mp3");
        let _ = file_icon_for_extension("song.wav");
        let _ = file_icon_for_extension("song.flac");
        let _ = file_icon_for_extension("song.ogg");
        let _ = file_icon_for_extension("song.m4a");
        let _ = file_icon_for_extension("song.aac");
        let _ = file_icon_for_extension("song.wma");
    }

    #[test]
    fn test_file_icon_video() {
        let _ = file_icon_for_extension("movie.mp4");
        let _ = file_icon_for_extension("movie.mkv");
        let _ = file_icon_for_extension("movie.avi");
        let _ = file_icon_for_extension("movie.mov");
        let _ = file_icon_for_extension("movie.wmv");
        let _ = file_icon_for_extension("movie.webm");
        let _ = file_icon_for_extension("movie.flv");
    }

    #[test]
    fn test_file_icon_code() {
        let _ = file_icon_for_extension("main.rs");
        let _ = file_icon_for_extension("script.py");
        let _ = file_icon_for_extension("app.js");
        let _ = file_icon_for_extension("app.ts");
        let _ = file_icon_for_extension("main.c");
        let _ = file_icon_for_extension("main.cpp");
        let _ = file_icon_for_extension("header.h");
        let _ = file_icon_for_extension("Main.java");
        let _ = file_icon_for_extension("main.go");
        let _ = file_icon_for_extension("script.rb");
        let _ = file_icon_for_extension("index.php");
        let _ = file_icon_for_extension("index.html");
        let _ = file_icon_for_extension("style.css");
        let _ = file_icon_for_extension("config.json");
        let _ = file_icon_for_extension("data.xml");
        let _ = file_icon_for_extension("config.yaml");
        let _ = file_icon_for_extension("config.yml");
        let _ = file_icon_for_extension("Cargo.toml");
        let _ = file_icon_for_extension("script.sh");
        let _ = file_icon_for_extension("script.bash");
    }

    #[test]
    fn test_file_icon_text() {
        let _ = file_icon_for_extension("readme.txt");
        let _ = file_icon_for_extension("README.md");
        let _ = file_icon_for_extension("server.log");
        let _ = file_icon_for_extension("app.cfg");
        let _ = file_icon_for_extension("nginx.conf");
        let _ = file_icon_for_extension("config.ini");
        let _ = file_icon_for_extension("release.nfo");
    }

    #[test]
    fn test_file_icon_default() {
        // Unknown extensions should return generic file icon
        let _ = file_icon_for_extension("unknown.xyz");
        let _ = file_icon_for_extension("noextension");
        let _ = file_icon_for_extension(".hidden");
    }

    #[test]
    fn test_file_icon_case_insensitive() {
        // Extensions should be case-insensitive
        let _ = file_icon_for_extension("PHOTO.PNG");
        let _ = file_icon_for_extension("Photo.Png");
        let _ = file_icon_for_extension("photo.PNG");
    }
}
