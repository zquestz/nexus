//! Chat interface for active server connections

use crate::i18n::t;
use crate::style::{
    BOLD_FONT, CHAT_LINE_HEIGHT, CHAT_MESSAGE_SIZE, CHAT_SPACING, CLOSE_BUTTON_PADDING,
    INPUT_PADDING, MONOSPACE_FONT, SMALL_PADDING, SMALL_SPACING, TAB_CONTENT_PADDING,
    TOOLTIP_BACKGROUND_PADDING, TOOLTIP_GAP, TOOLTIP_PADDING, TOOLTIP_TEXT_SIZE, chat,
    chat_tab_active_style, close_button_on_primary_style, content_background_style, shaped_text,
    tooltip_container_style,
};
use crate::types::{ChatTab, InputId, Message, MessageType, ScrollableId, ServerConnection};
use iced::widget::scrollable::{Direction, Scrollbar};
use iced::widget::text::Wrapping;
use iced::widget::{
    Column, Id, button, column, container, rich_text, row, scrollable, span, text::Rich,
    text_input, tooltip,
};
use iced::{Color, Element, Fill, Font, Theme};
use linkify::{LinkFinder, LinkKind};
use once_cell::sync::Lazy;

// ============================================================================
// Timestamp Settings
// ============================================================================

/// Settings for timestamp display in chat messages
#[derive(Debug, Clone, Copy)]
pub struct TimestampSettings {
    /// Whether to show timestamps at all
    pub show_timestamps: bool,
    /// Use 24-hour format (false = 12-hour with AM/PM)
    pub use_24_hour_time: bool,
    /// Show seconds in the timestamp
    pub show_seconds: bool,
}

impl TimestampSettings {
    /// Format a timestamp according to the current settings
    pub fn format(&self, timestamp: &chrono::DateTime<chrono::Local>) -> Option<String> {
        if !self.show_timestamps {
            return None;
        }

        let format = match (self.use_24_hour_time, self.show_seconds) {
            (true, true) => "%H:%M:%S",
            (true, false) => "%H:%M",
            (false, true) => "%I:%M:%S",
            (false, false) => "%I:%M",
        };

        Some(timestamp.format(format).to_string())
    }
}

// ============================================================================
// Link Detection
// ============================================================================

/// Global link finder configured for URL detection (including schemeless URLs)
static LINK_FINDER: Lazy<LinkFinder> = Lazy::new(|| {
    let mut finder = LinkFinder::new();
    finder.kinds(&[LinkKind::Url]);
    finder.url_must_have_scheme(false);
    finder
});

/// A segment of text that may or may not be a link
#[derive(Debug)]
enum TextSegment<'a> {
    /// Plain text
    Text(&'a str),
    /// A URL that should be clickable
    Link(&'a str),
}

/// Split text into segments of plain text and URLs
fn split_into_segments(text: &str) -> Vec<TextSegment<'_>> {
    LINK_FINDER
        .spans(text)
        .map(|s| {
            if s.kind().is_some() {
                TextSegment::Link(s.as_str())
            } else {
                TextSegment::Text(s.as_str())
            }
        })
        .collect()
}

/// Build the URL to open when a link is clicked
///
/// If the URL doesn't have a scheme, prepend "https://"
fn make_openable_url(url: &str) -> String {
    if url.starts_with("http://") || url.starts_with("https://") {
        url.to_string()
    } else {
        format!("https://{}", url)
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Style parameters for rendering a chat message
struct MessageStyle {
    timestamp_color: Color,
    prefix_color: Color,
    content_color: Color,
    link_color: Color,
    font_size: f32,
}

/// Build a styled rich text message with consistent formatting and clickable links
fn styled_message<'a>(
    time_str: Option<&str>,
    prefix: String,
    content: &str,
    style: &MessageStyle,
) -> Element<'a, Message> {
    // Build spans dynamically to support clickable links
    let mut spans: Vec<iced::widget::text::Span<'a, String, Font>> = Vec::new();

    // Add timestamp if present
    if let Some(ts) = time_str {
        spans.push(span(format!("[{}] ", ts)).color(style.timestamp_color));
    }

    // Add prefix (username, [SYS], etc.)
    spans.push(span(prefix).color(style.prefix_color));

    // Add content with link detection
    for segment in split_into_segments(content) {
        match segment {
            TextSegment::Text(text) => {
                spans.push(span(text.to_string()).color(style.content_color));
            }
            TextSegment::Link(url) => {
                let openable_url = make_openable_url(url);
                spans.push(
                    span(url.to_string())
                        .color(style.link_color)
                        .link(openable_url),
                );
            }
        }
    }

    let text_widget: Rich<'a, String, Message> = rich_text(spans)
        .on_link_click(Message::OpenUrl)
        .size(style.font_size)
        .line_height(CHAT_LINE_HEIGHT)
        .font(MONOSPACE_FONT)
        .wrapping(Wrapping::WordOrGlyph)
        .width(Fill);

    text_widget.into()
}

