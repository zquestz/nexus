//! Certificate fingerprint mismatch dialog view

use crate::style::{MONOSPACE_FONT, modal_overlay_style, primary_button_style, shaped_text};
use crate::i18n::t;
use crate::types::{FingerprintMismatch, Message};
use iced::widget::{Space, button, column, container, row};
use iced::{Element, Length};

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
    let title = shaped_text(t("title-fingerprint-mismatch"))
        .size(TITLE_SIZE)
        .width(Length::Fill)
        .center();

    let server_line = shaped_text(format!(
        "{} - [{}]:{}",
        mismatch.bookmark_name, mismatch.server_address, mismatch.server_port
    ))
    .size(TEXT_SIZE);

    let warning = shaped_text(t("fingerprint-warning")).size(TEXT_SIZE);

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
    .padding(BUTTON_PADDING)
    .style(primary_button_style());

    let cancel_button = button(
        shaped_text(t("button-cancel"))
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
