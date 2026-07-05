//! Shared helper functions for view rendering

use std::hash::{Hash, Hasher};

use chrono::{DateTime, Local, TimeZone, Utc};
use iced::widget::{Space, Text, button, container, tooltip};
use iced::{Color, Element, alignment};

use crate::i18n::t;
use crate::icon;
use crate::style::{
    ICON_SIZE, SORT_ICON_SIZE, TOOLBAR_BUTTON_PADDING, TOOLTIP_BACKGROUND_PADDING, TOOLTIP_GAP,
    TOOLTIP_PADDING, TOOLTIP_TEXT_SIZE, disabled_icon_button_style, muted_text_style, shaped_text,
    tooltip_container_style, transparent_icon_button_style,
};
use crate::types::Message;

/// Bytes per kilobyte.
const BYTES_PER_KB: u64 = 1024;

/// Bytes per megabyte.
const BYTES_PER_MB: u64 = BYTES_PER_KB * 1024;

/// Bytes per gigabyte.
const BYTES_PER_GB: u64 = BYTES_PER_MB * 1024;

/// Bytes per terabyte.
const BYTES_PER_TB: u64 = BYTES_PER_GB * 1024;

/// Convenience wrapper for `crate::i18n::t_args` to avoid verbose imports in view modules
pub fn t_args(key: &str, args: &[(&str, &str)]) -> String {
    crate::i18n::t_args(key, args)
}

/// Hash an exact color by component bits.
///
/// For `lazy()` dependency structs that bake theme colors into the widget
/// tree at build time (rich-text spans and color-parameterized styles):
/// hashing the precise bits means any palette change — including edits to a
/// custom theme — invalidates the cache.
pub(crate) fn hash_color<H: Hasher>(color: &Color, state: &mut H) {
    color.r.to_bits().hash(state);
    color.g.to_bits().hash(state);
    color.b.to_bits().hash(state);
    color.a.to_bits().hash(state);
}

/// Format bytes as a human-readable string with B/KB/MB/GB/TB units.
pub fn format_bytes(bytes: u64) -> String {
    if bytes >= BYTES_PER_TB {
        format!("{:.1} TB", bytes as f64 / BYTES_PER_TB as f64)
    } else if bytes >= BYTES_PER_GB {
        format!("{:.1} GB", bytes as f64 / BYTES_PER_GB as f64)
    } else if bytes >= BYTES_PER_MB {
        format!("{:.1} MB", bytes as f64 / BYTES_PER_MB as f64)
    } else if bytes >= BYTES_PER_KB {
        format!("{:.1} KB", bytes as f64 / BYTES_PER_KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Format a Unix epoch timestamp in the user's local timezone.
///
/// Returns an empty string for unset or invalid timestamps.
pub fn format_local_timestamp(timestamp: i64) -> String {
    if timestamp == 0 {
        return String::new();
    }

    Utc.timestamp_opt(timestamp, 0)
        .single()
        .map_or_else(String::new, |utc_time| {
            let local_time: DateTime<Local> = utc_time.with_timezone(&Local);
            local_time.format("%b %d, %Y %H:%M").to_string()
        })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_bytes_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1), "1 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1023), "1023 B");
    }

    #[test]
    fn format_bytes_kilobytes() {
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(10 * 1024), "10.0 KB");
        assert_eq!(format_bytes(1024 * 1024 - 1), "1024.0 KB");
    }

    #[test]
    fn format_bytes_megabytes() {
        assert_eq!(format_bytes(1024 * 1024), "1.0 MB");
        assert_eq!(format_bytes(1024 * 1024 + 512 * 1024), "1.5 MB");
        assert_eq!(format_bytes(100 * 1024 * 1024), "100.0 MB");
        assert_eq!(format_bytes(1024 * 1024 * 1024 - 1), "1024.0 MB");
    }

    #[test]
    fn format_bytes_gigabytes() {
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.0 GB");
        assert_eq!(
            format_bytes(1024 * 1024 * 1024 + 512 * 1024 * 1024),
            "1.5 GB"
        );
        assert_eq!(format_bytes(10 * 1024 * 1024 * 1024), "10.0 GB");
    }

    #[test]
    fn format_bytes_terabytes() {
        assert_eq!(format_bytes(1024 * 1024 * 1024 * 1024), "1.0 TB");
        assert_eq!(
            format_bytes(1024 * 1024 * 1024 * 1024 + 512 * 1024 * 1024 * 1024),
            "1.5 TB"
        );
    }

    #[test]
    fn format_local_timestamp_valid() {
        // 2025-01-15 10:30:00 UTC = 1736937000
        let result = format_local_timestamp(1736937000);
        assert!(!result.is_empty());
        assert!(result.contains("2025"));
    }

    #[test]
    fn format_local_timestamp_negative() {
        // Negative timestamps are valid; -1 = Dec 31, 1969 23:59:59 UTC.
        let result = format_local_timestamp(-1);
        assert!(!result.is_empty());
        assert!(result.contains("1969"));
    }

    #[test]
    fn format_local_timestamp_handles_unset() {
        assert_eq!(format_local_timestamp(0), "");
    }
}
