//! Chat interface for active server connections

use super::constants::PERMISSION_CHAT_SEND;
use crate::style::{
    BORDER_WIDTH, CHAT_INPUT_SIZE, CHAT_LINE_HEIGHT, CHAT_MESSAGE_SIZE, CHAT_SPACING,
    INPUT_PADDING, MONOSPACE_FONT, SMALL_PADDING, SMALL_SPACING, TOOLTIP_BACKGROUND_COLOR,
    TOOLTIP_BACKGROUND_PADDING, TOOLTIP_GAP, TOOLTIP_PADDING, TOOLTIP_TEXT_SIZE,
    action_button_text, admin_user_text_color, broadcast_message_color, chat_tab_active_style,
    chat_tab_inactive_style, chat_text_color, chat_timestamp_color, error_message_color,
    info_text_color, primary_button_style, primary_scrollbar_style, primary_text_input_style,
    shaped_text, sidebar_border, system_text_color, tooltip_border, tooltip_text_color,
};
use crate::handlers::network::{
    msg_username_broadcast_prefix, msg_username_error, msg_username_info, msg_username_system,
};
use crate::i18n::t;
use crate::types::{ChatTab, InputId, Message, ScrollableId, ServerConnection};
use iced::Theme;
use iced::widget::{
    Column, button, column, container, rich_text, row, scrollable, span, text_input, tooltip,
};
use iced::{Background, Element, Fill};

