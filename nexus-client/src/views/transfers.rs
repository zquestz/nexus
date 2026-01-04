//! Transfers panel view (download/upload progress)
//!
//! A global panel showing all file transfers across all connections.
//! Transfers persist across application restarts for resume support.

// ============================================================================
// Constants
// ============================================================================

/// Bullet separator for status line (e.g., "Server • Status • Speed")
const BULLET_SEPARATOR: &str = " • ";

/// Em dash separator for error messages (e.g., "Status — Error message")
const EM_DASH_SEPARATOR: &str = " — ";

/// Seconds per minute (for duration formatting)
const SECONDS_PER_MINUTE: i64 = 60;

/// Seconds per hour (for duration formatting)
const SECONDS_PER_HOUR: i64 = 3600;

/// Bytes per kilobyte
const BYTES_PER_KB: u64 = 1024;

/// Bytes per megabyte
const BYTES_PER_MB: u64 = BYTES_PER_KB * 1024;

/// Bytes per gigabyte
const BYTES_PER_GB: u64 = BYTES_PER_MB * 1024;

/// Minimum speed threshold for ETA calculation (bytes/second)
const MIN_SPEED_FOR_ETA: f64 = 1.0;

// ============================================================================
// Imports
// ============================================================================

use iced::alignment;
use iced::widget::{
    Column, Space, button, column, container, progress_bar, row, scrollable, tooltip,
};
use iced::{Center, Element, Fill};

use crate::i18n::{t, t_args};
use crate::icon;
use crate::style::{
    DETAIL_TEXT_SIZE, ELEMENT_SPACING, FORM_PADDING, ICON_BUTTON_PADDING, SCROLLBAR_PADDING,
    SIDEBAR_ACTION_ICON_SIZE, SMALL_SPACING, SPACER_SIZE_SMALL, TEXT_SIZE, TITLE_SIZE,
    TOOLTIP_BACKGROUND_PADDING, TOOLTIP_GAP, TOOLTIP_PADDING, TOOLTIP_TEXT_SIZE,
    TRANSFER_ACTION_BUTTON_SIZE, TRANSFER_ACTION_ICON_SIZE, TRANSFER_ICON_SIZE,
    TRANSFER_INFO_SPACING, TRANSFER_ITEM_SPACING, TRANSFER_LIST_MAX_WIDTH,
    TRANSFER_PROGRESS_BAR_HEIGHT, TRANSFER_PROGRESS_SPACING, TRANSFER_ROW_PADDING,
    alternating_row_style, content_background_style, danger_icon_button_style,
    disabled_icon_button_style, error_text_style, muted_text_style, shaped_text,
    tooltip_container_style, transparent_icon_button_style,
};
use crate::transfers::{Transfer, TransferDirection, TransferManager, TransferStatus};
use crate::types::Message;

// ============================================================================
// Helper Functions
// ============================================================================

/// Get a translated display string for the transfer status
fn status_text(status: TransferStatus) -> String {
    match status {
        TransferStatus::Queued => t("transfer-status-queued"),
        TransferStatus::Connecting => t("transfer-status-connecting"),
        TransferStatus::Transferring => t("transfer-status-transferring"),
        TransferStatus::Paused => t("transfer-status-paused"),
        TransferStatus::Completed => t("transfer-status-completed"),
        TransferStatus::Failed => t("transfer-status-failed"),
    }
}

