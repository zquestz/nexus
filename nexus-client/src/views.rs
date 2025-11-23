//! UI view rendering for the Nexus client - Multi-server layout

use crate::icons;
use crate::types::{BookmarkEditMode, InputId, Message, ScrollableId, ServerBookmark, ServerConnection, UserInfo};
use iced::widget::{Column, button, checkbox, column, container, row, scrollable, text, text_input};
use iced::{Center, Element, Fill};
use std::collections::HashMap;

/// Main application layout with toolbar, server list, content, and user list
pub fn main_layout<'a>(
    bookmarks: &'a [ServerBookmark],
    connections: &'a HashMap<usize, ServerConnection>,
    active_connection: Option<usize>,
    server_name: &'a str,
    server_address: &'a str,
    port: &'a str,
    username: &'a str,
    password: &'a str,
    connection_error: &'a Option<String>,
    bookmark_edit_mode: &'a BookmarkEditMode,
    bookmark_name: &'a str,
    bookmark_address: &'a str,
    bookmark_port: &'a str,
    bookmark_username: &'a str,
    bookmark_password: &'a str,
    message_input: &'a str,
    admin_username: &'a str,
    admin_password: &'a str,
    admin_is_admin: bool,
    admin_permissions: &'a [(String, bool)],
    delete_username: &'a str,
    show_bookmarks: bool,
    show_userlist: bool,
    show_add_user: bool,
    show_delete_user: bool,
) -> Element<'a, Message> {
    // Top toolbar
    let show_bookmarks_copy = show_bookmarks;
    let show_userlist_copy = show_userlist;
    
    let toolbar = container(
        row![
            text("Nexus BBS").size(16),
            button(text(if show_bookmarks { "[<]" } else { "[>]" }).size(12))
                .on_press(Message::ToggleBookmarks)
                .padding(8)
                .style(move |theme, status| {
                    let mut style = button::primary(theme, status);
                    if !show_bookmarks_copy {
                        style.background = Some(iced::Background::Color(iced::Color::from_rgb(0.4, 0.4, 0.4)));
                    }
                    style
                }),
            button(text(if show_userlist { "[>]" } else { "[<]" }).size(12))
                .on_press(Message::ToggleUserlist)
                .padding(8)
                .style(move |theme, status| {
                    let mut style = button::primary(theme, status);
                    if !show_userlist_copy {
                        style.background = Some(iced::Background::Color(iced::Color::from_rgb(0.4, 0.4, 0.4)));
                    }
                    style
                }),
            button(text(format!("{} User Create", icons::ICON_ADD)).size(12))
                .on_press(Message::ToggleAddUser)
                .padding(8),
            button(text(format!("{} User Delete", icons::ICON_DELETE)).size(12))
                .on_press(Message::ToggleDeleteUser)
                .padding(8),
        ]
        .spacing(10)
        .padding(8)
        .align_y(Center)
    )
    .width(Fill)
    .style(|_theme| {
        container::Style {
            background: Some(iced::Background::Color(iced::Color::from_rgb(0.15, 0.15, 0.15))),
            ..Default::default()
        }
    });

    // Left panel: Server list
    let server_list = server_list_panel(bookmarks, connections, active_connection);

    // Middle panel: Main content (bookmark editor, connection form, or active server view)
    let main_content = if *bookmark_edit_mode != BookmarkEditMode::None {
        bookmark_edit_view(
            bookmark_edit_mode,
            bookmark_name,
            bookmark_address,
            bookmark_port,
            bookmark_username,
            bookmark_password,
        )
    } else if let Some(conn_id) = active_connection {
        if let Some(conn) = connections.get(&conn_id) {
            server_content_view(conn, message_input, admin_username, admin_password, admin_is_admin, admin_permissions, delete_username, show_add_user, show_delete_user)
        } else {
            empty_content_view()
        }
    } else {
        connection_form_view(server_name, server_address, port, username, password, connection_error)
    };

    // Right panel: User list (show if enabled, display empty state if no active connection)
    let user_list = if show_userlist {
        if let Some(conn_id) = active_connection {
            if let Some(conn) = connections.get(&conn_id) {
                user_list_panel(&conn.online_users, conn_id)
            } else {
                user_list_panel(&[], 0) // Empty user list
            }
        } else {
            user_list_panel(&[], 0) // Empty user list
        }
    } else {
        container(text("")).width(0).into()
    };

    // Main layout: Toolbar at top, then three-column layout
    let main_row = if show_bookmarks {
        row![
            server_list,
            main_content,
            user_list,
        ]
        .spacing(0)
    } else {
        row![
            main_content,
            user_list,
        ]
        .spacing(0)
    };

    let layout = column![
        toolbar,
        main_row,
    ]
    .spacing(0);

    container(layout)
        .width(Fill)
        .height(Fill)
        .into()
}