/// Create a tab button with appropriate styling and unread indicator
fn create_tab_button(
    tab: ChatTab,
    label: String,
    is_active: bool,
    has_unread: bool,
) -> Element<'static, Message> {
    if is_active {
        // Active PM tabs include a close button
        if let ChatTab::UserMessage(ref username) = tab {
            let username_clone = username.clone();
            let close_button = tooltip(
                button(crate::icon::close().size(CHAT_MESSAGE_SIZE))
                    .on_press(Message::CloseUserMessageTab(username_clone))
                    .padding(iced::Padding::new(0.0).left(SMALL_PADDING as f32))
                    .style(|_theme, status| button::Style {
                        background: None,
                        text_color: match status {
                            button::Status::Hovered => error_message_color(),
                            _ => action_button_text(),
                        },
                        border: iced::Border::default(),
                        shadow: iced::Shadow::default(),
                    }),
                container(
                    shaped_text(format!("{} {}", t("tooltip-close"), username))
                        .size(TOOLTIP_TEXT_SIZE),
                )
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

            let tab_content = row![shaped_text(label).size(CHAT_MESSAGE_SIZE), close_button]
                .spacing(SMALL_SPACING)
                .align_y(iced::Alignment::Center);

            button(tab_content)
                .style(chat_tab_active_style())
                .padding(iced::Padding::new(INPUT_PADDING as f32).right(SMALL_PADDING as f32))
                .into()
        } else {
            button(shaped_text(label).size(CHAT_MESSAGE_SIZE))
                .style(chat_tab_active_style())
                .padding(INPUT_PADDING)
                .into()
        }
    } else {
        let tab_text = if has_unread {
            // Bold if there are unread messages
            shaped_text(label).size(CHAT_MESSAGE_SIZE).font(iced::Font {
                weight: iced::font::Weight::Bold,
                ..iced::Font::DEFAULT
            })
        } else {
            shaped_text(label).size(CHAT_MESSAGE_SIZE)
        };

        button(tab_text)
            .on_press(Message::SwitchChatTab(tab))
            .style(chat_tab_inactive_style())
            .padding(INPUT_PADDING)
            .into()
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
pub fn chat_view<'a>(
    conn: &'a ServerConnection,
    message_input: &'a str,
    theme: Theme,
) -> Element<'a, Message> {
    // Build tab bar row
    let mut tab_row = row![].spacing(SMALL_SPACING);

    // Server tab (always present)
    let is_server_active = conn.active_chat_tab == ChatTab::Server;
    let server_has_unread = conn.unread_tabs.contains(&ChatTab::Server);
    let server_tab_button = create_tab_button(
        ChatTab::Server,
        t("chat-tab-server"),
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

    // Wrap tabs to multiple rows if needed
    let tab_bar = tab_row.wrap();

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

    // Check if a username belongs to an admin
    let is_admin_user = |username: &str| -> bool {
        conn.online_users
            .iter()
            .any(|u| u.username == username && u.is_admin)
    };

    for msg in messages {
        let time_str = msg.timestamp.format("%H:%M").to_string();

        // Split message into lines to prevent spoofing via embedded newlines
        // Each line is displayed with the same timestamp/username prefix
        for line in msg.message.split('\n') {
            // Grey timestamp for all message types
            let timestamp_color = chat_timestamp_color(&theme);

            let display: Element<'_, Message> = if msg.username == msg_username_system() {
                // System messages: timestamp grey, rest in system color
                let color = system_text_color(&theme);
                rich_text![
                    span(format!("[{}] ", time_str)).color(timestamp_color),
                    span(format!("{} ", t("chat-prefix-system"))).color(color),
                    span(line).color(color),
                ]
                .size(CHAT_MESSAGE_SIZE)
                .line_height(CHAT_LINE_HEIGHT)
                .font(MONOSPACE_FONT)
                .into()
            } else if msg.username == msg_username_error() {
                // Error messages: timestamp grey, rest in error color
                let color = error_message_color();
                rich_text![
                    span(format!("[{}] ", time_str)).color(timestamp_color),
                    span(format!("{} ", t("chat-prefix-error"))).color(color),
                    span(line).color(color),
                ]
                .size(CHAT_MESSAGE_SIZE)
                .line_height(CHAT_LINE_HEIGHT)
                .font(MONOSPACE_FONT)
                .into()
            } else if msg.username == msg_username_info() {
                // Info messages: timestamp grey, rest in info color
                let color = info_text_color(&theme);
                rich_text![
                    span(format!("[{}] ", time_str)).color(timestamp_color),
                    span(format!("{} ", t("chat-prefix-info"))).color(color),
                    span(line).color(color),
                ]
                .size(CHAT_MESSAGE_SIZE)
                .line_height(CHAT_LINE_HEIGHT)
                .font(MONOSPACE_FONT)
                .into()
            } else if msg.username.starts_with(&msg_username_broadcast_prefix()) {
                // Broadcast messages: timestamp grey, rest in broadcast color
                let broadcast_color = broadcast_message_color(&theme);
                rich_text![
                    span(format!("[{}] ", time_str)).color(timestamp_color),
                    span(format!("{}: ", msg.username)).color(broadcast_color),
                    span(line).color(broadcast_color),
                ]
                .size(CHAT_MESSAGE_SIZE)
                .line_height(CHAT_LINE_HEIGHT)
                .font(MONOSPACE_FONT)
                .into()
            } else {
                // Regular chat messages: timestamp grey, username colored by admin status, message normal
                let username_color = if is_admin_user(&msg.username) {
                    admin_user_text_color(&theme)
                } else {
                    chat_text_color(&theme)
                };
                let text_color = chat_text_color(&theme);
                rich_text![
                    span(format!("[{}] ", time_str)).color(timestamp_color),
                    span(format!("{}: ", msg.username)).color(username_color),
                    span(line).color(text_color),
                ]
                .size(CHAT_MESSAGE_SIZE)
                .line_height(CHAT_LINE_HEIGHT)
                .font(MONOSPACE_FONT)
                .into()
            };
            chat_column = chat_column.push(display);
        }
    }

    let chat_scrollable = scrollable(chat_column)
        .id(ScrollableId::ChatMessages.into())
        .on_scroll(Message::ChatScrolled)
        .width(Fill)
        .height(Fill)
        .style(primary_scrollbar_style());

    // Message input placeholder based on active tab
    let placeholder = t("placeholder-message");

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
            text_input(&placeholder, message_input)
                .on_input(Message::ChatInputChanged)
                .on_submit(Message::SendMessagePressed)
                .id(text_input::Id::from(InputId::ChatInput))
                .padding(INPUT_PADDING)
                .size(CHAT_INPUT_SIZE)
                .font(MONOSPACE_FONT)
                .style(primary_text_input_style())
                .width(Fill)
        } else {
            text_input(&t("placeholder-no-permission"), message_input)
                .id(text_input::Id::from(InputId::ChatInput))
                .padding(INPUT_PADDING)
                .size(CHAT_INPUT_SIZE)
                .font(MONOSPACE_FONT)
                .style(primary_text_input_style())
                .width(Fill)
        },
        if can_send_message {
            button(shaped_text(t("button-send")).size(CHAT_MESSAGE_SIZE))
                .on_press(Message::SendMessagePressed)
                .padding(INPUT_PADDING)
                .style(primary_button_style())
        } else {
            button(shaped_text(t("button-send")).size(CHAT_MESSAGE_SIZE))
                .padding(INPUT_PADDING)
                .style(primary_button_style())
        },
    ]
    .spacing(SMALL_SPACING)
    .width(Fill);

    // Top border separator to match sidebars
    let top_separator = container(shaped_text(""))
        .width(Fill)
        .height(BORDER_WIDTH)
        .style(|theme| container::Style {
            background: Some(Background::Color(sidebar_border(theme))),
            ..Default::default()
        });

    // Bottom border separator to match sidebars
    let bottom_separator = container(shaped_text(""))
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
