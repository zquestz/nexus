//! Files panel view (browse, upload, download files)
//!
//! Sub-modules handle specific view components:
//! - `helpers` — Icons, formatting, breadcrumb parsing
//! - `toolbar` — Toolbar buttons, breadcrumb bar, search input
//! - `search` — Search results table and context menu
//! - `dialogs` — Delete, overwrite, info, new directory, rename dialogs
//! - `listing` — File listing table and context menu
//! - `tabs` — Tab bar for multi-tab file browsing

mod dialogs;
mod helpers;
mod listing;
mod search;
mod tabs;
mod toolbar;

use std::hash::Hash;

pub use helpers::build_navigate_path;

use dialogs::{
    delete_confirm_dialog, file_info_dialog, new_directory_dialog, overwrite_confirm_dialog,
    rename_dialog,
};
use helpers::build_navigate_path as build_path;
use listing::lazy_file_table;
use search::lazy_search_results_table;
use tabs::build_file_tab_bar;
use toolbar::{breadcrumb_bar, search_breadcrumb, search_input_row, toolbar};

use iced::widget::{Space, button, column, container, row, scrollable, stack, tooltip};
use iced::{Center, Element, Fill, alignment};
use nexus_common::protocol::{FileEntry, FileSearchResult};

use crate::i18n::t;
use crate::icon;
use crate::style::{
    CONTENT_MAX_WIDTH, CONTENT_PADDING, DROP_OVERLAY_ICON_SIZE, ICON_BUTTON_PADDING, NO_SPACING,
    SCROLLBAR_PADDING, SIDEBAR_ACTION_ICON_SIZE, SMALL_PADDING, SPACER_SIZE_MEDIUM,
    SPACER_SIZE_SMALL, TEXT_SIZE, TITLE_SIZE, TOOLTIP_BACKGROUND_PADDING, TOOLTIP_GAP,
    TOOLTIP_PADDING, TOOLTIP_TEXT_SIZE, content_background_style, drop_overlay_style,
    error_text_style, muted_text_style, shaped_text, shaped_text_wrapped, tooltip_container_style,
    transparent_icon_button_style,
};
use crate::types::{
    ClipboardOperation, FileSortColumn, FilesManagementState, Message, ScrollableId,
};

/// File permission flags for view rendering
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FilePermissions {
    pub file_root: bool,
    pub file_create_dir: bool,
    pub file_info: bool,
    pub file_delete: bool,
    pub file_rename: bool,
    pub file_move: bool,
    pub file_copy: bool,
    pub file_download: bool,
    /// True if the user has either `file_upload` or `file_upload_anywhere`.
    pub file_upload: bool,
    /// True if the user has `file_upload_anywhere` specifically (the bypass).
    /// Compose with `can_upload` to decide whether upload UI should be shown
    /// for a given folder: a folder that isn't upload-typed still accepts
    /// uploads when this is true.
    pub file_upload_anywhere: bool,
    pub file_search: bool,
}

impl FilePermissions {
    /// Whether the user should see upload UI for a folder with the given
    /// `can_upload` (folder-property) flag.
    ///
    /// `can_upload` means "folder is typed as upload/dropbox". The user's
    /// `file_upload_anywhere` bypass makes any folder effectively uploadable.
    pub fn effective_can_upload(&self, can_upload: bool) -> bool {
        self.file_upload && (can_upload || self.file_upload_anywhere)
    }
}

/// State needed to render the files toolbar
#[derive(Debug, Clone)]
pub struct ToolbarState<'a> {
    pub can_go_up: bool,
    pub has_file_root: bool,
    pub viewing_root: bool,
    pub show_hidden: bool,
    pub can_create_dir: bool,
    pub has_clipboard: bool,
    pub has_file_download: bool,
    pub has_file_upload: bool,
    pub can_upload: bool,
    pub current_path: &'a str,
    pub is_loading: bool,
    pub is_searching: bool,
}

/// Self-contained row data for lazy file table rendering
///
/// Each row carries all the data it needs to render, including pre-computed
/// paths and styling flags. This allows the table to work with `lazy()` since
/// the row data is owned and doesn't borrow from external state.
#[derive(Clone, PartialEq, Eq)]
struct FileRowData {
    /// File entry from server
    entry: FileEntry,
    /// Pre-computed full path for this entry
    path: String,
    /// Whether this entry is cut (pending move)
    is_cut: bool,
    /// User permissions (Copy, so cheap to include per-row)
    perms: FilePermissions,
    /// Whether clipboard has content (for paste option)
    has_clipboard: bool,
    /// Whether the requesting user is the owner of the enclosing
    /// `[NEXUS-DB-username]` drop box (uniform across all rows in a single
    /// listing). Composes with `file_delete` / `file_rename` to enable the
    /// Delete and Rename menu items.
    bypass_via_ownership: bool,
}

