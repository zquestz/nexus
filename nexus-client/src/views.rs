//! UI view rendering for the Nexus client

use crate::types::{ChatMessage, ConnectionState, InputId, Message, UserInfo};
use iced::widget::{Column, button, column, container, row, scrollable, text, text_input};
use iced::{Center, Element, Fill};

/// Render the connection screen (login form)
pub fn connection_screen<'a>(
    connection_state: &'a ConnectionState,
    server_address: &'a str,
    port: &'a str,
    username: &'a str,
    password: &'a str,
    connection_error: &'a Option<String>,
) -> Element<'a, Message> {
    let can_connect = !server_address.trim().is_empty() && !port.trim().is_empty();
    
    // Create submit handler that only triggers if fields are valid
    let on_submit = if can_connect {
        Some(Message::ConnectPressed)
    } else {
        None
    };
    let title = text("Nexus BBS Client")
        .size(32)
        .width(Fill)
        .align_x(Center);

    let server_input = if let Some(msg) = on_submit.clone() {
        text_input("Server IPv6 Address", server_address)
            .id(InputId::ServerAddress)
            .on_input(Message::ServerAddressChanged)
            .on_submit(msg)
            .padding(10)
            .size(16)
    } else {
        text_input("Server IPv6 Address", server_address)
            .id(InputId::ServerAddress)
            .on_input(Message::ServerAddressChanged)
            .padding(10)
            .size(16)
    };

    let port_input = if let Some(msg) = on_submit.clone() {
        text_input("Port", port)
            .id(InputId::Port)
            .on_input(Message::PortChanged)
            .on_submit(msg)
            .padding(10)
            .size(16)
    } else {
        text_input("Port", port)
            .id(InputId::Port)
            .on_input(Message::PortChanged)
            .padding(10)
            .size(16)
    };

    let username_input = if let Some(msg) = on_submit.clone() {
        text_input("Username", username)
            .id(InputId::Username)
            .on_input(Message::UsernameChanged)
            .on_submit(msg)
            .padding(10)
            .size(16)
    } else {
        text_input("Username", username)
            .id(InputId::Username)
            .on_input(Message::UsernameChanged)
            .padding(10)
            .size(16)
    };

    let password_input = if let Some(msg) = on_submit {
        text_input("Password", password)
            .id(InputId::Password)
            .on_input(Message::PasswordChanged)
            .on_submit(msg)
            .secure(true)
            .padding(10)
            .size(16)
    } else {
        text_input("Password", password)
            .id(InputId::Password)
            .on_input(Message::PasswordChanged)
            .secure(true)
            .padding(10)
            .size(16)
    };

    let connect_button = if *connection_state == ConnectionState::Connecting {
        button(text("Connecting...")).padding(10)
    } else if can_connect {
        button(text("Connect"))
            .on_press(Message::ConnectPressed)
            .padding(10)
    } else {
        button(text("Connect")).padding(10)
    };

    let mut content = column![
        title,
        text("").size(20),
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
        .padding(20)
        .center(Fill)
        .into()
}

/// Render the main screen (chat interface)
pub fn main_screen<'a>(
    username: &'a str,
    session_id: Option<u32>,
    chat_messages: &'a [ChatMessage],
    online_users: &'a [UserInfo],
    message_input: &'a str,
) -> Element<'a, Message> {
    // Left side: user list
    let user_list_title = text("Online Users").size(20);
    let mut user_list = Column::new().spacing(5).padding(10);
    for user in online_users {
        user_list = user_list.push(
            button(text(format!("{} ({})", user.username, user.session_id)))
                .on_press(Message::RequestUserInfo(user.session_id))
                .width(Fill),
        );
    }
    let user_list_panel = column![
        user_list_title,
        scrollable(user_list).height(Fill),
        button(text("Refresh Users"))
            .on_press(Message::RequestUserList)
            .padding(10)
            .width(Fill),
    ]
    .spacing(10)
    .padding(10)
    .width(250);

    // Right side: chat
    let chat_title = text("Chat - #server").size(20);

    let mut chat_messages_column = Column::new().spacing(5).padding(10);
    for msg in chat_messages {
        let display = if msg.username == "System" {
            text(format!("*** {}", msg.message)).color([0.7, 0.7, 0.7])
        } else if msg.username == "Error" {
            text(format!("Error: {}", msg.message)).color([1.0, 0.0, 0.0])
        } else if msg.username == "Info" {
            text(format!("ℹ {}", msg.message)).color([0.5, 0.8, 1.0])
        } else {
            text(format!(
                "[{}] {}: {}",
                msg.session_id, msg.username, msg.message
            ))
        };
        chat_messages_column = chat_messages_column.push(display);
    }

    let message_input_widget = text_input("Type a message...", message_input)
        .on_input(Message::MessageInputChanged)
        .on_submit(Message::SendMessagePressed)
        .padding(10)
        .size(16);

    let send_button = button(text("Send"))
        .on_press(Message::SendMessagePressed)
        .padding(10);

    let chat_panel = column![
        chat_title,
        scrollable(chat_messages_column).height(Fill),
        row![message_input_widget, send_button,].spacing(10),
    ]
    .spacing(10)
    .padding(10);

    // Top bar with disconnect button
    let top_bar = row![
        text(format!(
            "Connected as {} (session {})",
            username,
            session_id.unwrap_or(0)
        ))
        .size(16),
        button(text("Disconnect"))
            .on_press(Message::Disconnect)
            .padding(10),
    ]
    .spacing(20)
    .padding(10);

    let main_content = row![user_list_panel, chat_panel].spacing(10);

    container(column![top_bar, main_content])
        .width(Fill)
        .height(Fill)
        .into()
}
