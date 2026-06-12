//! Certificate fingerprint interception dialog view (TLS proxy detection)

use iced::widget::{Space, button, column, row};
use iced::{Element, Length};

use super::fingerprint::format_fingerprint_multiline;
use super::layout::scrollable_modal;
use crate::i18n::t;
use crate::network::FingerprintInterception;
use crate::style::{
    BUTTON_PADDING, CONTENT_MAX_WIDTH, CONTENT_PADDING, ELEMENT_SPACING,
    FINGERPRINT_SPACE_AFTER_LABEL, FINGERPRINT_SPACE_AFTER_SERVER_INFO,
    FINGERPRINT_SPACE_AFTER_TITLE, FINGERPRINT_SPACE_AFTER_WARNING,
    FINGERPRINT_SPACE_BEFORE_BUTTONS, FINGERPRINT_SPACE_BETWEEN_SECTIONS, MONOSPACE_FONT,
    TEXT_SIZE, panel_title, shaped_text, shaped_text_wrapped,
};
use crate::types::Message;

// ============================================================================
// Dialog View
// ============================================================================

/// Create the fingerprint interception warning dialog (TLS proxy detection)
pub fn fingerprint_interception_dialog<'a>(
    interception: &'a FingerprintInterception,
) -> Element<'a, Message> {
    let title = panel_title(t("title-fingerprint-mismatch"));

    // Mirror the stage-1 mismatch dialog: only show the leading "name -" when
    // we actually have a friendly name to display. Empty falls back to host:port.
    let endpoint =
        crate::uri::format_endpoint(&interception.server_address, interception.server_port);
    let server_line_text = if interception.server_name.is_empty() {
        endpoint
    } else {
        format!("{} - {}", interception.server_name, endpoint)
    };
    let server_line = shaped_text(server_line_text).size(TEXT_SIZE);

    let warning = shaped_text_wrapped(t("fingerprint-interception-warning")).size(TEXT_SIZE);

    let tls_label = shaped_text(t("label-tls-fingerprint")).size(TEXT_SIZE);
    let tls_value = shaped_text(format_fingerprint_multiline(&interception.tls_fingerprint))
        .size(TEXT_SIZE)
        .font(MONOSPACE_FONT);

    let server_label = shaped_text(t("label-server-fingerprint")).size(TEXT_SIZE);
    let server_value = shaped_text(format_fingerprint_multiline(
        &interception.server_fingerprint,
    ))
    .size(TEXT_SIZE)
    .font(MONOSPACE_FONT);

    let ok_button = button(
        shaped_text(t("button-ok"))
            .size(TEXT_SIZE)
            .width(Length::Fill)
            .center(),
    )
    .on_press(Message::DismissFingerprintInterception)
    .padding(BUTTON_PADDING)
    .style(button::secondary);

    let button_row = row![Space::new().width(Length::Fill), ok_button].spacing(ELEMENT_SPACING);

    let dialog = column![
        title,
        Space::new().height(FINGERPRINT_SPACE_AFTER_TITLE),
        server_line,
        Space::new().height(FINGERPRINT_SPACE_AFTER_SERVER_INFO),
        warning,
        Space::new().height(FINGERPRINT_SPACE_AFTER_WARNING),
        tls_label,
        Space::new().height(FINGERPRINT_SPACE_AFTER_LABEL),
        tls_value,
        Space::new().height(FINGERPRINT_SPACE_BETWEEN_SECTIONS),
        server_label,
        Space::new().height(FINGERPRINT_SPACE_AFTER_LABEL),
        server_value,
        Space::new().height(FINGERPRINT_SPACE_BEFORE_BUTTONS),
        button_row,
    ]
    .spacing(ELEMENT_SPACING)
    .padding(CONTENT_PADDING)
    .max_width(CONTENT_MAX_WIDTH);

    scrollable_modal(dialog)
}