/// Format bytes as human-readable string (e.g., "1.5 MB")
fn format_bytes(bytes: u64) -> String {
    if bytes >= BYTES_PER_GB {
        format!("{:.1} GB", bytes as f64 / BYTES_PER_GB as f64)
    } else if bytes >= BYTES_PER_MB {
        format!("{:.1} MB", bytes as f64 / BYTES_PER_MB as f64)
    } else if bytes >= BYTES_PER_KB {
        format!("{:.1} KB", bytes as f64 / BYTES_PER_KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Format duration in seconds as human-readable string (e.g., "5m 30s")
fn format_duration(seconds: i64) -> String {
    if seconds < SECONDS_PER_MINUTE {
        format!("{}s", seconds)
    } else if seconds < SECONDS_PER_HOUR {
        let mins = seconds / SECONDS_PER_MINUTE;
        let secs = seconds % SECONDS_PER_MINUTE;
        if secs > 0 {
            format!("{}m {}s", mins, secs)
        } else {
            format!("{}m", mins)
        }
    } else {
        let hours = seconds / SECONDS_PER_HOUR;
        let mins = (seconds % SECONDS_PER_HOUR) / SECONDS_PER_MINUTE;
        if mins > 0 {
            format!("{}h {}m", hours, mins)
        } else {
            format!("{}h", hours)
        }
    }
}

/// Calculate estimated time remaining based on speed and remaining bytes
fn estimate_remaining(transfer: &Transfer) -> Option<String> {
    let speed = transfer.bytes_per_second()?;
    if speed < MIN_SPEED_FOR_ETA {
        return None;
    }
    let remaining_bytes = transfer
        .total_bytes
        .saturating_sub(transfer.transferred_bytes);
    let remaining_seconds = (remaining_bytes as f64 / speed) as i64;
    Some(format_duration(remaining_seconds))
}

/// Build a transparent icon button with tooltip for transfer actions
fn action_button_with_tooltip<'a>(
    icon: iced::widget::Text<'a>,
    message: Message,
    tooltip_key: &str,
) -> Element<'a, Message> {
    let btn = button(icon.size(TRANSFER_ACTION_ICON_SIZE))
        .on_press(message)
        .width(TRANSFER_ACTION_BUTTON_SIZE)
        .height(TRANSFER_ACTION_BUTTON_SIZE)
        .style(transparent_icon_button_style);

    tooltip(
        btn,
        container(shaped_text(t(tooltip_key)).size(TOOLTIP_TEXT_SIZE))
            .padding(TOOLTIP_BACKGROUND_PADDING)
            .style(tooltip_container_style),
        tooltip::Position::Top,
    )
    .gap(TOOLTIP_GAP)
    .padding(TOOLTIP_PADDING)
    .into()
}

/// Build a danger icon button with tooltip for transfer actions (cancel/remove)
fn danger_action_button_with_tooltip<'a>(
    icon: iced::widget::Text<'a>,
    message: Message,
    tooltip_key: &str,
) -> Element<'a, Message> {
    let btn = button(icon.size(TRANSFER_ACTION_ICON_SIZE))
        .on_press(message)
        .width(TRANSFER_ACTION_BUTTON_SIZE)
        .height(TRANSFER_ACTION_BUTTON_SIZE)
        .style(danger_icon_button_style);

    tooltip(
        btn,
        container(shaped_text(t(tooltip_key)).size(TOOLTIP_TEXT_SIZE))
            .padding(TOOLTIP_BACKGROUND_PADDING)
            .style(tooltip_container_style),
        tooltip::Position::Top,
    )
    .gap(TOOLTIP_GAP)
    .padding(TOOLTIP_PADDING)
    .into()
}

