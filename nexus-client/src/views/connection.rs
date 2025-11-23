//! Connection form for new server connections

use crate::types::{InputId, Message};
use iced::widget::{button, column, container, text, text_input};
use iced::{Center, Element, Fill};

/// Displays connection form with server details and credentials
pub fn connection_form_view<'a>(
    server_name: &'a str,
    server_address: &'a str,
    port: &'a str,
    username: &'a str,
    password: &'a str,
    connection_error: &'a Option<String>,
) -> Element<'a, Message> {
    let can_connect =
        !server_address.trim().is_empty() && !port.trim().is_empty() && !username.trim().is_empty();

    let title = text("Connect to Server")
        .size(20)
        .width(Fill)
        .align_x(Center);

    let name_input = text_input("Server Name (optional)", server_name)
        .on_input(Message::ServerNameChanged)
        .on_submit(if can_connect {
            Message::ConnectPressed
        } else {
            Message::ServerNameChanged(String::new())
        })
        .id(text_input::Id::from(InputId::ServerName))
        .padding(8)
        .size(14);

    let server_input = text_input("Server IPv6 Address", server_address)
        .on_input(Message::ServerAddressChanged)
        .on_submit(if can_connect {
            Message::ConnectPressed
        } else {
            Message::ServerAddressChanged(String::new())
        })
        .id(text_input::Id::from(InputId::ServerAddress))
        .padding(8)
        .size(14);

    let port_input = text_input("Port", port)
        .on_input(Message::PortChanged)
        .on_submit(if can_connect {
            Message::ConnectPressed
        } else {
            Message::PortChanged(String::new())
        })
        .id(text_input::Id::from(InputId::Port))
        .padding(8)
        .size(14);

    let username_input = text_input("Username", username)
        .on_input(Message::UsernameChanged)
        .on_submit(if can_connect {
            Message::ConnectPressed
        } else {
            Message::UsernameChanged(String::new())
        })
        .id(text_input::Id::from(InputId::Username))
        .padding(8)
        .size(14);

    let password_input = text_input("Password", password)
        .on_input(Message::PasswordChanged)
        .on_submit(if can_connect {
            Message::ConnectPressed
        } else {
            Message::PasswordChanged(String::new())
        })
        .id(text_input::Id::from(InputId::Password))
        .secure(true)
        .padding(8)
        .size(14);

    let connect_button = if can_connect {
        button(text("Connect").size(14))
            .on_press(Message::ConnectPressed)
            .padding(10)
    } else {
        button(text("Connect").size(14)).padding(10)
    };

    let mut content = column![
        title,
        text("").size(15),
        name_input,
        server_input,
        port_input,
        username_input,
        password_input,
        text("").size(10),
        connect_button,
    ]
    .spacing(10)
    .padding(20)
    .max_width(400);

    if let Some(error) = connection_error {
        content = content.push(text("").size(10));
        content = content.push(text(error).size(14).color([1.0, 0.0, 0.0]));
    }

    container(content)
        .width(Fill)
        .height(Fill)
        .center(Fill)
        .into()
}