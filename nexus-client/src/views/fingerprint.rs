//! Certificate fingerprint mismatch dialog view

use crate::i18n::t;
use crate::style::{
    BUTTON_PADDING, ELEMENT_SPACING, FINGERPRINT_DIALOG_MAX_WIDTH, FINGERPRINT_SPACE_AFTER_LABEL,
    FINGERPRINT_SPACE_AFTER_SERVER_INFO, FINGERPRINT_SPACE_AFTER_TITLE,
    FINGERPRINT_SPACE_AFTER_WARNING, FINGERPRINT_SPACE_BEFORE_BUTTONS,
    FINGERPRINT_SPACE_BETWEEN_SECTIONS, FORM_PADDING, MONOSPACE_FONT, TEXT_SIZE, TITLE_SIZE,
    modal_overlay_style, shaped_text, shaped_text_wrapped,
};
use crate::types::{FingerprintMismatch, Message};
use iced::widget::{Space, button, column, container, row};
use iced::{Element, Length};

// ============================================================================
// Helper Functions
// ============================================================================

/// Format a colon-separated fingerprint into two lines for readability
fn format_fingerprint_multiline(fingerprint: &str) -> String {
    let parts: Vec<&str> = fingerprint.split(':').collect();
    let mid = parts.len() / 2;
    format!("{}\n{}", parts[..mid].join(":"), parts[mid..].join(":"))
}

// ============================================================================
// Dialog View
// ============================================================================

/// Create the fingerprint mismatch warning dialog
pub fn fingerprint_mismatch_dialog<'a>(mismatch: &'a FingerprintMismatch) -> Element<'a, Message> {
    let title = shaped_text(t("title-fingerprint-mismatch"))
        .size(TITLE_SIZE)
        .width(Length::Fill)
        .center();

    let server_line = shaped_text(format!(
        "{} - [{}]:{}",
        mismatch.bookmark_name, mismatch.server_address, mismatch.server_port
    ))
    .size(TEXT_SIZE);

    let warning = shaped_text_wrapped(t("fingerprint-warning")).size(TEXT_SIZE);

    let expected_label = shaped_text(t("label-expected-fingerprint")).size(TEXT_SIZE);
    let expected_value = shaped_text(format_fingerprint_multiline(&mismatch.expected))
        .size(TEXT_SIZE)
        .font(MONOSPACE_FONT);

    let received_label = shaped_text(t("label-received-fingerprint")).size(TEXT_SIZE);
    let received_value = shaped_text(format_fingerprint_multiline(&mismatch.received))
        .size(TEXT_SIZE)
        .font(MONOSPACE_FONT);

    let accept_button = button(
        shaped_text(t("button-accept-new-certificate"))
            .size(TEXT_SIZE)
            .width(Length::Fill)
            .center(),
    )
    .on_press(Message::AcceptNewFingerprint)
    .padding(BUTTON_PADDING);

    let cancel_button = button(
        shaped_text(t("button-cancel"))
            .size(TEXT_SIZE)
            .width(Length::Fill)
            .center(),
    )
    .on_press(Message::CancelFingerprintMismatch)
    .padding(BUTTON_PADDING);

    let button_row = row![accept_button, cancel_button].spacing(ELEMENT_SPACING);

    let dialog = column![
        title,
        Space::with_height(FINGERPRINT_SPACE_AFTER_TITLE),
        server_line,
        Space::with_height(FINGERPRINT_SPACE_AFTER_SERVER_INFO),
        warning,
        Space::with_height(FINGERPRINT_SPACE_AFTER_WARNING),
        expected_label,
        Space::with_height(FINGERPRINT_SPACE_AFTER_LABEL),
        expected_value,
        Space::with_height(FINGERPRINT_SPACE_BETWEEN_SECTIONS),
        received_label,
        Space::with_height(FINGERPRINT_SPACE_AFTER_LABEL),
        received_value,
        Space::with_height(FINGERPRINT_SPACE_BEFORE_BUTTONS),
        button_row,
    ]
    .spacing(ELEMENT_SPACING)
    .padding(FORM_PADDING)
    .max_width(FINGERPRINT_DIALOG_MAX_WIDTH);

    // Center the dialog and add dark overlay background
    container(dialog)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .style(modal_overlay_style)
        .into()
}