/// Server list panel (left side)
fn server_list_panel<'a>(
    bookmarks: &'a [ServerBookmark],
    connections: &'a HashMap<usize, ServerConnection>,
    active_connection: Option<usize>,
) -> Element<'a, Message> {
    let mut main_column = Column::new().spacing(0);
    
    // === CONNECTED SERVERS SECTION ===
    let connected_title = text("Connected").size(14).color([0.7, 0.7, 0.7]);
    let mut connected_column = Column::new().spacing(3).padding(5);
    
    if connections.is_empty() {
        connected_column = connected_column.push(
            text("No connections")
                .size(11)
                .color([0.4, 0.4, 0.4])
        );
    } else {
        // Sort connections by connection_id for consistent ordering
        let mut conn_list: Vec<_> = connections.iter().collect();
        conn_list.sort_by_key(|(id, _)| **id);
        
        for (conn_id, conn) in conn_list {
            let is_active = active_connection == Some(*conn_id);
            
            let display = format!("{} {}", icons::ICON_CONNECTED, conn.display_name);
            let mut btn = button(text(display).size(13))
                .width(Fill)
                .padding(6);
            
            if is_active {
                btn = btn.style(|theme, status| {
                    button::Style {
                        background: Some(iced::Background::Color(iced::Color::from_rgb(0.3, 0.4, 0.5))),
                        text_color: iced::Color::WHITE,
                        ..button::primary(theme, status)
                    }
                });
            }
            
            btn = btn.on_press(Message::SwitchToConnection(*conn_id));
            
            let server_row = row![
                btn,
                button(text("X").size(11))
                    .on_press(Message::DisconnectFromServer(*conn_id))
                    .padding(4),
            ]
            .spacing(3);
            
            connected_column = connected_column.push(server_row);
        }
    }
    
    let connected_section = column![
        connected_title,
        connected_column,
    ]
    .spacing(5)
    .padding(10);
    
    main_column = main_column.push(connected_section);
    
    // Separator line
    let separator = container(text(""))
        .width(Fill)
        .height(1)
        .style(|_theme| {
            container::Style {
                background: Some(iced::Background::Color(iced::Color::from_rgb(0.3, 0.3, 0.3))),
                ..Default::default()
            }
        });
    main_column = main_column.push(separator);
    
    // === BOOKMARKS SECTION ===
    let bookmarks_title = text("Bookmarks").size(14).color([0.7, 0.7, 0.7]);
    let mut bookmarks_column = Column::new().spacing(3).padding(5);
    
    if bookmarks.is_empty() {
        bookmarks_column = bookmarks_column.push(
            text("No bookmarks")
                .size(11)
                .color([0.4, 0.4, 0.4])
        );
    } else {
        for (index, bookmark) in bookmarks.iter().enumerate() {
            // Check if this bookmark is currently connected
            let connected_to_bookmark = connections.values()
                .any(|conn| conn.bookmark_index == Some(index));
            
            let bookmark_name = if connected_to_bookmark {
                format!("{} {}", icons::ICON_CONNECTED, bookmark.name)
            } else {
                format!("{} {}", icons::ICON_DISCONNECTED, bookmark.name)
            };
            
            let btn = button(text(bookmark_name).size(13))
                .width(Fill)
                .padding(6)
                .on_press(if connected_to_bookmark {
                    // Find the connection_id for this bookmark and switch to it
                    let conn_id = connections.iter()
                        .find(|(_, conn)| conn.bookmark_index == Some(index))
                        .map(|(id, _)| *id)
                        .unwrap_or(0);
                    Message::SwitchToConnection(conn_id)
                } else {
                    Message::ConnectToBookmark(index)
                });
            
            let server_row = row![
                btn,
                button(text("Edit").size(11))
                    .on_press(Message::ShowEditBookmark(index))
                    .padding(4),
                button(text("Del").size(11))
                    .on_press(Message::DeleteBookmark(index))
                    .padding(4),
            ]
            .spacing(3);
            
            bookmarks_column = bookmarks_column.push(server_row);
        }
    }
    
    let add_btn = button(text("+ Add Bookmark").size(12))
        .on_press(Message::ShowAddBookmark)
        .padding(8)
        .width(Fill);
    
    let bookmarks_section = column![
        bookmarks_title,
        scrollable(bookmarks_column).height(Fill),
        add_btn,
    ]
    .spacing(5)
    .padding(10);
    
    main_column = main_column.push(bookmarks_section);
    
    // Wrap in container with styling
    container(main_column)
        .height(Fill)
        .width(220)
        .style(|_theme| {
            container::Style {
                background: Some(iced::Background::Color(iced::Color::from_rgb(0.12, 0.12, 0.12))),
                border: iced::Border {
                    color: iced::Color::from_rgb(0.2, 0.2, 0.2),
                    width: 1.0,
                    ..Default::default()
                },
                ..Default::default()
            }
        })
        .into()
}

