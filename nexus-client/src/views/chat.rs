//! Chat interface for active server connections

use super::constants::{PERMISSION_CHAT_SEND, PERMISSION_USER_MESSAGE};
use crate::i18n::t;
use crate::style::{
    BOLD_FONT, CHAT_INPUT_SIZE, CHAT_LINE_HEIGHT, CHAT_MESSAGE_SIZE, CHAT_SPACING,
    CHAT_TIME_FORMAT, CLOSE_BUTTON_PADDING, INPUT_PADDING, MONOSPACE_FONT, SMALL_PADDING,
    SMALL_SPACING, TAB_CONTENT_PADDING, TOOLTIP_BACKGROUND_PADDING, TOOLTIP_GAP, TOOLTIP_PADDING,
    TOOLTIP_TEXT_SIZE, chat, chat_tab_active_style, chat_tab_inactive_style,
    close_button_on_primary_style, content_background_style, shaped_text, tooltip_container_style,
};
use crate::types::{ChatTab, InputId, Message, MessageType, ScrollableId, ServerConnection};
use iced::widget::{
    Column, button, column, container, rich_text, row, scrollable, span, text_input, tooltip,
};
use iced::{Color, Element, Fill, Theme};

// ============================================================================
// Helper Functions
// ============================================================================

/// Build a styled rich text message with consistent formatting
fn styled_message<'a>(
    time_str: &str,
    timestamp_color: Color,
    prefix: String,
    prefix_color: Color,
    content: &str,
    content_color: Color,
) -> Element<'a, Message> {
    rich_text![
        span(format!("[{}] ", time_str)).color(timestamp_color),
        span(prefix).color(prefix_color),
        span(content.to_string()).color(content_color),
    ]
    .size(CHAT_MESSAGE_SIZE)
    .line_height(CHAT_LINE_HEIGHT)
    .font(MONOSPACE_FONT)
    .into()
}

/// Check if a username belongs to an admin in the online users list
fn is_admin_username(conn: &ServerConnection, username: &str) -> bool {
    conn.online_users
        .iter()
        .any(|u| u.username == username && u.is_admin)
}

// ============================================================================
// Tab Button
// ============================================================================

/// Create a tab button with appropriate styling and unread indicator
fn create_tab_button(
    tab: ChatTab,
    label: String,
    is_active: bool,
    has_unread: bool,
) -> Element<'static, Message> {
    if is_active {
        create_active_tab_button(tab, label)
    } else {
        create_inactive_tab_button(tab, label, has_unread)
    }
}

/// Create an active tab button (with close button for PM tabs)
fn create_active_tab_button(tab: ChatTab, label: String) -> Element<'static, Message> {
    // Active PM tabs include a close button
    if let ChatTab::UserMessage(ref username) = tab {
        let username_clone = username.clone();
        let close_button = tooltip(
            button(crate::icon::close().size(CHAT_MESSAGE_SIZE))
                .on_press(Message::CloseUserMessageTab(username_clone))
                .padding(CLOSE_BUTTON_PADDING)
                .style(close_button_on_primary_style()),
            container(
                shaped_text(format!("{} {}", t("tooltip-close"), username)).size(TOOLTIP_TEXT_SIZE),
            )
            .padding(TOOLTIP_BACKGROUND_PADDING)
            .style(tooltip_container_style),
            tooltip::Position::Bottom,
        )
        .gap(TOOLTIP_GAP)
        .padding(TOOLTIP_PADDING);

        let tab_content = row![
            iced::widget::text(label)
                .size(CHAT_MESSAGE_SIZE)
                .shaping(iced::widget::text::Shaping::Advanced),
            close_button
        ]
        .spacing(SMALL_SPACING)
        .align_y(iced::Alignment::Center);

        button(tab_content)
            .on_press(Message::SwitchChatTab(tab))
            .padding(TAB_CONTENT_PADDING)
            .style(chat_tab_active_style())
            .into()
    } else {
        // Server tab (no close button)
        button(shaped_text(label).size(CHAT_MESSAGE_SIZE))
            .on_press(Message::SwitchChatTab(tab))
            .padding(INPUT_PADDING)
            .style(chat_tab_active_style())
            .into()
    }
}

/// Create an inactive tab button (bold if unread)
fn create_inactive_tab_button(
    tab: ChatTab,
    label: String,
    has_unread: bool,
) -> Element<'static, Message> {
    let tab_text = if has_unread {
        // Bold if there are unread messages
        shaped_text(label).size(CHAT_MESSAGE_SIZE).font(BOLD_FONT)
    } else {
        shaped_text(label).size(CHAT_MESSAGE_SIZE)
    };

    button(tab_text)
        .on_press(Message::SwitchChatTab(tab))
        .style(chat_tab_inactive_style())
        .padding(INPUT_PADDING)
        .into()
}

// ============================================================================
// Message Rendering
// ============================================================================

/// Build a rich text element for a single message line
fn render_message_line<'a>(
    time_str: &str,
    username: &str,
    line: &str,
    message_type: MessageType,
    theme: &Theme,
    username_is_admin: bool,
) -> Element<'a, Message> {
    let timestamp_color = chat::timestamp(theme);

    match message_type {
        MessageType::System => {
            let color = chat::system(theme);
            styled_message(
                time_str,
                timestamp_color,
                format!("{} ", t("chat-prefix-system")),
                color,
                line,
                color,
            )
        }
        MessageType::Error => {
            let color = chat::error(theme);
            styled_message(
                time_str,
                timestamp_color,
                format!("{} ", t("chat-prefix-error")),
                color,
                line,
                color,
            )
        }
        MessageType::Info => {
            let color = chat::info(theme);
            styled_message(
                time_str,
                timestamp_color,
                format!("{} ", t("chat-prefix-info")),
                color,
                line,
                color,
            )
        }
        MessageType::Broadcast => {
            let color = chat::broadcast(theme);
            styled_message(
                time_str,
                timestamp_color,
                format!("{} {}: ", t("chat-prefix-broadcast"), username),
                color,
                line,
                color,
            )
        }
        MessageType::Chat => {
            let username_color = if username_is_admin {
                chat::admin(theme)
            } else {
                chat::text(theme)
            };
            let text_color = chat::text(theme);
            styled_message(
                time_str,
                timestamp_color,
                format!("{}: ", username),
                username_color,
                line,
                text_color,
            )
        }
    }
}

