//! Shared helper functions for view rendering

use iced::widget::{Space, Text, button, container, tooltip};
use iced::{Element, alignment};

use crate::i18n::t;
use crate::icon;
use crate::style::{
    ICON_SIZE, SORT_ICON_SIZE, TOOLBAR_BUTTON_PADDING, TOOLTIP_BACKGROUND_PADDING, TOOLTIP_GAP,
    TOOLTIP_PADDING, TOOLTIP_TEXT_SIZE, disabled_icon_button_style, muted_text_style, shaped_text,
    tooltip_container_style, transparent_icon_button_style,
};
use crate::types::Message;

/// Convenience wrapper for `crate::i18n::t_args` to avoid verbose imports in view modules
pub fn t_args(key: &str, args: &[(&str, &str)]) -> String {
    crate::i18n::t_args(key, args)
}

/// Build a sidebar-sized icon button with a tooltip, with an enabled/disabled state.
///
/// When `enabled`, renders as a clickable button wrapped in a top-positioned tooltip.
/// When disabled, renders as an inert button with `disabled_icon_button_style` and no
/// tooltip — matching the convention used by the file browser toolbar (e.g. the Up
/// button in `views/files/toolbar.rs`).
pub fn tab_toolbar_icon_button<'a>(
    icon: Text<'a>,
    tooltip_key: &str,
    on_press: Message,
    enabled: bool,
) -> Element<'a, Message> {
    let icon_widget = container(icon.size(ICON_SIZE))
        .width(ICON_SIZE)
        .height(ICON_SIZE)
        .align_x(alignment::Horizontal::Center)
        .align_y(alignment::Vertical::Center);

    if enabled {
        tooltip(
            button(icon_widget)
                .on_press(on_press)
                .padding(TOOLBAR_BUTTON_PADDING)
                .style(transparent_icon_button_style),
            container(shaped_text(t(tooltip_key)).size(TOOLTIP_TEXT_SIZE))
                .padding(TOOLTIP_BACKGROUND_PADDING)
                .style(tooltip_container_style),
            tooltip::Position::Top,
        )
        .gap(TOOLTIP_GAP)
        .padding(TOOLTIP_PADDING)
        .into()
    } else {
        button(icon_widget)
            .padding(TOOLBAR_BUTTON_PADDING)
            .style(disabled_icon_button_style)
            .into()
    }
}

/// Build the sort indicator for a sortable table column header.
///
/// Active branch wraps the icon in a fixed-width container so the rendered
/// width matches the inactive `Space` placeholder exactly. `.size()` sets
/// font size, not glyph width, so without the container the column visibly
/// jiggles when sort is toggled — only noticeable when the column itself is
/// `Length::Shrink`, but the cost is the same either way and the behaviour
/// is now consistent across all sortable tables.
pub fn sort_icon_or_placeholder(is_active: bool, is_ascending: bool) -> Element<'static, Message> {
    if is_active {
        let icon = if is_ascending {
            icon::down_dir()
        } else {
            icon::up_dir()
        };
        container(icon.size(SORT_ICON_SIZE).style(muted_text_style))
            .width(SORT_ICON_SIZE)
            .center_x(SORT_ICON_SIZE)
            .into()
    } else {
        Space::new().width(SORT_ICON_SIZE).into()
    }
}