/// Build a single transfer row
///
/// Layout:
/// ```text
/// ┌─────────────────────────────────────────────────────────────────────┐
/// │ ↓  Transfer Name                                           ▶  ✕    │
/// │    [████████████████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░] │
/// │    Server • Status • 1.5 MB/s • ~2m • 192.0 KB / 1.0 GB            │
/// └─────────────────────────────────────────────────────────────────────┘
/// ```
fn build_transfer_row<'a>(transfer: &Transfer, index: usize) -> Element<'a, Message> {
    let id = transfer.id;

    // Direction icon (download or upload)
    let direction_icon = match transfer.direction {
        TransferDirection::Download => icon::download(),
        TransferDirection::Upload => icon::upload(),
    };

    // Transfer name (title)
    let name = shaped_text(transfer.display_name()).size(TEXT_SIZE);

    // Action buttons based on status (top-right corner)
    let actions: Element<'a, Message> = match transfer.status {
        TransferStatus::Queued => row![danger_action_button_with_tooltip(
            icon::close(),
            Message::TransferCancel(id),
            "tooltip-transfer-cancel"
        )]
        .spacing(SMALL_SPACING)
        .into(),
        TransferStatus::Connecting | TransferStatus::Transferring => row![
            action_button_with_tooltip(
                icon::pause(),
                Message::TransferPause(id),
                "tooltip-transfer-pause"
            ),
            danger_action_button_with_tooltip(
                icon::close(),
                Message::TransferCancel(id),
                "tooltip-transfer-cancel"
            ),
        ]
        .spacing(SMALL_SPACING)
        .into(),
        TransferStatus::Paused => row![
            action_button_with_tooltip(
                icon::play(),
                Message::TransferResume(id),
                "tooltip-transfer-resume"
            ),
            danger_action_button_with_tooltip(
                icon::close(),
                Message::TransferCancel(id),
                "tooltip-transfer-cancel"
            ),
        ]
        .spacing(SMALL_SPACING)
        .into(),
        TransferStatus::Completed => row![
            action_button_with_tooltip(
                icon::folder(),
                Message::TransferOpenFolder(id),
                "tooltip-transfer-open-folder"
            ),
            danger_action_button_with_tooltip(
                icon::close(),
                Message::TransferRemove(id),
                "tooltip-transfer-remove"
            ),
        ]
        .spacing(SMALL_SPACING)
        .into(),
        TransferStatus::Failed => row![
            action_button_with_tooltip(
                icon::play(),
                Message::TransferResume(id),
                "tooltip-transfer-resume"
            ),
            action_button_with_tooltip(
                icon::folder(),
                Message::TransferOpenFolder(id),
                "tooltip-transfer-open-folder"
            ),
            danger_action_button_with_tooltip(
                icon::close(),
                Message::TransferRemove(id),
                "tooltip-transfer-remove"
            ),
        ]
        .spacing(SMALL_SPACING)
        .into(),
    };

    // First row: icon, name, spacer, action buttons
    // Name uses Fill to take available space, actions use Shrink to stay visible
    let title_row = row![
        direction_icon.size(TRANSFER_ICON_SIZE),
        container(name).width(Fill),
        actions,
    ]
    .spacing(ELEMENT_SPACING)
    .align_y(Center);

    // Build size text for transfers with known size
    let size_text = if transfer.total_bytes > 0 {
        Some(format!(
            "{} / {}",
            format_bytes(transfer.transferred_bytes),
            format_bytes(transfer.total_bytes),
        ))
    } else if transfer.status == TransferStatus::Completed && transfer.transferred_bytes > 0 {
        // Completed but no total_bytes known - just show what was transferred
        Some(format_bytes(transfer.transferred_bytes))
    } else {
        None
    };

    // Server + status line (e.g., "The Lag • Paused" or "The Lag • Transferring • 1.5 MB/s • ~2m • 72.9 / 521.0 MB")
    let status_line: String = match transfer.status {
        TransferStatus::Transferring => {
            let mut line = [
                transfer.connection_info.server_name.as_str(),
                &status_text(transfer.status),
            ]
            .join(BULLET_SEPARATOR);
            if let Some(speed) = transfer.bytes_per_second() {
                line.push_str(BULLET_SEPARATOR);
                line.push_str(&format!("{}/s", format_bytes(speed as u64)));
            }
            if let Some(eta) = estimate_remaining(transfer) {
                line.push_str(BULLET_SEPARATOR);
                line.push_str(&format!("~{}", eta));
            }
            if let Some(ref size) = size_text {
                line.push_str(BULLET_SEPARATOR);
                line.push_str(size);
            }
            line
        }
        TransferStatus::Completed => {
            let mut line = transfer.connection_info.server_name.clone();
            let status = if let Some(elapsed) = transfer.elapsed_seconds() {
                t_args(
                    "transfer-completed-in",
                    &[("time", &format_duration(elapsed))],
                )
            } else {
                status_text(transfer.status)
            };
            line.push_str(BULLET_SEPARATOR);
            line.push_str(&status);
            if let Some(ref size) = size_text {
                line.push_str(BULLET_SEPARATOR);
                line.push_str(size);
            }
            line
        }
        _ => {
            let mut line = [
                transfer.connection_info.server_name.as_str(),
                &status_text(transfer.status),
            ]
            .join(BULLET_SEPARATOR);
            if let Some(ref size) = size_text {
                line.push_str(BULLET_SEPARATOR);
                line.push_str(size);
            }
            line
        }
    };

    let status_row: Element<'a, Message> = if transfer.status == TransferStatus::Failed {
        // For failed transfers, show error in red
        let error_msg = transfer.error.as_deref().unwrap_or_default();
        let text = if error_msg.is_empty() {
            status_line
        } else {
            format!("{}{}{}", status_line, EM_DASH_SEPARATOR, error_msg)
        };
        shaped_text(text)
            .size(DETAIL_TEXT_SIZE)
            .style(error_text_style)
            .into()
    } else {
        shaped_text(status_line)
            .size(DETAIL_TEXT_SIZE)
            .style(muted_text_style)
            .into()
    };

    // Progress bar row (full width, with spacing before and after)
    let progress_row: Element<'a, Message> = if transfer.total_bytes > 0 {
        let progress = transfer.progress_percent() / 100.0;
        column![
            Space::new().height(TRANSFER_PROGRESS_SPACING),
            container(progress_bar(0.0..=1.0, progress).girth(TRANSFER_PROGRESS_BAR_HEIGHT))
                .width(Fill),
            Space::new().height(TRANSFER_PROGRESS_SPACING),
        ]
        .into()
    } else {
        // No progress info available (queued, connecting, etc.)
        Space::new().height(0).into()
    };

    // Combine all rows into a column: Item → Bar → Stats
    let content = column![title_row, progress_row, status_row,]
        .spacing(TRANSFER_INFO_SPACING)
        .padding(TRANSFER_ROW_PADDING)
        .width(Fill);

    container(content)
        .width(Fill)
        .style(alternating_row_style(index.is_multiple_of(2)))
        .into()
}