/// Check if a nickname (display name) belongs to an admin in the online users list
///
/// Used for server chat messages where admin status isn't embedded in the message.
/// For private messages, use the `is_admin` field on `ChatMessage` instead.
fn is_admin_by_nickname(conn: &ServerConnection, nickname: &str) -> bool {
    conn.online_users
        .iter()
        .any(|u| u.nickname == nickname && u.is_admin)
}

/// Check if a nickname (display name) belongs to a shared account user.
///
/// For private messages, use the `is_shared` field on `ChatMessage` instead.
fn is_shared_by_nickname(conn: &ServerConnection, nickname: &str) -> bool {
    conn.online_users
        .iter()
        .any(|u| u.nickname == nickname && u.is_shared)
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
    if let ChatTab::UserMessage(ref nickname) = tab {
        let nickname_clone = nickname.clone();
        let close_button = tooltip(
            button(crate::icon::close().size(CHAT_MESSAGE_SIZE))
                .on_press(Message::CloseUserMessageTab(nickname_clone))
                .padding(CLOSE_BUTTON_PADDING)
                .style(close_button_on_primary_style()),
            container(
                shaped_text(format!("{} {}", t("tooltip-close"), nickname)).size(TOOLTIP_TEXT_SIZE),
            )
            .padding(TOOLTIP_BACKGROUND_PADDING)
            .style(tooltip_container_style),
            tooltip::Position::Bottom,
        )
        .gap(TOOLTIP_GAP)
        .padding(TOOLTIP_PADDING);

        let tab_content = row![shaped_text(label).size(CHAT_MESSAGE_SIZE), close_button]
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
        .style(iced::widget::button::secondary)
        .padding(INPUT_PADDING)
        .into()
}

// ============================================================================
// Message Rendering
// ============================================================================

/// Context for rendering a chat message line
struct MessageRenderContext<'a> {
    /// Formatted timestamp string (None if timestamps disabled)
    time_str: Option<String>,
    /// Display name of the sender (nickname)
    nickname: &'a str,
    /// The message line content
    line: &'a str,
    /// Type of message (Chat, System, Error, etc.)
    message_type: MessageType,
    /// Current theme for colors
    theme: &'a Theme,
    /// Whether the sender is an admin
    is_admin: bool,
    /// Whether the sender is a shared account user
    is_shared: bool,
    /// Font size for the message
    font_size: f32,
}