/// Connection form view (middle panel when no server active)
fn connection_form_view<'a>(
    server_name: &'a str,
    server_address: &'a str,
    port: &'a str,
    username: &'a str,
    password: &'a str,
    connection_error: &'a Option<String>,
) -> Element<'a, Message> {
    let can_connect = !server_address.trim().is_empty() 
        && !port.trim().is_empty() 
        && !username.trim().is_empty();
    
    let title = text("Connect to Server")
        .size(20)
        .width(Fill)
        .align_x(Center);

    let name_input = text_input("Server Name (optional)", server_name)
        .on_input(Message::ServerNameChanged)
        .on_submit(if can_connect { Message::ConnectPressed } else { Message::ServerNameChanged(String::new()) })
        .id(text_input::Id::from(InputId::ServerName))
        .padding(8)
        .size(14);

    let server_input = text_input("Server IPv6 Address", server_address)
        .on_input(Message::ServerAddressChanged)
        .on_submit(if can_connect { Message::ConnectPressed } else { Message::ServerAddressChanged(String::new()) })
        .id(text_input::Id::from(InputId::ServerAddress))
        .padding(8)
        .size(14);

    let port_input = text_input("Port", port)
        .on_input(Message::PortChanged)
        .on_submit(if can_connect { Message::ConnectPressed } else { Message::PortChanged(String::new()) })
        .id(text_input::Id::from(InputId::Port))
        .padding(8)
        .size(14);

    let username_input = text_input("Username", username)
        .on_input(Message::UsernameChanged)
        .on_submit(if can_connect { Message::ConnectPressed } else { Message::UsernameChanged(String::new()) })
        .id(text_input::Id::from(InputId::Username))
        .padding(8)
        .size(14);

    let password_input = text_input("Password", password)
        .on_input(Message::PasswordChanged)
        .on_submit(if can_connect { Message::ConnectPressed } else { Message::PasswordChanged(String::new()) })
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
        .padding(20)
        .center(Fill)
        .into()
}