impl Hash for FileRowData {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.entry.name.hash(state);
        self.entry.size.hash(state);
        self.entry.modified.hash(state);
        self.entry.dir_type.hash(state);
        self.entry.can_upload.hash(state);
        self.path.hash(state);
        self.is_cut.hash(state);
        self.perms.hash(state);
        self.has_clipboard.hash(state);
        self.bypass_via_ownership.hash(state);
    }
}

/// Dependencies for lazy file table caching
///
/// When these values change, the table will be rebuilt. Otherwise,
/// the cached widget tree is reused, avoiding expensive re-renders.
#[derive(Clone, PartialEq, Eq, Hash)]
struct FileTableDeps {
    /// Pre-built row data with all rendering context
    rows: Vec<FileRowData>,
    /// Sort column (for header display)
    sort_column: FileSortColumn,
    /// Sort direction (for header display)
    sort_ascending: bool,
}

/// Dependencies for lazy search results table caching
#[derive(Clone, PartialEq, Eq, Hash)]
struct SearchResultsDeps {
    /// Search results
    results: Vec<FileSearchResult>,
    /// User permissions
    perms: FilePermissions,
    /// Sort column
    sort_column: FileSortColumn,
    /// Sort direction
    sort_ascending: bool,
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
    show_drop_overlay: bool,
    username: &str,
) -> Element<'a, Message> {
    let tab = files_management.active_tab();
    // Compose the active listing's drop-box owner with the user's account
    // username (matching the server-side `in_owned_dropbox` semantics — the
    // dropbox suffix names an account, not a nickname). Lowercase comparison
    // matches the server and respects IDN/Unicode handling.
    let username_lower = username.to_lowercase();
    let bypass_via_ownership = tab
        .dropbox_owner
        .as_ref()
        .is_some_and(|owner| owner.to_lowercase() == username_lower);

    // If overwrite confirmation is pending, show that dialog
    if let Some(pending) = &tab.pending_overwrite {
        return overwrite_confirm_dialog(&pending.name, perms.file_delete, tab.is_paste_submitting);
    }

    // If rename dialog is pending, show that
    if let Some(path) = &tab.pending_rename {
        return rename_dialog(
            path,
            &tab.rename_name,
            tab.rename_error.as_ref(),
            tab.is_rename_submitting,
        );
    }

    // If file info is pending, show that dialog
    if let Some(info) = &tab.pending_info {
        return file_info_dialog(info);
    }

    // If delete confirmation is pending, show that dialog
    if let Some(path) = &tab.pending_delete {
        return delete_confirm_dialog(path, tab.delete_error.as_ref(), tab.is_delete_submitting);
    }

    // If creating directory, show the dialog instead
    if tab.creating_directory {
        return new_directory_dialog(
            &tab.new_directory_name,
            tab.new_directory_error.as_ref(),
            tab.is_create_dir_submitting,
        );
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

    // Check if in search mode
    let is_searching = tab.is_searching();

    // Breadcrumb navigation (or search breadcrumb when searching)
    let breadcrumbs: Element<'_, Message> = if let Some(query) = &tab.search_query {
        search_breadcrumb(query)
    } else {
        breadcrumb_bar(&tab.current_path, viewing_root)
    };

    // Determine if user can create directories here
    // User can create if they have file_create_dir permission OR current directory allows upload
    let can_create_dir = perms.file_create_dir || tab.current_dir_can_upload;

    // Check if we're in a loading state
    let is_loading = tab.entries.is_none() && tab.error.is_none();

    // Check if clipboard has content
    let has_clipboard = files_management.clipboard.is_some();

    // Toolbar with buttons
    let toolbar_state = ToolbarState {
        can_go_up: !is_at_home,
        has_file_root: perms.file_root,
        viewing_root,
        show_hidden,
        can_create_dir,
        has_clipboard,
        has_file_download: perms.file_download,
        has_file_upload: perms.file_upload,
        // Compose the server's folder-type flag with the user's bypass bit
        // so toolbar buttons reflect whether the user can actually upload here.
        can_upload: perms.effective_can_upload(tab.current_dir_can_upload),
        current_path: &tab.current_path,
        is_loading,
        is_searching,
    };
    let toolbar = toolbar(&toolbar_state);

    // Search input row (only shown if user has file_search permission)
    let search_row: Option<Element<'_, Message>> = if perms.file_search {
        Some(search_input_row(&tab.search_input, tab.search_loading))
    } else {
        None
    };

    // Content area - different handling for search mode vs normal browsing
    let content: Element<'a, Message> = if is_searching {
        // Search mode content
        if tab.search_loading {
            // Searching state
            container(
                shaped_text(t("files-searching"))
                    .size(TEXT_SIZE)
                    .style(muted_text_style),
            )
            .width(Fill)
            .center_x(Fill)
            .padding(SPACER_SIZE_SMALL)
            .into()
        } else if let Some(error) = &tab.search_error {
            // Search error state
            container(
                shaped_text_wrapped(error)
                    .size(TEXT_SIZE)
                    .style(error_text_style),
            )
            .width(Fill)
            .center_x(Fill)
            .padding(SPACER_SIZE_SMALL)
            .into()
        } else if let Some(results) = &tab.search_results {
            if results.is_empty() {
                // No results
                container(
                    shaped_text(t("files-no-results"))
                        .size(TEXT_SIZE)
                        .style(muted_text_style),
                )
                .width(Fill)
                .center_x(Fill)
                .padding(SPACER_SIZE_SMALL)
                .into()
            } else {
                // Search results table
                lazy_search_results_table(SearchResultsDeps {
                    results: results.clone(),
                    perms,
                    sort_column: tab.search_sort_column,
                    sort_ascending: tab.search_sort_ascending,
                })
            }
        } else {
            // Should not happen, but handle gracefully
            container(
                shaped_text(t("files-searching"))
                    .size(TEXT_SIZE)
                    .style(muted_text_style),
            )
            .width(Fill)
            .center_x(Fill)
            .padding(SPACER_SIZE_SMALL)
            .into()
        }
    } else {
        // Normal browsing mode
        // Build main content (listing, empty, or loading)
        if let Some(entries) = &tab.sorted_entries {
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
                // Build row data with all context pre-computed
                let rows: Vec<FileRowData> = entries
                    .iter()
                    .map(|entry| {
                        let path = build_path(&tab.current_path, &entry.name);
                        let is_cut = files_management.clipboard.as_ref().is_some_and(|c| {
                            c.operation == ClipboardOperation::Cut && c.path == path
                        });
                        FileRowData {
                            entry: entry.clone(),
                            path,
                            is_cut,
                            perms,
                            has_clipboard: files_management.clipboard.is_some(),
                            bypass_via_ownership,
                        }
                    })
                    .collect();

                let deps = FileTableDeps {
                    rows,
                    sort_column: tab.sort_column,
                    sort_ascending: tab.sort_ascending,
                };

                lazy_file_table(deps)
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
        }
    };

    // Build the form with max_width constraint
    // Breadcrumbs and toolbar stay fixed, only content scrolls
    let mut form_column = column![title_row, breadcrumbs, toolbar,];

    // Add error banner if present (between toolbar and search)
    if let Some(error) = &tab.error {
        form_column = form_column.push(
            container(
                shaped_text_wrapped(error)
                    .size(TEXT_SIZE)
                    .style(error_text_style),
            )
            .width(Fill)
            .center_x(Fill)
            .padding(iced::Padding {
                top: NO_SPACING,
                right: NO_SPACING,
                bottom: SPACER_SIZE_MEDIUM,
                left: NO_SPACING,
            }),
        );
    }

    // Add search row if user has permission
    if let Some(search) = search_row {
        form_column = form_column.push(search);
    }

    // Add scrollable content
    form_column = form_column
        .push(container(scrollable(content).id(ScrollableId::FilesContent)).height(Fill));

    let form = form_column
        .spacing(SPACER_SIZE_SMALL)
        .align_x(Center)
        .padding(CONTENT_PADDING)
        .max_width(CONTENT_MAX_WIDTH)
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
    let main_content = container(full_content)
        .width(Fill)
        .height(Fill)
        .style(content_background_style);

    // Add drop overlay if dragging files over uploadable folder
    if show_drop_overlay {
        let overlay = container(
            column![
                icon::upload().size(DROP_OVERLAY_ICON_SIZE),
                shaped_text(t("drop-to-upload")).size(TITLE_SIZE),
            ]
            .spacing(SPACER_SIZE_SMALL)
            .align_x(Center),
        )
        .width(Fill)
        .height(Fill)
        .center_x(Fill)
        .center_y(Fill)
        .style(drop_overlay_style);

        stack![main_content, overlay].into()
    } else {
        main_content.into()
    }
}