// ============================================================================
// Message List
// ============================================================================

/// Build the message list column for the active chat tab
fn build_message_list<'a>(conn: &'a ServerConnection, theme: &Theme) -> Column<'a, Message> {
    let messages = match &conn.active_chat_tab {
        ChatTab::Server => conn.chat_messages.as_slice(),
        ChatTab::UserMessage(username) => conn
            .user_messages
            .get(username)
            .map(|v| v.as_slice())
            .unwrap_or(&[]),
    };

    let mut chat_column = Column::new().spacing(CHAT_SPACING).padding(INPUT_PADDING);

    for msg in messages {
        let time_str = msg.get_timestamp().format(CHAT_TIME_FORMAT).to_string();
        let username_is_admin = is_admin_username(conn, &msg.username);

        // Split message into lines to prevent spoofing via embedded newlines
        // Each line is displayed with the same timestamp/username prefix
        for line in msg.message.split('\n') {
            let display = render_message_line(
                &time_str,
                &msg.username,
                line,
                msg.message_type,
                theme,
                username_is_admin,
            );
            chat_column = chat_column.push(display);
        }
    }

    chat_column
}

// ============================================================================
// Input Row
// ============================================================================

/// Build the message input row with text field and send button
fn build_input_row<'a>(
    message_input: &'a str,
    can_send_message: bool,
) -> iced::widget::Row<'a, Message> {
    let text_field = if can_send_message {
        text_input(&t("placeholder-message"), message_input)
            .on_input(Message::ChatInputChanged)
            .on_submit(Message::SendMessagePressed)
            .id(text_input::Id::from(InputId::ChatInput))
            .padding(INPUT_PADDING)
            .size(CHAT_INPUT_SIZE)
            .font(MONOSPACE_FONT)
            .width(Fill)
    } else {
        text_input(&t("placeholder-no-permission"), message_input)
            .id(text_input::Id::from(InputId::ChatInput))
            .padding(INPUT_PADDING)
            .size(CHAT_INPUT_SIZE)
            .font(MONOSPACE_FONT)
            .width(Fill)
    };

    let send_button = if can_send_message {
        button(shaped_text(t("button-send")).size(CHAT_MESSAGE_SIZE))
            .on_press(Message::SendMessagePressed)
            .padding(INPUT_PADDING)
    } else {
        button(shaped_text(t("button-send")).size(CHAT_MESSAGE_SIZE)).padding(INPUT_PADDING)
    };

    row![text_field, send_button]
        .spacing(SMALL_SPACING)
        .width(Fill)
}

// ============================================================================
// Tab Bar
// ============================================================================

/// Build the tab bar with server and PM tabs
fn build_tab_bar(conn: &ServerConnection) -> (iced::widget::Row<'static, Message>, bool) {
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

    // PM tabs (sorted alphabetically)
    let mut pm_usernames: Vec<String> = conn.user_messages.keys().cloned().collect();
    pm_usernames.sort();

    let has_pm_tabs = !pm_usernames.is_empty();

    for username in pm_usernames {
        let pm_tab = ChatTab::UserMessage(username.clone());
        let is_active = conn.active_chat_tab == pm_tab;
        let has_unread = conn.unread_tabs.contains(&pm_tab);
        let pm_tab_button = create_tab_button(pm_tab, username, is_active, has_unread);
        tab_row = tab_row.push(pm_tab_button);
    }

    (tab_row, has_pm_tabs)
}

// ============================================================================
// Chat View
// ============================================================================

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
    // Build tab bar
    let (tab_row, has_pm_tabs) = build_tab_bar(conn);
    let tab_bar = tab_row.wrap();

    // Build message list
    let chat_column = build_message_list(conn, &theme);

    let chat_scrollable = scrollable(chat_column)
        .id(ScrollableId::ChatMessages.into())
        .on_scroll(Message::ChatScrolled)
        .width(Fill)
        .height(Fill);

    // Determine if user can send messages
    let can_send_message = match &conn.active_chat_tab {
        ChatTab::Server => {
            conn.is_admin || conn.permissions.iter().any(|p| p == PERMISSION_CHAT_SEND)
        }
        ChatTab::UserMessage(_) => {
            conn.is_admin
                || conn
                    .permissions
                    .iter()
                    .any(|p| p == PERMISSION_USER_MESSAGE)
        }
    };

    // Build input row
    let input_row = build_input_row(message_input, can_send_message);

    // Chat content with background
    let chat_content = container(
        column![chat_scrollable, input_row]
            .spacing(SMALL_SPACING)
            .padding(SMALL_PADDING),
    )
    .width(Fill)
    .height(Fill)
    .style(content_background_style);

    // Only show tab bar if there are PM tabs (more than just #server)
    if has_pm_tabs {
        column![
            container(tab_bar).padding(SMALL_PADDING).width(Fill),
            chat_content,
        ]
        .width(Fill)
        .height(Fill)
        .into()
    } else {
        chat_content.into()
    }
}
