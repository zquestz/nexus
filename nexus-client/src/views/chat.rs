//! Chat interface for active server connections

use super::style::{
    BORDER_WIDTH, CHAT_INPUT_SIZE, CHAT_MESSAGE_SIZE, CHAT_SPACING, INPUT_PADDING, SMALL_PADDING,
    SMALL_SPACING, TOOLTIP_BACKGROUND_COLOR, TOOLTIP_BACKGROUND_PADDING, TOOLTIP_GAP,
    TOOLTIP_PADDING, TOOLTIP_TEXT_SIZE, broadcast_message_color, chat_tab_active_style,
    chat_tab_inactive_style, chat_text_color, error_message_color, info_text_color,
    primary_button_style, primary_scrollbar_style, primary_text_input_style, sidebar_border,
    system_text_color, tooltip_border, tooltip_text_color,
};
use crate::types::{ChatTab, InputId, Message, ScrollableId, ServerConnection};
use iced::widget::{
    Button, Column, button, column, container, row, scrollable, text, text_input, tooltip,
};
use iced::{Background, Element, Fill};

// Permission constants
const PERMISSION_CHAT_SEND: &str = "chat_send";

// Input placeholder text
const PLACEHOLDER_MESSAGE: &str = "Type a message...";
const PLACEHOLDER_NO_PERMISSION: &str = "No permission";

/// Create a tab button with appropriate styling and unread indicator
fn create_tab_button<'a>(
    tab: ChatTab,
    label: String,
    is_active: bool,
    has_unread: bool,
) -> Button<'a, Message> {
    if is_active {
        button(text(label).size(CHAT_MESSAGE_SIZE))
            .style(chat_tab_active_style())
            .padding(INPUT_PADDING)
    } else {
        let tab_text = if has_unread {
            // Bold if there are unread messages
            text(label).size(CHAT_MESSAGE_SIZE).font(iced::Font {
                weight: iced::font::Weight::Bold,
                ..iced::Font::DEFAULT
            })
        } else {
            text(label).size(CHAT_MESSAGE_SIZE)
        };

        button(tab_text)
            .on_press(Message::SwitchChatTab(tab))
            .style(chat_tab_inactive_style())
            .padding(INPUT_PADDING)
    }
}

