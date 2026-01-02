//! Transfers panel view (download/upload progress)
//!
//! A global panel showing all file transfers across all connections.
//! Transfers persist across application restarts for resume support.

use iced::widget::{Column, Row, Space, button, column, container, progress_bar, row, scrollable};
use iced::{Center, Element, Fill, Length};

use crate::i18n::t;
use crate::icon;
use crate::style::{
    BUTTON_PADDING, ELEMENT_SPACING, FORM_PADDING, NEWS_ACTION_BUTTON_SIZE, NEWS_ACTION_ICON_SIZE,
    SCROLLBAR_PADDING, SIDEBAR_ACTION_ICON_SIZE, SMALL_SPACING, SPACER_SIZE_SMALL, TEXT_SIZE,
    TRANSFER_ITEM_SPACING, TRANSFER_LIST_MAX_WIDTH, TRANSFER_ROW_PADDING, alternating_row_style,
    content_background_style, danger_icon_button_style, error_text_style, muted_text_style,
    panel_title, shaped_text, transparent_icon_button_style,
};
use crate::transfers::{Transfer, TransferManager, TransferStatus};
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
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Format duration in seconds as human-readable string (e.g., "5m 30s")
fn format_duration(seconds: i64) -> String {
    if seconds < 60 {
        format!("{}s", seconds)
    } else if seconds < 3600 {
        let mins = seconds / 60;
        let secs = seconds % 60;
        if secs > 0 {
            format!("{}m {}s", mins, secs)
        } else {
            format!("{}m", mins)
        }
    } else {
        let hours = seconds / 3600;
        let mins = (seconds % 3600) / 60;
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
    if speed < 1.0 {
        return None;
    }
    let remaining_bytes = transfer
        .total_bytes
        .saturating_sub(transfer.transferred_bytes);
    let remaining_seconds = (remaining_bytes as f64 / speed) as i64;
    Some(format_duration(remaining_seconds))
}

/// Build a transparent icon button for transfer actions
fn action_button<'a>(
    icon: iced::widget::Text<'a>,
    message: Message,
) -> button::Button<'a, Message> {
    button(icon.size(NEWS_ACTION_ICON_SIZE))
        .on_press(message)
        .width(NEWS_ACTION_BUTTON_SIZE)
        .height(NEWS_ACTION_BUTTON_SIZE)
        .style(transparent_icon_button_style)
}

/// Build a danger icon button for transfer actions (cancel/remove)
fn danger_action_button<'a>(
    icon: iced::widget::Text<'a>,
    message: Message,
) -> button::Button<'a, Message> {
    button(icon.size(NEWS_ACTION_ICON_SIZE))
        .on_press(message)
        .width(NEWS_ACTION_BUTTON_SIZE)
        .height(NEWS_ACTION_BUTTON_SIZE)
        .style(danger_icon_button_style)
}

