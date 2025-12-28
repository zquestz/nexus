//! Files panel view (browse, upload, download files)

use chrono::{DateTime, Local, TimeZone, Utc};

use super::layout::scrollable_panel;
use crate::i18n::t;
use crate::icon;
use crate::style::{
    BUTTON_PADDING, CLOSE_BUTTON_PADDING, CONTEXT_MENU_ITEM_PADDING, CONTEXT_MENU_MIN_WIDTH,
    CONTEXT_MENU_PADDING, CONTEXT_MENU_SEPARATOR_HEIGHT, CONTEXT_MENU_SEPARATOR_MARGIN,
    ELEMENT_SPACING, FILE_DATE_COLUMN_WIDTH, FILE_LIST_ICON_SIZE, FILE_LIST_ICON_SPACING,
    FILE_SIZE_COLUMN_WIDTH, FILE_TOOLBAR_BUTTON_PADDING, FILE_TOOLBAR_ICON_SIZE, FORM_MAX_WIDTH,
    FORM_PADDING, ICON_BUTTON_PADDING, INPUT_PADDING, NEWS_LIST_MAX_WIDTH, NO_SPACING,
    SCROLLBAR_PADDING, SEPARATOR_HEIGHT, SIDEBAR_ACTION_ICON_SIZE, SMALL_PADDING, SMALL_SPACING,
    SPACER_SIZE_MEDIUM, SPACER_SIZE_SMALL, TAB_CONTENT_PADDING, TEXT_SIZE, TITLE_SIZE,
    TOOLTIP_BACKGROUND_PADDING, TOOLTIP_GAP, TOOLTIP_PADDING, TOOLTIP_TEXT_SIZE,
    chat_tab_active_style, close_button_on_primary_style, content_background_style,
    context_menu_button_style, context_menu_container_style, context_menu_item_danger_style,
    disabled_icon_button_style, error_text_style, muted_text_style, panel_title, separator_style,
    shaped_text, shaped_text_wrapped, tooltip_container_style, transparent_icon_button_style,
    upload_folder_style,
};
use crate::types::{
    ClipboardOperation, FileSortColumn, FileTab, FilesManagementState, InputId, Message,
    ScrollableId, TabId,
};
use iced::widget::button as btn;
use iced::widget::text::Wrapping;
use iced::widget::{Space, button, column, container, row, scrollable, table, text_input, tooltip};
use iced::{Center, Element, Fill, Right, alignment};
use iced_aw::ContextMenu;
use nexus_common::protocol::{FileEntry, FileInfoDetails};

/// File permission flags for view rendering
#[derive(Debug, Clone, Copy)]
pub struct FilePermissions {
    pub file_root: bool,
    pub file_create_dir: bool,
    pub file_info: bool,
    pub file_delete: bool,
    pub file_rename: bool,
    pub file_move: bool,
    pub file_copy: bool,
}

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
/// Build a navigation path by appending a segment to the current path
///
/// This is public so the file info handler can use it to build full paths.
pub fn build_navigate_path(current_path: &str, segment: &str) -> String {
    if current_path.is_empty() || current_path == "/" {
        segment.to_string()
    } else {
        format!("{current_path}/{segment}")
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
    has_clipboard: bool,
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

    // Paste button - enabled if clipboard has content and not loading
    let paste_button: Element<'a, Message> = if has_clipboard && !is_loading {
        tooltip(
            button(icon::paste().size(FILE_TOOLBAR_ICON_SIZE))
                .padding(FILE_TOOLBAR_BUTTON_PADDING)
                .style(transparent_icon_button_style)
                .on_press(Message::FilePaste),
            container(shaped_text(t("tooltip-files-paste")).size(TOOLTIP_TEXT_SIZE))
                .padding(TOOLTIP_BACKGROUND_PADDING)
                .style(tooltip_container_style),
            tooltip::Position::Bottom,
        )
        .gap(TOOLTIP_GAP)
        .padding(TOOLTIP_PADDING)
        .into()
    } else {
        // Disabled paste button
        button(icon::paste().size(FILE_TOOLBAR_ICON_SIZE))
            .padding(FILE_TOOLBAR_BUTTON_PADDING)
            .style(disabled_icon_button_style)
            .into()
    };

    toolbar_row = toolbar_row.push(paste_button);

    // Up button - last
    toolbar_row = toolbar_row.push(up_button);

    toolbar_row.into()
}