/// Empty content view (when no server selected)
fn empty_content_view<'a>() -> Element<'a, Message> {
    container(
        text("Select a server from the list")
            .size(16)
            .color([0.5, 0.5, 0.5])
    )
    .width(Fill)
    .height(Fill)
    .center(Fill)
    .into()
}

/// Server content view (chat or admin for active server)
fn server_content_view<'a>(
    conn: &'a ServerConnection,
    message_input: &'a str,
    admin_username: &'a str,
    admin_password: &'a str,
    admin_is_admin: bool,
    admin_permissions: &'a [(String, bool)],
    delete_username: &'a str,
    show_add_user: bool,
    show_delete_user: bool,
) -> Element<'a, Message> {
    // If any admin panel is open, show admin view, otherwise show chat
    if show_add_user || show_delete_user {
        admin_view(conn, admin_username, admin_password, admin_is_admin, admin_permissions, delete_username, show_add_user, show_delete_user)
    } else {
        chat_view(conn, message_input, show_add_user, show_delete_user)
    }
}

/// Chat view for active server
fn chat_view<'a>(
    conn: &'a ServerConnection,
    message_input: &'a str,
    _show_add_user: bool,
    _show_delete_user: bool,
) -> Element<'a, Message> {
    // No tabs in chat view - just the content
    let top_bar = row![]
        .spacing(0)
        .padding(0);

    // Chat messages
    let mut chat_column = Column::new().spacing(3).padding(8);
    for msg in &conn.chat_messages {
        let time_str = msg.timestamp.format("%H:%M:%S").to_string();
        let display = if msg.username == "System" {
            text(format!("[{}] {} {}", time_str, icons::PREFIX_SYSTEM, msg.message)).size(12).color([0.7, 0.7, 0.7])
        } else if msg.username == "Error" {
            text(format!("[{}] {} {}", time_str, icons::ICON_ERROR, msg.message)).size(12).color([1.0, 0.0, 0.0])
        } else if msg.username == "Info" {
            text(format!("[{}] {} {}", time_str, icons::ICON_INFO, msg.message)).size(12).color([0.5, 0.8, 1.0])
        } else {
            text(format!("[{}] {}: {}", time_str, msg.username, msg.message)).size(12)
        };
        chat_column = chat_column.push(display);
    }

    let chat_scrollable = scrollable(chat_column)
        .id(ScrollableId::ChatMessages.into())
        .height(Fill);

    // Message input
    let input_row = row![
        text_input("Type a message...", message_input)
            .on_input(Message::MessageInputChanged)
            .on_submit(Message::SendMessagePressed)
            .padding(8)
            .size(13),
        button(text("Send").size(12))
            .on_press(Message::SendMessagePressed)
            .padding(8),
    ]
    .spacing(5);

    container(
        column![
            top_bar,
            chat_scrollable,
            input_row,
        ]
        .spacing(5)
        .padding(5)
    )
    .width(Fill)
    .height(Fill)
    .into()
}

