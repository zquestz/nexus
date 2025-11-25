//! Connection form for new server connections

use super::style::{
    BUTTON_PADDING, ELEMENT_SPACING, FORM_MAX_WIDTH, FORM_PADDING, INPUT_PADDING,
    SPACER_SIZE_LARGE, SPACER_SIZE_MEDIUM, TEXT_SIZE, TITLE_SIZE, error_message_color,
    primary_button_style, primary_text_input_style,
};
use crate::types::{InputId, Message};
use iced::widget::{button, column, container, text, text_input};
use iced::{Center, Element, Fill};

/// Displays connection form with server details and credentials
///
/// Shows validated input fields for connecting to a new server. Server name is
/// optional, but address, port, and username are required. Password can be empty
/// for servers that don't require authentication.
pub fn connection_form_view<'a>(
    server_name: &'a str,
    server_address: &'a str,
    port: &'a str,
    username: &'a str,
    password: &'a str,
    connection_error: &'a Option<String>,
    is_connecting: bool,
) -> Element<'a, Message> {
    // Validate required fields (server name and password are optional)
    let can_connect =
        !server_address.trim().is_empty() && !port.trim().is_empty() && !username.trim().is_empty();

    // Helper for on_submit - avoid action when form is invalid
    let submit_action = if can_connect {
        Message::ConnectPressed
    } else {
        Message::ServerNameChanged(String::new())
    };

    let title = text("Connect to Server")
        .size(TITLE_SIZE)
        .width(Fill)
        .align_x(Center);

    let name_input = text_input("Server Name (optional)", server_name)
        .on_input(Message::ServerNameChanged)
        .on_submit(submit_action.clone())
        .id(text_input::Id::from(InputId::ServerName))
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE)
        .style(primary_text_input_style());

    let server_input = text_input("Server IPv6 Address", server_address)
        .on_input(Message::ServerAddressChanged)
        .on_submit(submit_action.clone())
        .id(text_input::Id::from(InputId::ServerAddress))
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE)
        .style(primary_text_input_style());

    let port_input = text_input("Port", port)
        .on_input(Message::PortChanged)
        .on_submit(submit_action.clone())
        .id(text_input::Id::from(InputId::Port))
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE)
        .style(primary_text_input_style());

    let username_input = text_input("Username", username)
        .on_input(Message::UsernameChanged)
        .on_submit(submit_action.clone())
        .id(text_input::Id::from(InputId::Username))
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE)
        .style(primary_text_input_style());

    let password_input = text_input("Password", password)
        .on_input(Message::PasswordChanged)
        .on_submit(submit_action)
        .id(text_input::Id::from(InputId::Password))
        .secure(true)
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE)
        .style(primary_text_input_style());

    let connect_button = if can_connect && !is_connecting {
        button(text("Connect").size(TEXT_SIZE))
            .on_press(Message::ConnectPressed)
            .padding(BUTTON_PADDING)
            .style(primary_button_style())
    } else {
        button(text("Connect").size(TEXT_SIZE))
            .padding(BUTTON_PADDING)
            .style(primary_button_style())
    };

    let mut column_items = vec![title.into(), text("").size(SPACER_SIZE_LARGE).into()];

    // Show error if present (at top for visibility)
    if let Some(error) = connection_error {
        column_items.push(
            text(error)
                .size(TEXT_SIZE)
                .color(error_message_color())
                .into(),
        );
        column_items.push(text("").size(SPACER_SIZE_MEDIUM).into());
    }

    column_items.extend(vec![
        name_input.into(),
        server_input.into(),
        port_input.into(),
        username_input.into(),
        password_input.into(),
        text("").size(SPACER_SIZE_MEDIUM).into(),
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