/// Build the delete confirmation dialog
fn delete_confirm_dialog<'a>(path: &str, error: Option<&'a String>) -> Element<'a, Message> {
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
        .padding(FORM_PADDING)
        .max_width(FORM_MAX_WIDTH);

    scrollable_panel(form)
}

/// Build the overwrite confirmation dialog
fn overwrite_confirm_dialog<'a>(name: &str, has_file_delete: bool) -> Element<'a, Message> {
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
        buttons_row = buttons_row.push(
            button(shaped_text(t("button-overwrite")).size(TEXT_SIZE))
                .on_press(Message::FileOverwriteConfirm)
                .padding(BUTTON_PADDING)
                .style(btn::danger),
        );
    }

    let form = column![
        title,
        Space::new().height(SPACER_SIZE_MEDIUM),
        shaped_text_wrapped(&message)
            .size(TEXT_SIZE)
            .width(Fill)
            .align_x(Center),
        Space::new().height(SPACER_SIZE_MEDIUM),
        buttons_row,
    ]
    .spacing(ELEMENT_SPACING)
    .padding(FORM_PADDING)
    .max_width(FORM_MAX_WIDTH);

    scrollable_panel(form)
}

/// Icon size for file info dialog
const FILE_INFO_ICON_SIZE: f32 = 64.0;

/// Spacing between icon and name in file info header
const FILE_INFO_ICON_SPACING: f32 = 12.0;

/// Size for sort indicator icons in column headers
const SORT_ICON_SIZE: f32 = 12.0;

/// Right margin for sort icons (prevents scrollbar overlap on rightmost column)
const SORT_ICON_RIGHT_MARGIN: f32 = 12.0;

