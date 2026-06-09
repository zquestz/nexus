//! Toolbar, breadcrumb bar, and search input for the files view

use iced::widget::text::Wrapping;
use iced::widget::{Space, button, container, row, text_input, tooltip};
use iced::{Center, Element, Fill};

use super::ToolbarState;
use super::helpers::{parse_breadcrumbs, truncate_segment};
use crate::i18n::{t, t_args};
use crate::icon;
use crate::style::{
    BREADCRUMB_MAX_SEGMENT_LENGTH, ICON_SIZE, INPUT_PADDING, NO_SPACING, SMALL_SPACING,
    SPACER_SIZE_SMALL, TEXT_SIZE, TOOLBAR_BUTTON_PADDING, TOOLTIP_BACKGROUND_PADDING, TOOLTIP_GAP,
    TOOLTIP_PADDING, TOOLTIP_TEXT_SIZE, disabled_icon_button_style, muted_text_style, shaped_text,
    tooltip_container_style, transparent_icon_button_style,
};
use crate::types::{FilesManagementState, InputId, Message};

pub(super) fn breadcrumb_bar<'a>(current_path: &str, viewing_root: bool) -> Element<'a, Message> {
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
        let is_truncated = truncated_name.chars().count() < full_name.chars().count();

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

/// Build the toolbar with Up, Home, View Root/Home, Refresh, Download All, New Directory buttons
pub(super) fn toolbar<'a>(state: &ToolbarState<'_>) -> Element<'a, Message> {
    // Home button - tooltip changes based on viewing mode
    let home_tooltip = if state.viewing_root {
        t("tooltip-files-go-root")
    } else {
        t("tooltip-files-home")
    };
    let home_button = tooltip(
        button(icon::home().size(ICON_SIZE))
            .padding(TOOLBAR_BUTTON_PADDING)
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
        button(icon::refresh().size(ICON_SIZE))
            .padding(TOOLBAR_BUTTON_PADDING)
            .style(transparent_icon_button_style)
            .on_press(Message::FileRefresh),
        container(shaped_text(t("tooltip-files-refresh")).size(TOOLTIP_TEXT_SIZE))
            .padding(TOOLTIP_BACKGROUND_PADDING)
            .style(tooltip_container_style),
        tooltip::Position::Bottom,
    )
    .gap(TOOLTIP_GAP)
    .padding(TOOLTIP_PADDING);

    // Up button - enabled only when not at home and not searching
    let up_button: Element<'a, Message> = if state.can_go_up && !state.is_searching {
        tooltip(
            button(icon::up_dir().size(ICON_SIZE))
                .padding(TOOLBAR_BUTTON_PADDING)
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
        button(icon::up_dir().size(ICON_SIZE))
            .padding(TOOLBAR_BUTTON_PADDING)
            .style(disabled_icon_button_style)
            .into()
    };

    // Start building toolbar row with Up button first, then Home
    let mut toolbar_row = row![up_button, home_button].spacing(SPACER_SIZE_SMALL);

    // Root mode is a privileged browsing mode, not an ordinary file action.
    // Hide it without file_root so regular users don't see a confusing,
    // permanently disabled server-root toggle.
    if state.has_file_root {
        let root_toggle_tooltip = if state.viewing_root {
            t("tooltip-files-view-home")
        } else {
            t("tooltip-files-view-root")
        };

        let root_toggle_button = tooltip(
            button(icon::folder_root().size(ICON_SIZE))
                .padding(TOOLBAR_BUTTON_PADDING)
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

    // Hidden files toggle button - disabled during search (index includes all files)
    let hidden_icon = if state.show_hidden {
        icon::eye()
    } else {
        icon::eye_off()
    };
    let hidden_toggle_button: Element<'a, Message> = if !state.is_searching {
        let hidden_tooltip = if state.show_hidden {
            t("tooltip-files-hide-hidden")
        } else {
            t("tooltip-files-show-hidden")
        };
        tooltip(
            button(hidden_icon.size(ICON_SIZE))
                .padding(TOOLBAR_BUTTON_PADDING)
                .style(transparent_icon_button_style)
                .on_press(Message::FileToggleHidden),
            container(shaped_text(hidden_tooltip).size(TOOLTIP_TEXT_SIZE))
                .padding(TOOLTIP_BACKGROUND_PADDING)
                .style(tooltip_container_style),
            tooltip::Position::Bottom,
        )
        .gap(TOOLTIP_GAP)
        .padding(TOOLTIP_PADDING)
        .into()
    } else {
        button(hidden_icon.size(ICON_SIZE))
            .padding(TOOLBAR_BUTTON_PADDING)
            .style(disabled_icon_button_style)
            .into()
    };

    toolbar_row = toolbar_row.push(hidden_toggle_button);

    // Download All button - downloads current directory recursively
    // Disabled during search (no current directory to download)
    let download_all_button: Element<'a, Message> =
        if state.has_file_download && !state.is_loading && !state.is_searching {
            tooltip(
                button(icon::download().size(ICON_SIZE))
                    .padding(TOOLBAR_BUTTON_PADDING)
                    .style(transparent_icon_button_style)
                    .on_press(Message::FileDownloadAll(state.current_path.to_string())),
                container(shaped_text(t("tooltip-files-download-all")).size(TOOLTIP_TEXT_SIZE))
                    .padding(TOOLTIP_BACKGROUND_PADDING)
                    .style(tooltip_container_style),
                tooltip::Position::Bottom,
            )
            .gap(TOOLTIP_GAP)
            .padding(TOOLTIP_PADDING)
            .into()
        } else {
            // Disabled download button (shown but not clickable)
            button(icon::download().size(ICON_SIZE))
                .padding(TOOLBAR_BUTTON_PADDING)
                .style(disabled_icon_button_style)
                .into()
        };

    toolbar_row = toolbar_row.push(download_all_button);

    // Upload button - enabled if user has file_upload permission AND current dir allows upload
    // Disabled during search (no destination directory)
    let upload_button: Element<'a, Message> =
        if state.has_file_upload && state.can_upload && !state.is_loading && !state.is_searching {
            tooltip(
                button(icon::upload().size(ICON_SIZE))
                    .padding(TOOLBAR_BUTTON_PADDING)
                    .style(transparent_icon_button_style)
                    .on_press(Message::FileUpload(state.current_path.to_string())),
                container(shaped_text(t("tooltip-upload")).size(TOOLTIP_TEXT_SIZE))
                    .padding(TOOLTIP_BACKGROUND_PADDING)
                    .style(tooltip_container_style),
                tooltip::Position::Bottom,
            )
            .gap(TOOLTIP_GAP)
            .padding(TOOLTIP_PADDING)
            .into()
        } else {
            button(icon::upload().size(ICON_SIZE))
                .padding(TOOLBAR_BUTTON_PADDING)
                .style(disabled_icon_button_style)
                .into()
        };

    toolbar_row = toolbar_row.push(upload_button);

    // New Directory button - enabled if user has file_create_dir permission OR current dir allows upload
    // Disabled while loading or searching (no parent directory)
    let new_dir_button: Element<'a, Message> =
        if state.can_create_dir && !state.is_loading && !state.is_searching {
            tooltip(
                button(icon::folder_empty().size(ICON_SIZE))
                    .padding(TOOLBAR_BUTTON_PADDING)
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
            button(icon::folder_empty().size(ICON_SIZE))
                .padding(TOOLBAR_BUTTON_PADDING)
                .style(disabled_icon_button_style)
                .into()
        };

    toolbar_row = toolbar_row.push(new_dir_button);

    // Paste button - enabled if clipboard has content and not loading
    // Disabled during search (no destination directory)
    let paste_button: Element<'a, Message> =
        if state.has_clipboard && !state.is_loading && !state.is_searching {
            tooltip(
                button(icon::paste().size(ICON_SIZE))
                    .padding(TOOLBAR_BUTTON_PADDING)
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
            button(icon::paste().size(ICON_SIZE))
                .padding(TOOLBAR_BUTTON_PADDING)
                .style(disabled_icon_button_style)
                .into()
        };

    toolbar_row = toolbar_row.push(paste_button);

    toolbar_row.into()
}

/// Build the search input row
pub(super) fn search_input_row<'a>(
    search_input: &str,
    search_loading: bool,
) -> Element<'a, Message> {
    let input = text_input(&t("files-search-placeholder"), search_input)
        .id(InputId::FileSearchInput)
        .on_input(Message::FileSearchInputChanged)
        .on_submit(Message::FileSearchSubmit)
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE)
        .width(Fill);

    let search_button: Element<'_, Message> = if search_loading {
        button(icon::search().size(ICON_SIZE))
            .padding(TOOLBAR_BUTTON_PADDING)
            .style(disabled_icon_button_style)
            .into()
    } else {
        tooltip(
            button(icon::search().size(ICON_SIZE))
                .padding(TOOLBAR_BUTTON_PADDING)
                .style(transparent_icon_button_style)
                .on_press(Message::FileSearchSubmit),
            container(shaped_text(t("tooltip-files-search")).size(TOOLTIP_TEXT_SIZE))
                .padding(TOOLTIP_BACKGROUND_PADDING)
                .style(tooltip_container_style),
            tooltip::Position::Bottom,
        )
        .gap(TOOLTIP_GAP)
        .padding(TOOLTIP_PADDING)
        .into()
    };

    row![input, search_button]
        .spacing(SMALL_SPACING)
        .align_y(Center)
        .into()
}

/// Build the search breadcrumb (shows "Search - {query}")
pub(super) fn search_breadcrumb<'a>(query: &str) -> Element<'a, Message> {
    let breadcrumb_text = t_args("files-search-breadcrumb", &[("query", query)]);
    container(
        shaped_text(breadcrumb_text)
            .size(TEXT_SIZE)
            .style(muted_text_style),
    )
    .padding([SPACER_SIZE_SMALL, NO_SPACING])
    .into()
}