/// Displays chat messages and input field
///
/// The chat area serves as a message/notification center that displays:
/// - System messages (user connect/disconnect, topic changes)
/// - Error messages
/// - Info messages (command responses)
/// - Broadcast messages
/// - Chat messages (server enforces chat_receive permission)
///
/// The send input is only enabled with chat_send permission.
pub fn chat_view<'a>(conn: &'a ServerConnection, message_input: &'a str) -> Element<'a, Message> {
    // Build tab bar row
    let mut tab_row = row![].spacing(SMALL_SPACING);

    // Server tab (always present)
    let is_server_active = conn.active_chat_tab == ChatTab::Server;
    let server_has_unread = conn.unread_tabs.contains(&ChatTab::Server);
    let server_tab_button = create_tab_button(
        ChatTab::Server,
        "#server".to_string(),
        is_server_active,
        server_has_unread,
    );
    tab_row = tab_row.push(server_tab_button);

    // PM tabs
    let mut pm_usernames: Vec<String> = conn.user_messages.keys().cloned().collect();
    pm_usernames.sort();

    // Check if we have any PM tabs
    let has_pm_tabs = !pm_usernames.is_empty();

    for username in &pm_usernames {
        let pm_tab = ChatTab::UserMessage(username.clone());
        let is_active = conn.active_chat_tab == pm_tab;
        let has_unread = conn.unread_tabs.contains(&pm_tab);
        let pm_tab_button = create_tab_button(pm_tab, username.clone(), is_active, has_unread);
        tab_row = tab_row.push(pm_tab_button);
    }

    // Wrap tab bar in scrollable (tabs only, not close button)
    let scrollable_tabs = scrollable(tab_row)
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::default(),
        ))
        .width(Fill)
        .style(primary_scrollbar_style());

    // Build final tab bar with close button outside scrollable
    let tab_bar = if let ChatTab::UserMessage(ref username) = conn.active_chat_tab {
        let username_clone = username.clone();
        let close_button = tooltip(
            button(crate::icon::close().size(CHAT_MESSAGE_SIZE))
                .on_press(Message::CloseUserMessageTab(username_clone))
                .padding(INPUT_PADDING)
                .style(|theme, status| button::Style {
                    background: None,
                    text_color: match status {
                        button::Status::Hovered => error_message_color(),
                        _ => chat_text_color(theme),
                    },
                    border: iced::Border::default(),
                    shadow: iced::Shadow::default(),
                }),
            container(text(format!("Close {}", username)).size(TOOLTIP_TEXT_SIZE))
                .padding(TOOLTIP_BACKGROUND_PADDING)
                .style(|theme| container::Style {
                    background: Some(Background::Color(TOOLTIP_BACKGROUND_COLOR)),
                    text_color: Some(tooltip_text_color(theme)),
                    border: tooltip_border(),
                    ..Default::default()
                }),
            tooltip::Position::Bottom,
        )
        .gap(TOOLTIP_GAP)
        .padding(TOOLTIP_PADDING);

        row![scrollable_tabs, close_button].spacing(SMALL_SPACING)
    } else {
        row![scrollable_tabs]
    };

    // Check send permission (for server chat)
    let can_send = conn.is_admin || conn.permissions.iter().any(|p| p == PERMISSION_CHAT_SEND);

    // Get messages for active tab
    let messages = match &conn.active_chat_tab {
        ChatTab::Server => &conn.chat_messages,
        ChatTab::UserMessage(username) => conn
            .user_messages
            .get(username)
            .map(|v| v.as_slice())
            .unwrap_or(&[]),
    };

    // Build message list
    let mut chat_column = Column::new().spacing(CHAT_SPACING).padding(INPUT_PADDING);

    for msg in messages {
        let time_str = msg.timestamp.format("%H:%M:%S").to_string();
        let display = if msg.username == "System" {
            text(format!("[{}] [SYS] {}", time_str, msg.message))
                .size(CHAT_MESSAGE_SIZE)
                .style(|theme| iced::widget::text::Style {
                    color: Some(system_text_color(theme)),
                })
                .font(iced::Font::MONOSPACE)
        } else if msg.username == "Error" {
            text(format!("[{}] [ERR] {}", time_str, msg.message))
                .size(CHAT_MESSAGE_SIZE)
                .color(error_message_color())
                .font(iced::Font::MONOSPACE)
        } else if msg.username == "Info" {
            text(format!("[{}] [INFO] {}", time_str, msg.message))
                .size(CHAT_MESSAGE_SIZE)
                .style(|theme| iced::widget::text::Style {
                    color: Some(info_text_color(theme)),
                })
                .font(iced::Font::MONOSPACE)
        } else if msg.username.starts_with("[BROADCAST]") {
            text(format!("[{}] {}: {}", time_str, msg.username, msg.message))
                .size(CHAT_MESSAGE_SIZE)
                .style(|theme| iced::widget::text::Style {
                    color: Some(broadcast_message_color(theme)),
                })
                .font(iced::Font::MONOSPACE)
        } else {
            text(format!("[{}] {}: {}", time_str, msg.username, msg.message))
                .size(CHAT_MESSAGE_SIZE)
                .style(|theme| iced::widget::text::Style {
                    color: Some(chat_text_color(theme)),
                })
                .font(iced::Font::MONOSPACE)
        };
        chat_column = chat_column.push(display);
    }

    let chat_scrollable = scrollable(chat_column)
        .id(ScrollableId::ChatMessages.into())
        .width(Fill)
        .height(Fill)
        .style(primary_scrollbar_style());

    // Message input placeholder based on active tab
    let placeholder = PLACEHOLDER_MESSAGE;

    // Can send if: (Server tab + chat_send) OR (PM tab + user_message)
    let can_send_message = match &conn.active_chat_tab {
        ChatTab::Server => can_send,
        ChatTab::UserMessage(_) => {
            conn.is_admin || conn.permissions.iter().any(|p| p == "user_message")
        }
    };

    // Message input
    let input_row = row![
        if can_send_message {
            text_input(placeholder, message_input)
                .on_input(Message::ChatInputChanged)
                .on_submit(Message::SendMessagePressed)
                .id(text_input::Id::from(InputId::ChatInput))
                .padding(INPUT_PADDING)
                .size(CHAT_INPUT_SIZE)
                .style(primary_text_input_style())
                .width(Fill)
        } else {
            text_input(PLACEHOLDER_NO_PERMISSION, message_input)
                .id(text_input::Id::from(InputId::ChatInput))
                .padding(INPUT_PADDING)
                .size(CHAT_INPUT_SIZE)
                .style(primary_text_input_style())
                .width(Fill)
        },
        if can_send_message {
            button(text("Send").size(CHAT_MESSAGE_SIZE))
                .on_press(Message::SendMessagePressed)
                .padding(INPUT_PADDING)
                .style(primary_button_style())
        } else {
            button(text("Send").size(CHAT_MESSAGE_SIZE))
                .padding(INPUT_PADDING)
                .style(primary_button_style())
        },
    ]
    .spacing(SMALL_SPACING)
    .width(Fill);

    // Top border separator to match sidebars
    let top_separator = container(text(""))
        .width(Fill)
        .height(BORDER_WIDTH)
        .style(|theme| container::Style {
            background: Some(Background::Color(sidebar_border(theme))),
            ..Default::default()
        });

    // Bottom border separator to match sidebars
    let bottom_separator = container(text(""))
        .width(Fill)
        .height(BORDER_WIDTH)
        .style(|theme| container::Style {
            background: Some(Background::Color(sidebar_border(theme))),
            ..Default::default()
        });

    // Only show tab bar if there are PM tabs (more than just #server)
    if !has_pm_tabs {
        column![
            top_separator,
            container(
                column![chat_scrollable, input_row,]
                    .spacing(SMALL_SPACING)
                    .padding(SMALL_PADDING),
            )
            .width(Fill)
            .height(Fill),
            bottom_separator,
        ]
        .width(Fill)
        .height(Fill)
        .into()
    } else {
        column![
            top_separator,
            container(tab_bar).padding(SMALL_PADDING).width(Fill),
            container(
                column![chat_scrollable, input_row,]
                    .spacing(SMALL_SPACING)
                    .padding(SMALL_PADDING),
            )
            .width(Fill)
            .height(Fill),
            bottom_separator,
        ]
        .width(Fill)
        .height(Fill)
        .into()
    }
}