/// Build the file info dialog
fn file_info_dialog(info: &FileInfoDetails) -> Element<'_, Message> {
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
            let s = format_timestamp(ts);
            if s.is_empty() { t("files-info-na") } else { s }
        }
        None => t("files-info-na"),
    };
    content = content.push(info_row(t("files-info-created"), created_value));

    // Modified (always available)
    let modified_value = format_timestamp(info.modified);
    let modified_value = if modified_value.is_empty() {
        t("files-info-na")
    } else {
        modified_value
    };
    content = content.push(info_row(t("files-info-modified"), modified_value));

    // SHA-256 hash (files only) - use WordOrGlyph wrapping for long hash without spaces
    if let Some(hash) = &info.sha256 {
        let sha_row = row![
            shaped_text(t("files-info-sha256")).size(TEXT_SIZE),
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

    let form = content.padding(FORM_PADDING).max_width(FORM_MAX_WIDTH);

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
/// Rename dialog for files and directories
fn rename_dialog<'a>(path: &str, name: &str, error: Option<&String>) -> Element<'a, Message> {
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
        if name_valid {
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
        .padding(FORM_PADDING)
        .max_width(FORM_MAX_WIDTH);

    scrollable_panel(form)
}

/// Build the file table
///
/// Note: entries should already be sorted before calling this function.
fn file_table<'a>(
    entries: &'a [FileEntry],
    current_path: &'a str,
    perms: FilePermissions,
    clipboard: &'a Option<crate::types::ClipboardItem>,
    sort_column: FileSortColumn,
    sort_ascending: bool,
) -> Element<'a, Message> {
    // Name column header (clickable)
    let name_header_content: Element<'_, Message> = if sort_column == FileSortColumn::Name {
        let sort_icon = if sort_ascending {
            icon::down_dir()
        } else {
            icon::up_dir()
        };
        row![
            shaped_text(t("files-column-name"))
                .size(TEXT_SIZE)
                .style(muted_text_style),
            Space::new().width(Fill),
            sort_icon.size(SORT_ICON_SIZE).style(muted_text_style),
            Space::new().width(SORT_ICON_RIGHT_MARGIN),
        ]
        .align_y(Center)
        .into()
    } else {
        shaped_text(t("files-column-name"))
            .size(TEXT_SIZE)
            .style(muted_text_style)
            .into()
    };
    let name_header: Element<'_, Message> = button(name_header_content)
        .padding(NO_SPACING)
        .width(Fill)
        .style(transparent_icon_button_style)
        .on_press(Message::FileSortBy(FileSortColumn::Name))
        .into();

    // Name column with icon
    let name_column = table::column(name_header, |entry: &FileEntry| {
        let is_directory = entry.dir_type.is_some();
        let display_name = FilesManagementState::display_name(&entry.name);

        // Check if this entry is cut (pending move) - show muted
        let entry_path = build_navigate_path(current_path, &entry.name);
        let is_cut = clipboard
            .as_ref()
            .is_some_and(|c| c.operation == ClipboardOperation::Cut && c.path == entry_path);

        // Icon based on type (folder or file extension)
        // Uploadable folders get primary color
        let icon_element: Element<'_, Message> = if is_directory {
            if entry.can_upload {
                icon::folder()
                    .size(FILE_LIST_ICON_SIZE)
                    .style(upload_folder_style)
                    .into()
            } else {
                icon::folder().size(FILE_LIST_ICON_SIZE).into()
            }
        } else {
            file_icon_for_extension(&entry.name)
                .size(FILE_LIST_ICON_SIZE)
                .into()
        };

        // Name with icon - use muted style if cut
        let name_text = if is_cut {
            shaped_text(display_name)
                .size(TEXT_SIZE)
                .wrapping(Wrapping::WordOrGlyph)
                .style(muted_text_style)
        } else {
            shaped_text(display_name)
                .size(TEXT_SIZE)
                .wrapping(Wrapping::WordOrGlyph)
        };

        let name_content: Element<'_, Message> = row![
            icon_element,
            Space::new().width(FILE_LIST_ICON_SPACING),
            name_text,
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

        // Wrap in context menu if user has any file action permission
        let has_any_permission = perms.file_info
            || perms.file_delete
            || perms.file_rename
            || perms.file_move
            || perms.file_copy;
        let has_clipboard = clipboard.is_some();

        if has_any_permission {
            // Build the full path for this entry
            let entry_path = build_navigate_path(current_path, &entry.name);
            let entry_name = entry.name.clone();
            let entry_is_dir = is_directory;

            ContextMenu::new(row_element, move || {
                let mut menu_items: Vec<Element<'_, Message>> = vec![];
                let mut has_clipboard_section = false;
                let mut has_normal_section = false;

                // === Section 1: Clipboard actions ===

                // Cut (if permission)
                if perms.file_move {
                    menu_items.push(
                        button(shaped_text(t("files-cut")).size(TEXT_SIZE))
                            .padding(CONTEXT_MENU_ITEM_PADDING)
                            .width(Fill)
                            .style(context_menu_button_style)
                            .on_press(Message::FileCut(entry_path.clone(), entry_name.clone()))
                            .into(),
                    );
                    has_clipboard_section = true;
                }

                // Copy (if permission)
                if perms.file_copy {
                    menu_items.push(
                        button(shaped_text(t("files-copy")).size(TEXT_SIZE))
                            .padding(CONTEXT_MENU_ITEM_PADDING)
                            .width(Fill)
                            .style(context_menu_button_style)
                            .on_press(Message::FileCopyToClipboard(
                                entry_path.clone(),
                                entry_name.clone(),
                            ))
                            .into(),
                    );
                    has_clipboard_section = true;
                }

                // Paste (only on directories, when clipboard has content)
                if entry_is_dir && has_clipboard && (perms.file_move || perms.file_copy) {
                    menu_items.push(
                        button(shaped_text(t("files-paste")).size(TEXT_SIZE))
                            .padding(CONTEXT_MENU_ITEM_PADDING)
                            .width(Fill)
                            .style(context_menu_button_style)
                            .on_press(Message::FilePasteInto(entry_path.clone()))
                            .into(),
                    );
                    has_clipboard_section = true;
                }

                // === Section 2: Normal actions ===

                // Separator before normal actions (if we had clipboard actions)
                if has_clipboard_section && (perms.file_info || perms.file_rename) {
                    menu_items.push(
                        container(Space::new())
                            .width(Fill)
                            .height(CONTEXT_MENU_SEPARATOR_HEIGHT)
                            .style(separator_style)
                            .into(),
                    );
                }

                // Info (if permission)
                if perms.file_info {
                    menu_items.push(
                        button(shaped_text(t("files-info")).size(TEXT_SIZE))
                            .padding(CONTEXT_MENU_ITEM_PADDING)
                            .width(Fill)
                            .style(context_menu_button_style)
                            .on_press(Message::FileInfoClicked(entry_name.clone()))
                            .into(),
                    );
                    has_normal_section = true;
                }

                // Rename (if permission)
                if perms.file_rename {
                    menu_items.push(
                        button(shaped_text(t("files-rename")).size(TEXT_SIZE))
                            .padding(CONTEXT_MENU_ITEM_PADDING)
                            .width(Fill)
                            .style(context_menu_button_style)
                            .on_press(Message::FileRenameClicked(entry_name.clone()))
                            .into(),
                    );
                    has_normal_section = true;
                }

                // === Section 3: Destructive actions ===

                // Delete (if permission) - with separator before destructive action
                if perms.file_delete {
                    // Only add separator if there are items above it
                    if has_clipboard_section || has_normal_section {
                        menu_items.push(
                            container(Space::new())
                                .width(Fill)
                                .height(CONTEXT_MENU_SEPARATOR_HEIGHT)
                                .style(separator_style)
                                .into(),
                        );
                    }
                    menu_items.push(
                        button(shaped_text(t("files-delete")).size(TEXT_SIZE))
                            .padding(CONTEXT_MENU_ITEM_PADDING)
                            .width(Fill)
                            .style(context_menu_item_danger_style)
                            .on_press(Message::FileDeleteClicked(entry_path.clone()))
                            .into(),
                    );
                }

                container(
                    iced::widget::Column::with_children(menu_items)
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
    })
    .width(Fill);

    // Size column header (clickable)
    let size_header_content: Element<'_, Message> = if sort_column == FileSortColumn::Size {
        let sort_icon = if sort_ascending {
            icon::down_dir()
        } else {
            icon::up_dir()
        };
        row![
            shaped_text(t("files-column-size"))
                .size(TEXT_SIZE)
                .style(muted_text_style),
            Space::new().width(Fill),
            sort_icon.size(SORT_ICON_SIZE).style(muted_text_style),
            Space::new().width(SORT_ICON_RIGHT_MARGIN),
        ]
        .align_y(Center)
        .into()
    } else {
        shaped_text(t("files-column-size"))
            .size(TEXT_SIZE)
            .style(muted_text_style)
            .into()
    };
    let size_header: Element<'_, Message> = button(size_header_content)
        .padding(NO_SPACING)
        .width(Fill)
        .style(transparent_icon_button_style)
        .on_press(Message::FileSortBy(FileSortColumn::Size))
        .into();

    // Size column
    let size_column = table::column(size_header, |entry: &FileEntry| {
        let size_text = if entry.dir_type.is_some() {
            String::new()
        } else {
            format_size(entry.size)
        };
        shaped_text(size_text)
            .size(TEXT_SIZE)
            .style(muted_text_style)
    })
    .width(FILE_SIZE_COLUMN_WIDTH)
    .align_x(Right);

    // Modified column header (clickable)
    let modified_header_content: Element<'_, Message> = if sort_column == FileSortColumn::Modified {
        let sort_icon = if sort_ascending {
            icon::down_dir()
        } else {
            icon::up_dir()
        };
        row![
            shaped_text(t("files-column-modified"))
                .size(TEXT_SIZE)
                .style(muted_text_style),
            Space::new().width(Fill),
            sort_icon.size(SORT_ICON_SIZE).style(muted_text_style),
            Space::new().width(SORT_ICON_RIGHT_MARGIN),
        ]
        .align_y(Center)
        .into()
    } else {
        shaped_text(t("files-column-modified"))
            .size(TEXT_SIZE)
            .style(muted_text_style)
            .into()
    };
    let modified_header: Element<'_, Message> = button(modified_header_content)
        .padding(NO_SPACING)
        .width(Fill)
        .style(transparent_icon_button_style)
        .on_press(Message::FileSortBy(FileSortColumn::Modified))
        .into();

    // Modified column
    let modified_column = table::column(modified_header, |entry: &FileEntry| {
        let date_text = format_timestamp(entry.modified);
        shaped_text(date_text)
            .size(TEXT_SIZE)
            .style(muted_text_style)
    })
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
// Tab Bar Functions
// ============================================================================

/// Create a tab button for the file browser
fn create_file_tab_button(
    tab_id: TabId,
    tab: &FileTab,
    is_active: bool,
    can_close: bool,
) -> Element<'static, Message> {
    let label = tab.tab_name();

    if is_active {
        create_active_file_tab_button(tab_id, label, can_close)
    } else {
        create_inactive_file_tab_button(tab_id, label)
    }
}

/// Create an active file tab button (with close button if closeable)
fn create_active_file_tab_button(
    tab_id: TabId,
    label: String,
    can_close: bool,
) -> Element<'static, Message> {
    if can_close {
        let close_button = tooltip(
            button(icon::close().size(TEXT_SIZE))
                .on_press(Message::FileTabClose(tab_id))
                .padding(CLOSE_BUTTON_PADDING)
                .style(close_button_on_primary_style()),
            container(shaped_text(t("tooltip-close-tab")).size(TOOLTIP_TEXT_SIZE))
                .padding(TOOLTIP_BACKGROUND_PADDING)
                .style(tooltip_container_style),
            tooltip::Position::Bottom,
        )
        .gap(TOOLTIP_GAP)
        .padding(TOOLTIP_PADDING);

        let tab_content = row![shaped_text(label).size(TEXT_SIZE), close_button]
            .spacing(SMALL_SPACING)
            .align_y(iced::Alignment::Center);

        button(tab_content)
            .on_press(Message::FileTabSwitch(tab_id))
            .padding(TAB_CONTENT_PADDING)
            .style(chat_tab_active_style())
            .into()
    } else {
        // Single tab (no close button)
        button(shaped_text(label).size(TEXT_SIZE))
            .on_press(Message::FileTabSwitch(tab_id))
            .padding(INPUT_PADDING)
            .style(chat_tab_active_style())
            .into()
    }
}

