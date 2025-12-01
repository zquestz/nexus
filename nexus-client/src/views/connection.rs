//! Connection form for new server connections

use crate::i18n::t;
use crate::style::{
    BUTTON_PADDING, ELEMENT_SPACING, FORM_MAX_WIDTH, FORM_PADDING, INPUT_PADDING,
    SPACER_SIZE_MEDIUM, SPACER_SIZE_SMALL, TEXT_SIZE, TITLE_SIZE, error_text_style, shaped_text,
    shaped_text_wrapped,
};
use crate::types::{ConnectionFormState, InputId, Message};
use iced::widget::{Space, button, checkbox, column, container, text, text_input};
use iced::{Center, Element, Fill};

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
    let can_connect = !form.server_name.trim().is_empty()
        && !form.server_address.trim().is_empty()
        && !form.port.trim().is_empty();

    // Helper for on_submit - use no-op when form is invalid
    let submit_action = if can_connect {
        Message::ConnectPressed
    } else {
        Message::ServerNameChanged(String::new())
    };

    let title = shaped_text(t("title-connect-to-server"))
        .size(TITLE_SIZE)
        .width(Fill)
        .align_x(Center);

    let server_name_input = text_input(&t("placeholder-server-name"), &form.server_name)
        .on_input(Message::ServerNameChanged)
        .on_submit(submit_action.clone())
        .id(text_input::Id::from(InputId::ServerName))
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE);

    let server_address_input = text_input(&t("placeholder-server-address"), &form.server_address)
        .on_input(Message::ServerAddressChanged)
        .on_submit(submit_action.clone())
        .id(text_input::Id::from(InputId::ServerAddress))
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE);

    let port_input = text_input(&t("placeholder-port"), &form.port)
        .on_input(Message::PortChanged)
        .on_submit(submit_action.clone())
        .id(text_input::Id::from(InputId::Port))
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE);

    let username_input = text_input(&t("placeholder-username-optional"), &form.username)
        .on_input(Message::UsernameChanged)
        .on_submit(submit_action.clone())
        .id(text_input::Id::from(InputId::Username))
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE);

    let password_input = text_input(&t("placeholder-password-optional"), &form.password)
        .on_input(Message::PasswordChanged)
        .on_submit(submit_action)
        .id(text_input::Id::from(InputId::Password))
        .secure(true)
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE);

    let connect_button = if can_connect && !form.is_connecting {
        button(shaped_text(t("button-connect")).size(TEXT_SIZE))
            .on_press(Message::ConnectPressed)
            .padding(BUTTON_PADDING)
    } else {
        button(shaped_text(t("button-connect")).size(TEXT_SIZE)).padding(BUTTON_PADDING)
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
        column_items.push(Space::with_height(SPACER_SIZE_SMALL).into());
    } else {
        column_items.push(Space::with_height(SPACER_SIZE_MEDIUM).into());
    }

    column_items.extend([
        server_name_input.into(),
        server_address_input.into(),
        port_input.into(),
        username_input.into(),
        password_input.into(),
        Space::with_height(SPACER_SIZE_SMALL).into(),
        checkbox(t("label-add-bookmark"), form.add_bookmark)
            .on_toggle(Message::AddBookmarkToggled)
            .size(TEXT_SIZE)
            .text_shaping(text::Shaping::Advanced)
            .into(),
        Space::with_height(SPACER_SIZE_MEDIUM).into(),
        connect_button.into(),
    ]);

    let content = column(column_items)
        .spacing(ELEMENT_SPACING)
        .padding(FORM_PADDING)
        .max_width(FORM_MAX_WIDTH);

    container(content)
        .width(Fill)
        .height(Fill)
        .center(Fill)
        .into()
}
