//! Certificate fingerprint mismatch dialog view

use super::constants::BUTTON_CANCEL;
use super::style::*;
use crate::types::{FingerprintMismatch, Message};
use iced::widget::{Space, button, column, container, row};
use iced::{Element, Length};

// UI text constants
const TITLE_FINGERPRINT_MISMATCH: &str = "Certificate Fingerprint Mismatch!";
const WARNING_TEXT: &str = "This could indicate a security issue (MITM attack) or the server's certificate was regenerated. Only accept if you trust the server administrator.";
const LABEL_EXPECTED_FINGERPRINT: &str = "Expected fingerprint:";
const LABEL_RECEIVED_FINGERPRINT: &str = "Received fingerprint:";
const BUTTON_ACCEPT_NEW_CERTIFICATE: &str = "Accept New Certificate";

// Size constants
const TITLE_SIZE: u16 = 20;
const TEXT_SIZE: u16 = 14;
const BUTTON_PADDING: u16 = 10;
const DIALOG_SPACING: u16 = 10;
const DIALOG_PADDING: u16 = 20;
const DIALOG_MAX_WIDTH: f32 = 600.0;
const SPACE_AFTER_TITLE: u16 = 10;
const SPACE_AFTER_SERVER_INFO: u16 = 10;
const SPACE_AFTER_WARNING: u16 = 10;
const SPACE_AFTER_LABEL: u16 = 0;
const SPACE_BETWEEN_SECTIONS: u16 = 8;
const SPACE_BEFORE_BUTTONS: u16 = 10;

/// Format a colon-separated fingerprint into two lines for readability
fn format_fingerprint_multiline(fingerprint: &str) -> String {
    let parts: Vec<&str> = fingerprint.split(':').collect();
    let mid = parts.len() / 2;
    format!("{}\n{}", parts[..mid].join(":"), parts[mid..].join(":"))
}

/// Create the fingerprint mismatch warning dialog
pub fn fingerprint_mismatch_dialog<'a>(mismatch: &'a FingerprintMismatch) -> Element<'a, Message> {
    let title = shaped_text(TITLE_FINGERPRINT_MISMATCH)
        .size(TITLE_SIZE)
        .width(Length::Fill)
        .center();

    let server_line = shaped_text(format!(
        "{} - [{}]:{}",
        mismatch.bookmark_name, mismatch.server_address, mismatch.server_port
    ))
    .size(TEXT_SIZE);

    let warning = shaped_text(WARNING_TEXT).size(TEXT_SIZE);

    let expected_label = shaped_text(LABEL_EXPECTED_FINGERPRINT).size(TEXT_SIZE);
    let expected_value = shaped_text(format_fingerprint_multiline(&mismatch.expected))
        .size(TEXT_SIZE)
        .font(iced::Font::MONOSPACE);

    let received_label = shaped_text(LABEL_RECEIVED_FINGERPRINT).size(TEXT_SIZE);
    let received_value = shaped_text(format_fingerprint_multiline(&mismatch.received))
        .size(TEXT_SIZE)
        .font(iced::Font::MONOSPACE);

    let accept_button = button(
        shaped_text(BUTTON_ACCEPT_NEW_CERTIFICATE)
            .size(TEXT_SIZE)
            .width(Length::Fill)
            .center(),
    )
    .on_press(Message::AcceptNewFingerprint)
    .padding(BUTTON_PADDING)
    .style(primary_button_style());

    let cancel_button = button(
        shaped_text(BUTTON_CANCEL)
            .size(TEXT_SIZE)
            .width(Length::Fill)
            .center(),
    )
    .on_press(Message::CancelFingerprintMismatch)
    .padding(BUTTON_PADDING)
    .style(primary_button_style());

    let button_row = row![accept_button, cancel_button].spacing(DIALOG_SPACING);

    let dialog = column![
        title,
        Space::with_height(SPACE_AFTER_TITLE),
        server_line,
        Space::with_height(SPACE_AFTER_SERVER_INFO),
        warning,
        Space::with_height(SPACE_AFTER_WARNING),
        expected_label,
        Space::with_height(SPACE_AFTER_LABEL),
        expected_value,
        Space::with_height(SPACE_BETWEEN_SECTIONS),
        received_label,
        Space::with_height(SPACE_AFTER_LABEL),
        received_value,
        Space::with_height(SPACE_BEFORE_BUTTONS),
        button_row,
    ]
    .spacing(DIALOG_SPACING)
    .padding(DIALOG_PADDING)
    .max_width(DIALOG_MAX_WIDTH);

    // Center the dialog and add dark overlay background
    let dialog_container = container(dialog)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .style(modal_overlay_style);

    dialog_container.into()
}
