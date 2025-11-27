//! Certificate fingerprint mismatch dialog view

use super::constants::BUTTON_CANCEL;
use super::style::*;
use crate::types::{FingerprintMismatch, Message};
use iced::widget::{Space, button, column, container, row, text};
use iced::{Element, Length};

// UI text constants
const TITLE_FINGERPRINT_MISMATCH: &str = "Certificate Fingerprint Mismatch!";
const LABEL_EXPECTED_FINGERPRINT: &str = "Expected fingerprint:";
const LABEL_RECEIVED_FINGERPRINT: &str = "Received fingerprint:";
const BUTTON_ACCEPT_NEW_CERTIFICATE: &str = "Accept New Certificate";

// Size constants
const TITLE_SIZE: u16 = 20;
const WARNING_SIZE: u16 = 14;
const LABEL_SIZE: u16 = 14;
const BUTTON_SIZE: u16 = 14;
const FINGERPRINT_SIZE: u16 = 12;
const BUTTON_PADDING: u16 = 10;
const DIALOG_SPACING: u16 = 10;
const DIALOG_PADDING: u16 = 20;
const DIALOG_MAX_WIDTH: f32 = 600.0;
const SPACE_AFTER_TITLE: u16 = 15;
const SPACE_AFTER_WARNING: u16 = 15;
const SPACE_AFTER_LABEL: u16 = 5;
const SPACE_BETWEEN_SECTIONS: u16 = 10;
const SPACE_BEFORE_BUTTONS: u16 = 20;

/// Create the fingerprint mismatch warning dialog
pub fn fingerprint_mismatch_dialog<'a>(mismatch: &'a FingerprintMismatch) -> Element<'a, Message> {
    let title = text(TITLE_FINGERPRINT_MISMATCH)
        .size(TITLE_SIZE)
        .width(Length::Fill)
        .center();

    let warning = text(format!(
        "The certificate for '{}' has changed.\n\
        This could indicate a security issue (MITM attack) or the server's certificate was regenerated.\n\n\
        Only accept if you trust the server administrator.",
        mismatch.bookmark_name
    ))
    .size(WARNING_SIZE);

    let expected_label = text(LABEL_EXPECTED_FINGERPRINT).size(LABEL_SIZE);
    let expected_value = text(&mismatch.expected)
        .size(FINGERPRINT_SIZE)
        .font(iced::Font::MONOSPACE);

    let received_label = text(LABEL_RECEIVED_FINGERPRINT).size(LABEL_SIZE);
    let received_value = text(&mismatch.received)
        .size(FINGERPRINT_SIZE)
        .font(iced::Font::MONOSPACE);

    let accept_button = button(
        text(BUTTON_ACCEPT_NEW_CERTIFICATE)
            .size(BUTTON_SIZE)
            .width(Length::Fill)
            .center(),
    )
    .on_press(Message::AcceptNewFingerprint)
    .padding(BUTTON_PADDING)
    .style(primary_button_style());

    let cancel_button = button(text(BUTTON_CANCEL).size(BUTTON_SIZE).width(Length::Fill).center())
        .on_press(Message::CancelFingerprintMismatch)
        .padding(BUTTON_PADDING)
        .style(primary_button_style());

    let button_row = row![accept_button, cancel_button].spacing(DIALOG_SPACING);

    let dialog = column![
        title,
        Space::with_height(SPACE_AFTER_TITLE),
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