// ============================================================================
// Main View Function
// ============================================================================

/// Displays the transfers panel
///
/// Shows a list of all transfers (active, queued, paused, completed, failed).
/// Provides action buttons for pause/resume/cancel/remove operations.
pub fn transfers_view<'a>(manager: &'a TransferManager) -> Element<'a, Message> {
    // Check if there are any inactive (completed or failed) transfers to clear
    let has_inactive = manager.completed().next().is_some() || manager.failed().next().is_some();

    // Clear Inactive button (in title row) - always visible, disabled when no inactive transfers
    let clear_inactive_btn: Element<'a, Message> = {
        let trash_icon = container(icon::trash().size(SIDEBAR_ACTION_ICON_SIZE))
            .width(SIDEBAR_ACTION_ICON_SIZE)
            .height(SIDEBAR_ACTION_ICON_SIZE)
            .align_x(alignment::Horizontal::Center)
            .align_y(alignment::Vertical::Center);

        let btn = button(trash_icon).padding(ICON_BUTTON_PADDING);

        let btn = if has_inactive {
            btn.on_press(Message::TransferClearInactive)
                .style(danger_icon_button_style)
        } else {
            btn.style(disabled_icon_button_style)
        };

        tooltip(
            btn,
            container(shaped_text(t("tooltip-clear-inactive")).size(TOOLTIP_TEXT_SIZE))
                .padding(TOOLTIP_BACKGROUND_PADDING)
                .style(tooltip_container_style),
            tooltip::Position::Top,
        )
        .gap(TOOLTIP_GAP)
        .padding(TOOLTIP_PADDING)
        .into()
    };

    // Title row with clear inactive button on the right
    // We add an invisible spacer on the left to balance the button width for proper centering
    let button_width =
        SIDEBAR_ACTION_ICON_SIZE + ICON_BUTTON_PADDING.left + ICON_BUTTON_PADDING.right;
    let title_row: Element<'a, Message> = row![
        Space::new().width(SCROLLBAR_PADDING),
        Space::new().width(button_width), // Balance the button on the right
        shaped_text(t("title-transfers"))
            .size(TITLE_SIZE)
            .width(Fill)
            .align_x(Center),
        clear_inactive_btn,
        Space::new().width(SCROLLBAR_PADDING),
    ]
    .align_y(Center)
    .into();

    // Build transfer list
    let transfers: Vec<&Transfer> = manager.all().collect();

    let scroll_content: Element<'a, Message> = if transfers.is_empty() {
        // Empty state - just show muted text
        shaped_text(t("transfers-empty"))
            .size(TEXT_SIZE)
            .width(Fill)
            .align_x(Center)
            .style(muted_text_style)
            .into()
    } else {
        // Build transfer rows, sorted by status priority then created_at
        // Active first, then queued, paused, failed, completed
        let mut sorted_transfers = transfers;
        sorted_transfers.sort_by(|a, b| {
            let status_priority = |s: TransferStatus| -> u8 {
                match s {
                    TransferStatus::Transferring => 0,
                    TransferStatus::Connecting => 1,
                    TransferStatus::Queued => 2,
                    TransferStatus::Paused => 3,
                    TransferStatus::Failed => 4,
                    TransferStatus::Completed => 5,
                }
            };
            let a_priority = status_priority(a.status);
            let b_priority = status_priority(b.status);
            if a_priority != b_priority {
                a_priority.cmp(&b_priority)
            } else {
                // Within same status, newest first
                b.created_at.cmp(&a.created_at)
            }
        });

        let mut rows = Column::new().spacing(TRANSFER_ITEM_SPACING);

        for (index, transfer) in sorted_transfers.iter().enumerate() {
            rows = rows.push(build_transfer_row(transfer, index));
        }

        rows.width(Fill).into()
    };

    // Scrollable content with symmetric padding for scrollbar space
    let padded_scroll_content = row![
        Space::new().width(SCROLLBAR_PADDING),
        container(scroll_content).width(Fill),
        Space::new().width(SCROLLBAR_PADDING),
    ];

    // Build the form with max_width constraint (matching news panel)
    let form = column![
        title_row,
        Space::new().height(SPACER_SIZE_SMALL),
        container(scrollable(padded_scroll_content)).height(Fill),
    ]
    .spacing(ELEMENT_SPACING)
    .align_x(Center)
    .padding(iced::Padding {
        top: FORM_PADDING,
        right: FORM_PADDING - SCROLLBAR_PADDING,
        bottom: FORM_PADDING,
        left: FORM_PADDING - SCROLLBAR_PADDING,
    })
    .max_width(TRANSFER_LIST_MAX_WIDTH + SCROLLBAR_PADDING * 2.0)
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

    // ==================== format_bytes tests ====================

    #[test]
    fn test_format_bytes_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1), "1 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1023), "1023 B");
    }

    #[test]
    fn test_format_bytes_kilobytes() {
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(10 * 1024), "10.0 KB");
        assert_eq!(format_bytes(1024 * 1024 - 1), "1024.0 KB");
    }

    #[test]
    fn test_format_bytes_megabytes() {
        assert_eq!(format_bytes(1024 * 1024), "1.0 MB");
        assert_eq!(format_bytes(1024 * 1024 + 512 * 1024), "1.5 MB");
        assert_eq!(format_bytes(100 * 1024 * 1024), "100.0 MB");
        assert_eq!(format_bytes(1024 * 1024 * 1024 - 1), "1024.0 MB");
    }

    #[test]
    fn test_format_bytes_gigabytes() {
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.0 GB");
        assert_eq!(
            format_bytes(1024 * 1024 * 1024 + 512 * 1024 * 1024),
            "1.5 GB"
        );
        assert_eq!(format_bytes(10 * 1024 * 1024 * 1024), "10.0 GB");
    }

    // ==================== format_duration tests ====================

    #[test]
    fn test_format_duration_seconds() {
        assert_eq!(format_duration(0), "0s");
        assert_eq!(format_duration(1), "1s");
        assert_eq!(format_duration(30), "30s");
        assert_eq!(format_duration(59), "59s");
    }

    #[test]
    fn test_format_duration_minutes() {
        assert_eq!(format_duration(60), "1m");
        assert_eq!(format_duration(61), "1m 1s");
        assert_eq!(format_duration(90), "1m 30s");
        assert_eq!(format_duration(120), "2m");
        assert_eq!(format_duration(3599), "59m 59s");
    }

    #[test]
    fn test_format_duration_hours() {
        assert_eq!(format_duration(3600), "1h");
        assert_eq!(format_duration(3660), "1h 1m");
        assert_eq!(format_duration(5400), "1h 30m");
        assert_eq!(format_duration(7200), "2h");
        assert_eq!(format_duration(36000), "10h");
    }

    #[test]
    fn test_format_duration_hours_ignores_seconds() {
        // When hours are shown, seconds are not displayed
        assert_eq!(format_duration(3661), "1h 1m"); // 1h 1m 1s -> 1h 1m
        assert_eq!(format_duration(3659), "1h"); // 59m 59s rounds to 1h (no minutes)
    }
}
