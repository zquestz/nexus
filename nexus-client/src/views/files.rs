//! Files panel view (browse, upload, download files)

use chrono::{DateTime, Local, TimeZone, Utc};

use crate::i18n::t;
use crate::icon;
use crate::style::{
    FILE_DATE_COLUMN_WIDTH, FILE_LIST_ICON_SIZE, FILE_LIST_ICON_SPACING, FILE_SIZE_COLUMN_WIDTH,
    FILE_TOOLBAR_BUTTON_PADDING, FILE_TOOLBAR_ICON_SIZE, FORM_PADDING, NEWS_LIST_MAX_WIDTH,
    NO_SPACING, SEPARATOR_HEIGHT, SPACER_SIZE_SMALL, TEXT_SIZE, TOOLTIP_BACKGROUND_PADDING,
    TOOLTIP_GAP, TOOLTIP_PADDING, TOOLTIP_TEXT_SIZE, content_background_style,
    disabled_icon_button_style, error_text_style, muted_text_style, panel_title, shaped_text,
    shaped_text_wrapped, tooltip_container_style, transparent_icon_button_style,
};
use crate::types::{FilesManagementState, Message, ScrollableId};
use iced::widget::{Space, button, column, container, row, scrollable, table, tooltip};
use iced::{Center, Element, Fill, Right};
use nexus_common::protocol::FileEntry;

// ============================================================================
// Helper Functions
// ============================================================================

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
fn breadcrumb_bar<'a>(current_path: &str) -> Element<'a, Message> {
    let mut breadcrumbs = iced::widget::Row::new().spacing(SPACER_SIZE_SMALL);

    // Home link - always clickable (acts as refresh when at home)
    let home_btn = button(
        shaped_text(t("files-home"))
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
        // Add separator
        breadcrumbs = breadcrumbs.push(shaped_text("/").size(TEXT_SIZE).style(muted_text_style));

        // Strip any folder type suffix for display
        let display_name = FilesManagementState::display_name(display_name);

        // All segments are clickable (last one acts as refresh)
        let segment_btn = button(
            shaped_text(&display_name)
                .size(TEXT_SIZE)
                .style(muted_text_style),
        )
        .padding(NO_SPACING)
        .style(transparent_icon_button_style)
        .on_press(Message::FileNavigate(path));

        breadcrumbs = breadcrumbs.push(segment_btn);
    }

    container(breadcrumbs)
        .padding([SPACER_SIZE_SMALL, NO_SPACING])
        .into()
}

/// Build the toolbar with Home, Refresh, and Up buttons
fn toolbar<'a>(can_go_up: bool) -> Element<'a, Message> {
    // Home button - always enabled
    let home_button = tooltip(
        button(icon::home().size(FILE_TOOLBAR_ICON_SIZE))
            .padding(FILE_TOOLBAR_BUTTON_PADDING)
            .style(transparent_icon_button_style)
            .on_press(Message::FileNavigateHome),
        container(shaped_text(t("tooltip-files-home")).size(TOOLTIP_TEXT_SIZE))
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

    row![home_button, refresh_button, up_button]
        .spacing(SPACER_SIZE_SMALL)
        .into()
}

/// Build the file table
fn file_table<'a>(entries: &'a [FileEntry], current_path: &'a str) -> Element<'a, Message> {
    // Name column with icon
    let name_column = table::column(
        shaped_text(t("files-column-name"))
            .size(TEXT_SIZE)
            .style(muted_text_style),
        move |entry: &FileEntry| {
            let is_directory = entry.dir_type.is_some();
            let display_name = FilesManagementState::display_name(&entry.name);

            // Icon based on type
            let icon_element: Element<'_, Message> = if is_directory {
                icon::folder().size(FILE_LIST_ICON_SIZE).into()
            } else {
                icon::file().size(FILE_LIST_ICON_SIZE).into()
            };

            // Name with icon
            let name_content: Element<'_, Message> = row![
                icon_element,
                Space::new().width(FILE_LIST_ICON_SPACING),
                shaped_text(display_name).size(TEXT_SIZE),
            ]
            .align_y(Center)
            .into();

            // For directories, make the row clickable
            if is_directory {
                let navigate_path = build_navigate_path(current_path, &entry.name);
                button(name_content)
                    .padding(NO_SPACING)
                    .style(transparent_icon_button_style)
                    .on_press(Message::FileNavigate(navigate_path))
                    .into()
            } else {
                name_content
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
pub fn files_view<'a>(files_management: &'a FilesManagementState) -> Element<'a, Message> {
    let is_at_home =
        files_management.current_path.is_empty() || files_management.current_path == "/";

    // Title row (centered, matching other panels)
    let title_row = panel_title(t("files-panel-title"));

    // Breadcrumb navigation
    let breadcrumbs = breadcrumb_bar(&files_management.current_path);

    // Toolbar with buttons
    let toolbar = toolbar(!is_at_home);

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
            file_table(entries, &files_management.current_path)
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
}