/// Admin view for active server
fn admin_view<'a>(
    _conn: &'a ServerConnection,
    admin_username: &'a str,
    admin_password: &'a str,
    admin_is_admin: bool,
    admin_permissions: &'a [(String, bool)],
    delete_username: &'a str,
    show_add_user: bool,
    show_delete_user: bool,
) -> Element<'a, Message> {

    // Show Add User form
    if show_add_user {
        let create_title = text("User Create")
            .size(20)
            .width(Fill)
            .align_x(Center);
        
        let can_create = !admin_username.trim().is_empty() && !admin_password.trim().is_empty();

        let username_input = text_input("Username", admin_username)
            .on_input(Message::AdminUsernameChanged)
            .on_submit(if can_create { Message::CreateUserPressed } else { Message::AdminUsernameChanged(String::new()) })
            .id(text_input::Id::from(InputId::AdminUsername))
            .padding(8)
            .size(14);

        let password_input = text_input("Password", admin_password)
            .on_input(Message::AdminPasswordChanged)
            .on_submit(if can_create { Message::CreateUserPressed } else { Message::AdminPasswordChanged(String::new()) })
            .id(text_input::Id::from(InputId::AdminPassword))
            .secure(true)
            .padding(8)
            .size(14);

        let admin_checkbox = checkbox("Make Admin", admin_is_admin)
            .on_toggle(Message::AdminIsAdminToggled)
            .size(14);

        let permissions_title = text("Permissions:").size(14);
        let mut permissions_column = Column::new().spacing(5);
        for (permission, enabled) in admin_permissions {
            let perm_name = permission.clone();
            let checkbox_widget = checkbox(permission.as_str(), *enabled)
                .on_toggle(move |checked| Message::AdminPermissionToggled(perm_name.clone(), checked))
                .size(14);
            permissions_column = permissions_column.push(checkbox_widget);
        }

        let create_button = if can_create {
            button(text("Create").size(14))
                .on_press(Message::CreateUserPressed)
                .padding(10)
        } else {
            button(text("Create").size(14)).padding(10)
        };

        let cancel_button = button(text("Cancel").size(14))
            .on_press(Message::ToggleAddUser)
            .padding(10);

        let create_form = column![
            create_title,
            text("").size(15),
            username_input,
            password_input,
            admin_checkbox,
            text("").size(5),
            permissions_title,
            permissions_column,
            text("").size(10),
            row![
                create_button,
                cancel_button,
            ]
            .spacing(10),
        ]
        .spacing(10)
        .padding(20)
        .max_width(400);

        return container(create_form)
            .width(Fill)
            .height(Fill)
            .padding(20)
            .center(Fill)
            .into();
    }

    // Show Delete User panel
    if show_delete_user {
        let delete_title = text("User Delete")
            .size(20)
            .width(Fill)
            .align_x(Center);
        
        let can_delete = !delete_username.trim().is_empty();
        
        let username_input = text_input("Username", delete_username)
            .on_input(Message::DeleteUsernameChanged)
            .on_submit(if can_delete { Message::DeleteUserPressed(delete_username.to_string()) } else { Message::DeleteUsernameChanged(String::new()) })
            .id(text_input::Id::from(InputId::DeleteUsername))
            .padding(8)
            .size(14);
        
        let delete_button = if can_delete {
            button(text("Delete").size(14))
                .on_press(Message::DeleteUserPressed(delete_username.to_string()))
                .padding(10)
        } else {
            button(text("Delete").size(14)).padding(10)
        };
        
        let cancel_button = button(text("Cancel").size(14))
            .on_press(Message::ToggleDeleteUser)
            .padding(10);

        let delete_form = column![
            delete_title,
            text("").size(15),
            username_input,
            text("").size(10),
            text(format!("{} Warning: Deletion is permanent!", icons::ICON_WARNING)).color([1.0, 0.5, 0.0]).size(14),
            text("").size(10),
            row![
                delete_button,
                cancel_button,
            ]
            .spacing(10),
        ]
        .spacing(10)
        .padding(20)
        .max_width(400);

        return container(delete_form)
            .width(Fill)
            .height(Fill)
            .padding(20)
            .center(Fill)
            .into();
    }

    // Fallback (should never reach here since we only call admin_view when at least one is true)
    container(text(""))
        .width(Fill)
        .height(Fill)
        .into()
}