/// Build a single transfer row
fn build_transfer_row<'a>(transfer: &Transfer, index: usize) -> Element<'a, Message> {
    let id = transfer.id;

    // Left side: name, server, and status info
    let name = shaped_text(transfer.display_name()).size(TEXT_SIZE);

    let server_info = shaped_text(&transfer.connection.server_name)
        .size(TEXT_SIZE - 2.0)
        .style(muted_text_style);

    // Status line with speed/progress for active transfers
    let status_line: Element<'a, Message> = match transfer.status {
        TransferStatus::Transferring => {
            let mut parts = vec![status_text(transfer.status)];

            // Add speed if available
            if let Some(speed) = transfer.bytes_per_second() {
                parts.push(format!(" • {}/s", format_bytes(speed as u64)));
            }

            // Add ETA if available
            if let Some(eta) = estimate_remaining(transfer) {
                parts.push(format!(" • ~{}", eta));
            }

            shaped_text(parts.join(""))
                .size(TEXT_SIZE - 2.0)
                .style(muted_text_style)
                .into()
        }
        TransferStatus::Failed => {
            let error_msg = transfer.error.as_deref().unwrap_or_default();
            shaped_text(format!("{}: {}", status_text(transfer.status), error_msg))
                .size(TEXT_SIZE - 2.0)
                .style(error_text_style)
                .into()
        }
        TransferStatus::Completed => {
            let mut parts = vec![status_text(transfer.status)];

            // Show elapsed time for completed transfers
            if let Some(elapsed) = transfer.elapsed_seconds() {
                parts.push(format!(" in {}", format_duration(elapsed)));
            }

            shaped_text(parts.join(""))
                .size(TEXT_SIZE - 2.0)
                .style(muted_text_style)
                .into()
        }
        _ => shaped_text(status_text(transfer.status))
            .size(TEXT_SIZE - 2.0)
            .style(muted_text_style)
            .into(),
    };

    // Progress info (bytes transferred / total)
    let progress_text = if transfer.total_bytes > 0 {
        format!(
            "{} / {} ({:.0}%)",
            format_bytes(transfer.transferred_bytes),
            format_bytes(transfer.total_bytes),
            transfer.progress_percent()
        )
    } else if transfer.status == TransferStatus::Completed {
        format_bytes(transfer.transferred_bytes)
    } else {
        String::new()
    };

    let left_info = column![name, server_info, status_line]
        .spacing(2.0)
        .width(Fill);

    // Progress bar (only show for active transfers with known size)
    let progress_section: Element<'a, Message> = if transfer.total_bytes > 0
        && (transfer.status == TransferStatus::Transferring
            || transfer.status == TransferStatus::Connecting
            || transfer.status == TransferStatus::Paused)
    {
        let progress = transfer.progress_percent() / 100.0;
        column![
            progress_bar(0.0..=1.0, progress),
            shaped_text(progress_text)
                .size(TEXT_SIZE - 2.0)
                .style(muted_text_style),
        ]
        .spacing(2.0)
        .width(Length::Fixed(150.0))
        .into()
    } else if !progress_text.is_empty() {
        shaped_text(progress_text)
            .size(TEXT_SIZE - 2.0)
            .style(muted_text_style)
            .width(Length::Fixed(150.0))
            .into()
    } else {
        Space::new().into()
    };

    // Action buttons based on status
    let actions: Element<'a, Message> = match transfer.status {
        TransferStatus::Queued => {
            // Can cancel queued transfers
            row![danger_action_button(
                icon::close(),
                Message::TransferCancel(id)
            )]
            .spacing(SMALL_SPACING)
            .into()
        }
        TransferStatus::Connecting | TransferStatus::Transferring => {
            // Can pause or cancel active transfers
            row![
                action_button(icon::pause(), Message::TransferPause(id)),
                danger_action_button(icon::close(), Message::TransferCancel(id)),
            ]
            .spacing(SMALL_SPACING)
            .into()
        }
        TransferStatus::Paused => {
            // Can resume or cancel paused transfers
            row![
                action_button(icon::play(), Message::TransferResume(id)),
                danger_action_button(icon::close(), Message::TransferCancel(id)),
            ]
            .spacing(SMALL_SPACING)
            .into()
        }
        TransferStatus::Completed => {
            // Can open folder or remove completed transfers
            row![
                action_button(icon::folder(), Message::TransferOpenFolder(id)),
                danger_action_button(icon::close(), Message::TransferRemove(id)),
            ]
            .spacing(SMALL_SPACING)
            .into()
        }
        TransferStatus::Failed => {
            // Can retry or remove failed transfers
            row![
                action_button(icon::play(), Message::TransferResume(id)),
                danger_action_button(icon::close(), Message::TransferRemove(id)),
            ]
            .spacing(SMALL_SPACING)
            .into()
        }
    };

    let row_content: Row<'_, Message> = row![left_info, progress_section, actions]
        .spacing(ELEMENT_SPACING)
        .padding(TRANSFER_ROW_PADDING)
        .align_y(Center);

    container(row_content)
        .width(Fill)
        .style(alternating_row_style(index.is_multiple_of(2)))
        .into()
}

/// Build the toolbar with clear buttons
fn build_toolbar<'a>(manager: &TransferManager) -> Element<'a, Message> {
    let has_completed = manager.completed().next().is_some();
    let has_failed = manager.failed().next().is_some();

    let mut toolbar_items: Vec<Element<'a, Message>> = Vec::new();

    // Clear Completed button
    if has_completed {
        let clear_completed = button(
            row![
                icon::trash().size(SIDEBAR_ACTION_ICON_SIZE),
                Space::new().width(SMALL_SPACING),
                shaped_text(t("transfer-clear-completed")).size(TEXT_SIZE),
            ]
            .align_y(Center),
        )
        .on_press(Message::TransferClearCompleted)
        .padding(BUTTON_PADDING)
        .style(transparent_icon_button_style);

        toolbar_items.push(clear_completed.into());
    }

    // Clear Failed button
    if has_failed {
        let clear_failed = button(
            row![
                icon::trash().size(SIDEBAR_ACTION_ICON_SIZE),
                Space::new().width(SMALL_SPACING),
                shaped_text(t("transfer-clear-failed")).size(TEXT_SIZE),
            ]
            .align_y(Center),
        )
        .on_press(Message::TransferClearFailed)
        .padding(BUTTON_PADDING)
        .style(danger_icon_button_style);

        toolbar_items.push(clear_failed.into());
    }

    if toolbar_items.is_empty() {
        Space::new().into()
    } else {
        row(toolbar_items)
            .spacing(ELEMENT_SPACING)
            .align_y(Center)
            .into()
    }
}

// ============================================================================
// Main View Function
// ============================================================================

/// Displays the transfers panel
///
/// Shows a list of all transfers (active, queued, paused, completed, failed).
/// Provides action buttons for pause/resume/cancel/remove operations.
pub fn transfers_view<'a>(manager: &'a TransferManager) -> Element<'a, Message> {
    // Title row (centered, using panel_title helper for consistent sizing)
    let title_row = panel_title(t("title-transfers"));

    // Build toolbar
    let toolbar = build_toolbar(manager);

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
        toolbar,
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
