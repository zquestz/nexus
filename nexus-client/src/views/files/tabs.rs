//! File browser tab bar

use iced::Element;
use iced::widget::{button, container, row, tooltip};

use crate::i18n::t;
use crate::icon;
use crate::style::{
    CLOSE_BUTTON_PADDING, INPUT_PADDING, SMALL_SPACING, TAB_CONTENT_PADDING, TEXT_SIZE,
    TOOLTIP_BACKGROUND_PADDING, TOOLTIP_GAP, TOOLTIP_PADDING, TOOLTIP_TEXT_SIZE,
    chat_tab_active_style, close_button_on_primary_style, shaped_text, tooltip_container_style,
};
use crate::types::{FileTab, FilesManagementState, Message, TabId};

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
            container(shaped_text(t("tooltip-tab-close")).size(TOOLTIP_TEXT_SIZE))
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
pub(super) fn build_file_tab_bar(
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