/// User list panel (right side)
fn user_list_panel<'a>(users: &'a [UserInfo], _bookmark_index: usize) -> Element<'a, Message> {
    let title = text("Users").size(16);

    let mut users_column = Column::new().spacing(3).padding(5);
    
    if users.is_empty() {
        users_column = users_column.push(
            text("No users online")
                .size(11)
                .color([0.5, 0.5, 0.5])
        );
    } else {
        for user in users {
            users_column = users_column.push(
                button(text(&user.username).size(12))
                    .on_press(Message::RequestUserInfo(user.session_id))
                    .width(Fill)
                    .padding(6),
            );
        }
    }

    let panel = column![
        title,
        scrollable(users_column).height(Fill),
    ]
    .spacing(8)
    .padding(10)
    .width(180);

    container(panel)
        .height(Fill)
        .style(|_theme| {
            container::Style {
                background: Some(iced::Background::Color(iced::Color::from_rgb(0.12, 0.12, 0.12))),
                border: iced::Border {
                    color: iced::Color::from_rgb(0.2, 0.2, 0.2),
                    width: 1.0,
                    ..Default::default()
                },
                ..Default::default()
            }
        })
        .into()
}

/// Bookmark edit view (main content area)
fn bookmark_edit_view<'a>(
    bookmark_edit_mode: &'a BookmarkEditMode,
    bookmark_name: &'a str,
    bookmark_address: &'a str,
    bookmark_port: &'a str,
    bookmark_username: &'a str,
    bookmark_password: &'a str,
) -> Element<'a, Message> {
    let dialog_title = match bookmark_edit_mode {
        BookmarkEditMode::Add => "Add Server",
        BookmarkEditMode::Edit(_) => "Edit Server",
        BookmarkEditMode::None => "",
    };

    let can_save = !bookmark_name.trim().is_empty() 
        && !bookmark_address.trim().is_empty() 
        && !bookmark_port.trim().is_empty();

    let content = column![
        text(dialog_title).size(20).width(Fill).align_x(Center),
        text("").size(15),
        text_input("Server Name", bookmark_name)
            .on_input(Message::BookmarkNameChanged)
            .on_submit(if can_save { Message::SaveBookmark } else { Message::BookmarkNameChanged(String::new()) })
            .id(text_input::Id::from(InputId::BookmarkName))
            .padding(8)
            .size(14),
        text_input("IPv6 Address", bookmark_address)
            .on_input(Message::BookmarkAddressChanged)
            .on_submit(if can_save { Message::SaveBookmark } else { Message::BookmarkAddressChanged(String::new()) })
            .id(text_input::Id::from(InputId::BookmarkAddress))
            .padding(8)
            .size(14),
        text_input("Port", bookmark_port)
            .on_input(Message::BookmarkPortChanged)
            .on_submit(if can_save { Message::SaveBookmark } else { Message::BookmarkPortChanged(String::new()) })
            .id(text_input::Id::from(InputId::BookmarkPort))
            .padding(8)
            .size(14),
        text_input("Username (optional)", bookmark_username)
            .on_input(Message::BookmarkUsernameChanged)
            .on_submit(if can_save { Message::SaveBookmark } else { Message::BookmarkUsernameChanged(String::new()) })
            .id(text_input::Id::from(InputId::BookmarkUsername))
            .padding(8)
            .size(14),
        text_input("Password (optional)", bookmark_password)
            .on_input(Message::BookmarkPasswordChanged)
            .on_submit(if can_save { Message::SaveBookmark } else { Message::BookmarkPasswordChanged(String::new()) })
            .id(text_input::Id::from(InputId::BookmarkPassword))
            .secure(true)
            .padding(8)
            .size(14),
        text("").size(10),
        row![
            if can_save {
                button(text("Save").size(14))
                    .on_press(Message::SaveBookmark)
                    .padding(10)
            } else {
                button(text("Save").size(14)).padding(10)
            },
            button(text("Cancel").size(14))
                .on_press(Message::CancelBookmarkEdit)
                .padding(10),
        ]
        .spacing(10),
    ]
    .spacing(10)
    .padding(20)
    .max_width(400);

    container(content)
        .width(Fill)
        .height(Fill)
        .padding(20)
        .center(Fill)
        .into()
}