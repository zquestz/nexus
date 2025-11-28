//! Connection form for new server connections

use super::constants::{PLACEHOLDER_PASSWORD, PLACEHOLDER_PORT, PLACEHOLDER_USERNAME};
use super::style::{
    BUTTON_PADDING, ELEMENT_SPACING, FORM_MAX_WIDTH, FORM_PADDING, INPUT_PADDING,
    SPACER_SIZE_LARGE, SPACER_SIZE_MEDIUM, TEXT_SIZE, TITLE_SIZE, error_message_color,
    primary_button_style, primary_text_input_style, shaped_text,
};
use crate::types::{InputId, Message};
use iced::widget::{button, column, container, text_input};
use iced::{Center, Element, Fill};

/// Connection form input data
pub struct ConnectionFormData<'a> {
    pub server_name: &'a str,
    pub server_address: &'a str,
    pub port: &'a str,
    pub username: &'a str,
    pub password: &'a str,
    pub connection_error: &'a Option<String>,
    pub is_connecting: bool,
}

// UI text constants
const TITLE_CONNECT_TO_SERVER: &str = "Connect to Server";
const PLACEHOLDER_SERVER_NAME_OPTIONAL: &str = "Server Name (optional)";
const PLACEHOLDER_SERVER_IPV6_ADDRESS: &str = "Server IPv6 Address";
const BUTTON_CONNECT: &str = "Connect";

/// Displays connection form with server details and credentials
///
/// Shows validated input fields for connecting to a new server. Server name is
/// optional, but address, port, and username are required. Password can be empty
/// for servers that don't require authentication.
pub fn connection_form_view<'a>(data: ConnectionFormData<'a>) -> Element<'a, Message> {
    // Validate required fields (server name and password are optional)
    let can_connect = !data.server_address.trim().is_empty()
        && !data.port.trim().is_empty()
        && !data.username.trim().is_empty();

    // Helper for on_submit - avoid action when form is invalid
    let submit_action = if can_connect {
        Message::ConnectPressed
    } else {
        Message::ServerNameChanged(String::new())
    };

    let title = shaped_text(TITLE_CONNECT_TO_SERVER)
        .size(TITLE_SIZE)
        .width(Fill)
        .align_x(Center);

    let server_name_input = text_input(PLACEHOLDER_SERVER_NAME_OPTIONAL, data.server_name)
        .on_input(Message::ServerNameChanged)
        .on_submit(submit_action.clone())
        .id(text_input::Id::from(InputId::ServerName))
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE)
        .style(primary_text_input_style());

    let server_address_input = text_input(PLACEHOLDER_SERVER_IPV6_ADDRESS, data.server_address)
        .on_input(Message::ServerAddressChanged)
        .on_submit(submit_action.clone())
        .id(text_input::Id::from(InputId::ServerAddress))
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE)
        .style(primary_text_input_style());

    let port_input = text_input(PLACEHOLDER_PORT, data.port)
        .on_input(Message::PortChanged)
        .on_submit(submit_action.clone())
        .id(text_input::Id::from(InputId::Port))
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE)
        .style(primary_text_input_style());

    let username_input = text_input(PLACEHOLDER_USERNAME, data.username)
        .on_input(Message::UsernameChanged)
        .on_submit(submit_action.clone())
        .id(text_input::Id::from(InputId::Username))
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE)
        .style(primary_text_input_style());

    let password_input = text_input(PLACEHOLDER_PASSWORD, data.password)
        .on_input(Message::PasswordChanged)
        .on_submit(submit_action.clone())
        .id(text_input::Id::from(InputId::Password))
        .secure(true)
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE)
        .style(primary_text_input_style());

    let connect_button = if can_connect && !data.is_connecting {
        button(shaped_text(BUTTON_CONNECT).size(TEXT_SIZE))
            .on_press(Message::ConnectPressed)
            .padding(BUTTON_PADDING)
            .style(primary_button_style())
    } else {
        button(shaped_text(BUTTON_CONNECT).size(TEXT_SIZE))
            .padding(BUTTON_PADDING)
            .style(primary_button_style())
    };

    let mut column_items = vec![title.into(), shaped_text("").size(SPACER_SIZE_LARGE).into()];

    // Show error if present (at top for visibility)
    if let Some(error) = data.connection_error {
        column_items.push(
            shaped_text(error)
                .size(TEXT_SIZE)
                .color(error_message_color())
                .into(),
        );
        column_items.push(shaped_text("").size(SPACER_SIZE_MEDIUM).into());
    }

    column_items.extend(vec![
        server_name_input.into(),
        server_address_input.into(),
        port_input.into(),
        username_input.into(),
        password_input.into(),
        shaped_text("").size(SPACER_SIZE_MEDIUM).into(),
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
