//! Connection form for new server connections

use iced::widget::button as btn;
use iced::widget::{Id, Space, button, checkbox, column, row, text, text_input};
use iced::{Center, Element, Fill};
use iced_aw::NumberInput;

use super::layout::scrollable_panel;
use crate::i18n::t;
use crate::style::{
    BUTTON_PADDING, CONTENT_MAX_WIDTH, CONTENT_PADDING, ELEMENT_SPACING, INPUT_PADDING,
    MONOSPACE_FONT, SPACER_SIZE_MEDIUM, SPACER_SIZE_SMALL, TEXT_SIZE, error_text_style,
    panel_title, shaped_text, shaped_text_wrapped,
};
use crate::types::{ConnectionFormState, InputId, Message};

// ============================================================================
// Connection Form View
// ============================================================================

/// Displays connection form with server details and credentials
///
/// Shows validated input fields for connecting to a new server. Server name is
/// optional, but address, port, and username are required. Password can be empty
/// for servers that don't require authentication.
pub fn connection_form_view(form: &ConnectionFormState) -> Element<'_, Message> {
    // Validate required fields (username and password are optional)
    // Port is always valid since it's a u16
    let can_connect = !form.server_name.trim().is_empty() && !form.server_address.trim().is_empty();

    let title = panel_title(t("title-connect-to-server"));

    let server_name_input = text_input(&t("placeholder-server-name"), &form.server_name)
        .on_input(Message::ServerNameChanged)
        .on_submit(Message::ConnectPressed)
        .id(Id::from(InputId::ServerName))
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE);

    let server_address_input = text_input(&t("placeholder-server-address"), &form.server_address)
        .on_input(Message::ServerAddressChanged)
        .on_submit(Message::ConnectPressed)
        .id(Id::from(InputId::ServerAddress))
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE);

    let port_label = shaped_text(t("label-port")).size(TEXT_SIZE);
    let port_number_input: Element<'_, Message> =
        NumberInput::new(&form.port, 1..=65535, Message::PortChanged)
            .id(Id::from(InputId::Port))
            .padding(INPUT_PADDING)
            .into();
    let port_input = row![port_label, port_number_input]
        .spacing(ELEMENT_SPACING)
        .align_y(Center);

    let username_input = text_input(&t("placeholder-username-optional"), &form.username)
        .on_input(Message::UsernameChanged)
        .on_submit(Message::ConnectPressed)
        .id(Id::from(InputId::Username))
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE);

    let password_input = text_input(&t("placeholder-password-optional"), &form.password)
        .on_input(Message::PasswordChanged)
        .on_submit(Message::ConnectPressed)
        .id(Id::from(InputId::Password))
        .secure(true)
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE);

    let nickname_input = text_input(&t("placeholder-nickname-optional"), &form.nickname)
        .on_input(Message::NicknameChanged)
        .on_submit(Message::ConnectPressed)
        .id(Id::from(InputId::Nickname))
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE);

    let fingerprint_input = text_input(&t("placeholder-fingerprint"), &form.fingerprint)
        .on_input(Message::FingerprintChanged)
        .on_submit(Message::ConnectPressed)
        .id(Id::from(InputId::Fingerprint))
        .font(MONOSPACE_FONT)
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE);

    let connect_button = if can_connect && !form.is_connecting {
        button(shaped_text(t("button-connect")).size(TEXT_SIZE))
            .on_press(Message::ConnectPressed)
            .padding(BUTTON_PADDING)
    } else {
        button(shaped_text(t("button-connect")).size(TEXT_SIZE)).padding(BUTTON_PADDING)
    };

    // Cancel button is gated on `connect_origin` — only shown when the form
    // was opened from somewhere we can return to (tracker click, server-list
    // "+" entrypoint, etc.). The default-state form (app boot, post-disconnect
    // of last connection) has `connect_origin = None` and renders Connect alone.
    let cancel_button: Option<Element<'_, Message>> = if form.connect_origin.is_some() {
        Some(
            button(shaped_text(t("button-cancel")).size(TEXT_SIZE))
                .on_press(Message::ConnectionFormCancel)
                .padding(BUTTON_PADDING)
                .style(btn::secondary)
                .into(),
        )
    } else {
        None
    };

    let mut column_items: Vec<Element<'_, Message>> = vec![title.into()];

    // Show error if present (at top for visibility)
    if let Some(error) = &form.error {
        column_items.push(
            shaped_text_wrapped(error)
                .size(TEXT_SIZE)
                .width(Fill)
                .align_x(Center)
                .style(error_text_style)
                .into(),
        );
        column_items.push(Space::new().height(SPACER_SIZE_SMALL).into());
    } else {
        column_items.push(Space::new().height(SPACER_SIZE_MEDIUM).into());
    }

    column_items.extend([
        server_name_input.into(),
        server_address_input.into(),
        port_input.into(),
        username_input.into(),
        password_input.into(),
        nickname_input.into(),
        fingerprint_input.into(),
        Space::new().height(SPACER_SIZE_SMALL).into(),
        checkbox(form.add_bookmark)
            .label(t("label-add-bookmark"))
            .on_toggle(Message::AddBookmarkToggled)
            .size(TEXT_SIZE)
            .text_shaping(text::Shaping::Advanced)
            .into(),
        Space::new().height(SPACER_SIZE_MEDIUM).into(),
    ]);

    // Button row: Cancel (left, when origin present) + Connect (right).
    // Project convention: [Cancel] [Save / Connect].
    let button_row = if let Some(cancel) = cancel_button {
        row![Space::new().width(Fill), cancel, connect_button]
    } else {
        row![Space::new().width(Fill), connect_button]
    };
    column_items.push(button_row.spacing(ELEMENT_SPACING).into());

    let content = column(column_items)
        .spacing(ELEMENT_SPACING)
        .padding(CONTENT_PADDING)
        .max_width(CONTENT_MAX_WIDTH);

    scrollable_panel(content)
}