/// Create an inactive file tab button
fn create_inactive_file_tab_button(tab_id: TabId, label: String) -> Element<'static, Message> {
    button(shaped_text(label).size(TEXT_SIZE))
        .on_press(Message::FileTabSwitch(tab_id))
        .style(iced::widget::button::secondary)
        .padding(INPUT_PADDING)
        .into()
}

/// Build the file tab bar
///
/// Returns a tuple of (tab row, has_multiple_tabs)
fn build_file_tab_bar(
    files_management: &FilesManagementState,
) -> (iced::widget::Row<'static, Message>, bool) {
    let mut tab_row = row![].spacing(SMALL_SPACING);

    let num_tabs = files_management.tabs.len();
    let has_multiple_tabs = num_tabs > 1;

    for (index, tab) in files_management.tabs.iter().enumerate() {
        let is_active = index == files_management.active_tab;
        let tab_button = create_file_tab_button(tab.id, tab, is_active, has_multiple_tabs);
        tab_row = tab_row.push(tab_button);
    }

    (tab_row, has_multiple_tabs)
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
/// * `perms` - File permission flags for the current user
/// * `show_hidden` - Whether to show hidden files (from config)
pub fn files_view<'a>(
    files_management: &'a FilesManagementState,
    perms: FilePermissions,
    show_hidden: bool,
) -> Element<'a, Message> {
    let tab = files_management.active_tab();

    // If overwrite confirmation is pending, show that dialog
    if let Some(pending) = &tab.pending_overwrite {
        return overwrite_confirm_dialog(&pending.name, perms.file_delete);
    }

    // If rename dialog is pending, show that
    if let Some(path) = &tab.pending_rename {
        return rename_dialog(path, &tab.rename_name, tab.rename_error.as_ref());
    }

    // If file info is pending, show that dialog
    if let Some(info) = &tab.pending_info {
        return file_info_dialog(info);
    }

    // If delete confirmation is pending, show that dialog
    if let Some(path) = &tab.pending_delete {
        return delete_confirm_dialog(path, tab.delete_error.as_ref());
    }

    // If creating directory, show the dialog instead
    if tab.creating_directory {
        return new_directory_dialog(&tab.new_directory_name, tab.new_directory_error.as_ref());
    }
    let is_at_home = tab.current_path.is_empty() || tab.current_path == "/";
    let viewing_root = tab.viewing_root;

    // New tab button (icon style like news create button)
    let new_tab_btn: Element<'_, Message> = {
        let add_icon = container(icon::plus().size(SIDEBAR_ACTION_ICON_SIZE))
            .width(SIDEBAR_ACTION_ICON_SIZE)
            .height(SIDEBAR_ACTION_ICON_SIZE)
            .align_x(alignment::Horizontal::Center)
            .align_y(alignment::Vertical::Center);

        tooltip(
            button(add_icon)
                .on_press(Message::FileTabNew)
                .padding(ICON_BUTTON_PADDING)
                .style(transparent_icon_button_style),
            container(shaped_text(t("tooltip-new-tab")).size(TOOLTIP_TEXT_SIZE))
                .padding(TOOLTIP_BACKGROUND_PADDING)
                .style(tooltip_container_style),
            tooltip::Position::Top,
        )
        .gap(TOOLTIP_GAP)
        .padding(TOOLTIP_PADDING)
        .into()
    };

    // Title row with new tab button on the right
    // We add an invisible spacer on the left to balance the button width for proper centering
    let button_width =
        SIDEBAR_ACTION_ICON_SIZE + ICON_BUTTON_PADDING.left + ICON_BUTTON_PADDING.right;
    let title_row: Element<'_, Message> = row![
        Space::new().width(SCROLLBAR_PADDING),
        Space::new().width(button_width), // Balance the new tab button on the right
        shaped_text(t("files-panel-title"))
            .size(TITLE_SIZE)
            .width(Fill)
            .align_x(Center),
        new_tab_btn,
        Space::new().width(SCROLLBAR_PADDING),
    ]
    .align_y(Center)
    .into();

    // Build tab bar (only shown when 2+ tabs)
    let (tab_row, has_multiple_tabs) = build_file_tab_bar(files_management);
    let tab_bar = tab_row.wrap();

    // Breadcrumb navigation
    let breadcrumbs = breadcrumb_bar(&tab.current_path, viewing_root);

    // Determine if user can create directories here
    // User can create if they have file_create_dir permission OR current directory allows upload
    let can_create_dir = perms.file_create_dir || tab.current_dir_can_upload;

    // Check if we're in a loading state
    let is_loading = tab.entries.is_none() && tab.error.is_none();

    // Check if clipboard has content
    let has_clipboard = files_management.clipboard.is_some();

    // Toolbar with buttons
    let toolbar = toolbar(
        !is_at_home,
        perms.file_root,
        viewing_root,
        show_hidden,
        can_create_dir,
        has_clipboard,
        is_loading,
    );

    // Content area (table or status message)
    // Priority: error > entries > loading
    let content: Element<'a, Message> = if let Some(error) = &tab.error {
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
    } else if let Some(entries) = &tab.sorted_entries {
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
            file_table(
                entries,
                &tab.current_path,
                perms,
                &files_management.clipboard,
                tab.sort_column,
                tab.sort_ascending,
            )
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

    // Build full content with optional tab bar at top
    let full_content: Element<'a, Message> = if has_multiple_tabs {
        column![
            container(tab_bar).padding(SMALL_PADDING).width(Fill),
            centered_form,
        ]
        .width(Fill)
        .height(Fill)
        .into()
    } else {
        centered_form.into()
    };

    // Wrap everything in content background
    container(full_content)
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
