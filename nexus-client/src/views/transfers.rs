//! Transfers panel view (download/upload progress)
//!
//! A global panel showing all file transfers across all connections.
//! Transfers persist across application restarts for resume support.

use iced::widget::{Column, Row, Space, column, container, row, scrollable};
use iced::{Center, Element, Fill};

use crate::i18n::t;
use crate::style::{
    ELEMENT_SPACING, FORM_PADDING, SCROLLBAR_PADDING, SPACER_SIZE_SMALL, TEXT_SIZE,
    TRANSFER_ITEM_SPACING, TRANSFER_LIST_MAX_WIDTH, TRANSFER_ROW_PADDING, alternating_row_style,
    content_background_style, muted_text_style, panel_title, shaped_text,
};
use crate::transfers::{Transfer, TransferManager, TransferStatus};
use crate::types::Message;

// ============================================================================
// Helper Functions
// ============================================================================

/// Get a display string for the transfer status
fn status_text(status: TransferStatus) -> &'static str {
    match status {
        TransferStatus::Queued => "Queued",
        TransferStatus::Connecting => "Connecting...",
        TransferStatus::Transferring => "Transferring...",
        TransferStatus::Paused => "Paused",
        TransferStatus::Completed => "Completed",
        TransferStatus::Failed => "Failed",
    }
}

/// Build a single transfer row
fn build_transfer_row<'a>(transfer: &Transfer, index: usize) -> Element<'a, Message> {
    let name = shaped_text(transfer.display_name())
        .size(TEXT_SIZE)
        .width(Fill);

    let status = shaped_text(status_text(transfer.status))
        .size(TEXT_SIZE)
        .style(muted_text_style);

    let row_content: Row<'_, Message> = row![name, status]
        .spacing(ELEMENT_SPACING)
        .padding(TRANSFER_ROW_PADDING);

    container(row_content)
        .width(Fill)
        .style(alternating_row_style(index.is_multiple_of(2)))
        .into()
}

// ============================================================================
// Main View Function
// ============================================================================

/// Displays the transfers panel
///
/// Shows a list of all transfers (active, queued, paused, failed).
/// Completed transfers are automatically cleared.
pub fn transfers_view<'a>(manager: &'a TransferManager) -> Element<'a, Message> {
    // Title row (centered, using panel_title helper for consistent sizing)
    let title_row = panel_title(t("title-transfers"));

    // Build transfer list
    let transfers: Vec<&Transfer> = manager
        .all()
        .filter(|t| t.status != TransferStatus::Completed)
        .collect();

    let scroll_content: Element<'a, Message> = if transfers.is_empty() {
        // Empty state - just show muted text
        shaped_text(t("transfers-empty"))
            .size(TEXT_SIZE)
            .width(Fill)
            .align_x(Center)
            .style(muted_text_style)
            .into()
    } else {
        // Build transfer rows
        let mut rows = Column::new().spacing(TRANSFER_ITEM_SPACING);

        for (index, transfer) in transfers.iter().enumerate() {
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