/// Build a rich text element for a single message line
fn render_message_line(ctx: MessageRenderContext<'_>) -> Element<'static, Message> {
    let timestamp_color = chat::timestamp(ctx.theme);
    let link_color = ctx.theme.palette().primary;

    match ctx.message_type {
        MessageType::System => {
            let color = chat::system(ctx.theme);
            let style = MessageStyle {
                timestamp_color,
                prefix_color: color,
                content_color: color,
                link_color,
                font_size: ctx.font_size,
            };
            styled_message(
                ctx.time_str.as_deref(),
                format!("{} ", t("chat-prefix-system")),
                ctx.line,
                &style,
            )
        }
        MessageType::Error => {
            let color = chat::error(ctx.theme);
            let style = MessageStyle {
                timestamp_color,
                prefix_color: color,
                content_color: color,
                link_color,
                font_size: ctx.font_size,
            };
            styled_message(
                ctx.time_str.as_deref(),
                format!("{} ", t("chat-prefix-error")),
                ctx.line,
                &style,
            )
        }
        MessageType::Info => {
            let color = chat::info(ctx.theme);
            let style = MessageStyle {
                timestamp_color,
                prefix_color: color,
                content_color: color,
                link_color,
                font_size: ctx.font_size,
            };
            styled_message(
                ctx.time_str.as_deref(),
                format!("{} ", t("chat-prefix-info")),
                ctx.line,
                &style,
            )
        }
        MessageType::Broadcast => {
            let color = chat::broadcast(ctx.theme);
            let style = MessageStyle {
                timestamp_color,
                prefix_color: color,
                content_color: color,
                link_color,
                font_size: ctx.font_size,
            };
            styled_message(
                ctx.time_str.as_deref(),
                format!("{} {}: ", t("chat-prefix-broadcast"), ctx.nickname),
                ctx.line,
                &style,
            )
        }
        MessageType::Chat => {
            let username_color = if ctx.is_admin {
                chat::admin(ctx.theme)
            } else if ctx.is_shared {
                chat::shared(ctx.theme)
            } else {
                chat::text(ctx.theme)
            };
            let text_color = chat::text(ctx.theme);
            let style = MessageStyle {
                timestamp_color,
                prefix_color: username_color,
                content_color: text_color,
                link_color,
                font_size: ctx.font_size,
            };
            styled_message(
                ctx.time_str.as_deref(),
                format!("{}: ", ctx.nickname),
                ctx.line,
                &style,
            )
        }
    }
}

// ============================================================================
// Message List
// ============================================================================

/// Build the message list column for the active chat tab
fn build_message_list<'a>(
    conn: &'a ServerConnection,
    theme: &Theme,
    font_size: f32,
    timestamp_settings: TimestampSettings,
) -> Column<'a, Message> {
    let messages = match &conn.active_chat_tab {
        ChatTab::Server => conn.chat_messages.as_slice(),
        ChatTab::UserMessage(nickname) => conn
            .user_messages
            .get(nickname)
            .map(|v| v.as_slice())
            .unwrap_or(&[]),
    };

    let mut chat_column = Column::new().spacing(CHAT_SPACING).padding(INPUT_PADDING);

    for msg in messages {
        let time_str = timestamp_settings.format(&msg.get_timestamp());
        // For private messages, use the stored is_admin/is_shared flags.
        // For server chat, fall back to looking up in online users.
        let sender_is_admin = if msg.is_admin {
            true
        } else {
            is_admin_by_nickname(conn, &msg.nickname)
        };
        let sender_is_shared = if msg.is_shared {
            true
        } else {
            is_shared_by_nickname(conn, &msg.nickname)
        };

        // Split message into lines to prevent spoofing via embedded newlines
        // Each line is displayed with the same timestamp/username prefix
        for line in msg.message.split('\n') {
            let display = render_message_line(MessageRenderContext {
                time_str: time_str.clone(),
                nickname: &msg.nickname,
                line,
                message_type: msg.message_type,
                theme,
                is_admin: sender_is_admin,
                is_shared: sender_is_shared,
                font_size,
            });
            chat_column = chat_column.push(display);
        }
    }

    chat_column
}

// ============================================================================
// Input Row
// ============================================================================

/// Build the message input row with text field and send button
fn build_input_row<'a>(message_input: &'a str, font_size: f32) -> iced::widget::Row<'a, Message> {
    let text_field = text_input(&t("placeholder-message"), message_input)
        .on_input(Message::ChatInputChanged)
        .on_submit(Message::SendMessagePressed)
        .id(Id::from(InputId::ChatInput))
        .padding(INPUT_PADDING)
        .size(font_size)
        .font(MONOSPACE_FONT)
        .width(Fill);

    let send_button = button(shaped_text(t("button-send")).size(font_size))
        .on_press(Message::SendMessagePressed)
        .padding(INPUT_PADDING);

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

    // PM tabs (sorted alphabetically by nickname/display name)
    let mut pm_nicknames: Vec<String> = conn.user_messages.keys().cloned().collect();
    pm_nicknames.sort();

    let has_pm_tabs = !pm_nicknames.is_empty();

    for nickname in pm_nicknames {
        let pm_tab = ChatTab::UserMessage(nickname.clone());
        let is_active = conn.active_chat_tab == pm_tab;
        let has_unread = conn.unread_tabs.contains(&pm_tab);
        let pm_tab_button = create_tab_button(pm_tab, nickname, is_active, has_unread);
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
    chat_font_size: u8,
    timestamp_settings: TimestampSettings,
) -> Element<'a, Message> {
    let font_size = chat_font_size as f32;

    // Build tab bar
    let (tab_row, has_pm_tabs) = build_tab_bar(conn);
    let tab_bar = tab_row.wrap();

    // Build message list
    let chat_column = build_message_list(conn, &theme, font_size, timestamp_settings);

    let chat_scrollable = scrollable(chat_column)
        .id(ScrollableId::ChatMessages)
        .on_scroll(Message::ChatScrolled)
        .direction(Direction::Vertical(Scrollbar::default()))
        .width(Fill)
        .height(Fill);

    // Build input row (always enabled - permission checked on send)
    let input_row = build_input_row(message_input, font_size);

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
