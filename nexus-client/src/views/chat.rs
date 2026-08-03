//! Chat interface for active server connections

use std::hash::{Hash, Hasher};

use iced::widget::scrollable::{Direction, Scrollbar};
use iced::widget::text::Wrapping;
use iced::widget::{
    Column, button, column, container, lazy, rich_text, row, scrollable, selectable_group, span,
    text::Rich, tooltip,
};
use iced::{Color, Element, Fill, Font, Theme};
use linkify::{LinkFinder, LinkKind};
use nexus_common::names::fold_name;
use nexus_common::protocol::ChatAction;
use once_cell::sync::Lazy;

use super::helpers::hash_color;
use crate::i18n::t;
use crate::network::FEATURE_VOICE;
use crate::style::{
    BOLD_FONT, CHAT_ACTION_PREFIX, CHAT_LINE_HEIGHT, CHAT_MESSAGE_SEPARATOR, CHAT_MESSAGE_SIZE,
    CHAT_SPACING, CLOSE_BUTTON_PADDING, INPUT_PADDING, MONOSPACE_FONT, MONOSPACE_ITALIC_FONT,
    SMALL_PADDING, SMALL_SPACING, TAB_CONTENT_PADDING, TOOLTIP_BACKGROUND_PADDING, TOOLTIP_GAP,
    TOOLTIP_PADDING, TOOLTIP_TEXT_SIZE, chat, chat_tab_active_style, close_button_on_primary_style,
    content_background_style, shaped_text, tooltip_container_style,
};
use crate::types::{ChatMessage, ChatTab, Message, MessageType, ScrollableId, ServerConnection};
use crate::views::constants::PERMISSION_VOICE_LISTEN;
use crate::views::voice::{build_input_row_with_voice, build_voice_bar};

const CONSOLE_TAB_TOOLTIP_KEY: &str = "console-tab";

// ============================================================================
// chat_view argument bundles
// ============================================================================

/// Voice-state arguments to [`chat_view`]. Bundled so the function
/// signature stays narrow.
pub struct ChatViewVoice {
    /// Active voice target (`#channel` or `nickname`), if any.
    pub target: Option<String>,
    /// Whether the local user is currently transmitting voice.
    pub is_local_speaking: bool,
    /// Whether the local user is deafened (output muted).
    pub is_deafened: bool,
    /// Microphone level for the input meter (0.0–1.0).
    pub mic_level: f32,
}

/// Presentation arguments to [`chat_view`]: theme + font / timestamp
/// settings.
pub struct ChatViewStyle {
    pub theme: Theme,
    pub font_size: u8,
    pub timestamps: TimestampSettings,
}

// ============================================================================
// Timestamp Settings
// ============================================================================

/// Settings for timestamp display in chat messages
#[derive(Debug, Clone, Copy, Hash)]
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
/// If the URL doesn't have a scheme, prepend "https://".
/// nexus:// URIs are preserved as-is for internal handling.
fn make_openable_url(url: &str) -> String {
    if crate::uri::is_nexus_uri(url) || crate::uri::is_allowed_external_url(url) {
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
    /// Use italic font for content (action messages)
    italic: bool,
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

    // Choose font based on italic flag (for action messages)
    let text_font = if style.italic {
        MONOSPACE_ITALIC_FONT
    } else {
        MONOSPACE_FONT
    };

    // Add timestamp if present
    if let Some(ts) = time_str {
        spans.push(span(format!("[{}] ", ts)).color(style.timestamp_color));
    }

    // Add prefix (username, [SYS], etc.) - uses italic for action messages
    spans.push(span(prefix).color(style.prefix_color).font(text_font));

    // Add content with link detection
    for segment in split_into_segments(content) {
        match segment {
            TextSegment::Text(text) => {
                spans.push(
                    span(text.to_string())
                        .color(style.content_color)
                        .font(text_font),
                );
            }
            TextSegment::Link(url) => {
                let openable_url = make_openable_url(url);
                spans.push(
                    span(url.to_string())
                        .color(style.link_color)
                        .font(text_font)
                        .link(openable_url),
                );
            }
        }
    }

    let text_widget: Rich<'a, String, Message> = rich_text(spans)
        .on_link_click(Message::OpenUrl)
        .selectable(true)
        .size(style.font_size)
        .line_height(CHAT_LINE_HEIGHT)
        .font(MONOSPACE_FONT)
        .wrapping(Wrapping::WordOrGlyph)
        .width(Fill);

    text_widget.into()
}

// ============================================================================
// Lazy Dependencies
// ============================================================================

/// Theme colors baked into message rich-text spans at build time (spans
/// require concrete colors — there is no draw-time style closure for them).
/// Hashed by exact color bits so any palette change rebuilds.
#[derive(Clone)]
struct ChatColors {
    timestamp: Color,
    system: Color,
    error: Color,
    info: Color,
    broadcast: Color,
    admin: Color,
    shared: Color,
    text: Color,
    link: Color,
}

impl ChatColors {
    fn from_theme(theme: &Theme) -> Self {
        Self {
            timestamp: chat::timestamp(theme),
            system: chat::system(theme),
            error: chat::error(theme),
            info: chat::info(theme),
            broadcast: chat::broadcast(theme),
            admin: chat::admin(theme),
            shared: chat::shared(theme),
            text: chat::text(theme),
            link: theme.palette().primary,
        }
    }
}

impl Hash for ChatColors {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Full destructure: adding a field fails to compile until it is
        // explicitly hashed here.
        let Self {
            timestamp,
            system,
            error,
            info,
            broadcast,
            admin,
            shared,
            text,
            link,
        } = self;
        hash_color(*timestamp, state);
        hash_color(*system, state);
        hash_color(*error, state);
        hash_color(*info, state);
        hash_color(*broadcast, state);
        hash_color(*admin, state);
        hash_color(*shared, state);
        hash_color(*text, state);
        hash_color(*link, state);
    }
}

/// Dependencies for the lazy message list.
///
/// Borrows the active tab's messages directly: a cache-hit frame costs one
/// hash pass over the tab's content with no allocations, and the render
/// closure only runs (cloning into `'static` rich text, as every frame did
/// before the cache) when the hash changes. The closure reads only this
/// struct, so it cannot depend on state that isn't hashed. The i18n locale
/// is fixed at launch, so the `t()` prefixes inside the closure cannot go
/// stale.
///
/// Known trade-off: the hash is linear in the tab's history size on every
/// `view()` call (histories are uncapped). That is orders of magnitude
/// cheaper than the pre-cache full rebuild + linkify over the same history;
/// if profiling ever shows it mattering, the escalation path is an
/// append-time rolling hash or list virtualization — not a partial hash,
/// which would reintroduce stale rendering.
struct ChatMessagesDeps<'a> {
    messages: &'a [ChatMessage],
    font_size: u8,
    timestamps: TimestampSettings,
    colors: ChatColors,
}

impl Hash for ChatMessagesDeps<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Full destructure of the deps and of every message: adding a field
        // to either struct fails to compile until it is hashed or excluded.
        let Self {
            messages,
            font_size,
            timestamps,
            colors,
        } = self;
        messages.len().hash(state);
        for ChatMessage {
            nickname,
            message,
            message_type,
            timestamp,
            is_admin,
            is_shared,
            action,
        } in messages.iter()
        {
            nickname.hash(state);
            message.hash(state);
            message_type.hash(state);
            timestamp
                .as_ref()
                .map(|t| (t.timestamp(), t.timestamp_subsec_nanos()))
                .hash(state);
            is_admin.hash(state);
            is_shared.hash(state);
            action.hash(state);
        }
        font_size.hash(state);
        timestamps.hash(state);
        colors.hash(state);
    }
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

/// Create an active tab button (with close button for channel and user message tabs)
fn create_active_tab_button(tab: ChatTab, label: String) -> Element<'static, Message> {
    match &tab {
        ChatTab::Channel(channel) => {
            // Channel tabs include a close button
            let channel_clone = channel.clone();
            let close_button = tooltip(
                button(crate::icon::close().size(CHAT_MESSAGE_SIZE))
                    .on_press(Message::CloseChannelTab(channel_clone))
                    .padding(CLOSE_BUTTON_PADDING)
                    .style(close_button_on_primary_style()),
                container(
                    shaped_text(format!("{} {}", t("tooltip-close"), channel))
                        .size(TOOLTIP_TEXT_SIZE),
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
        }
        ChatTab::UserMessage(nickname) => {
            // User message tabs include a close button
            let nickname_clone = nickname.clone();
            let close_button = tooltip(
                button(crate::icon::close().size(CHAT_MESSAGE_SIZE))
                    .on_press(Message::CloseUserMessageTab(nickname_clone))
                    .padding(CLOSE_BUTTON_PADDING)
                    .style(close_button_on_primary_style()),
                container(
                    shaped_text(format!("{} {}", t("tooltip-close"), nickname))
                        .size(TOOLTIP_TEXT_SIZE),
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
        }
        ChatTab::Console => create_console_tab_button(tab, true),
    }
}

/// Create a console tab button (icon-only with tooltip)
fn create_console_tab_button(tab: ChatTab, is_active: bool) -> Element<'static, Message> {
    let tooltip_text = t(CONSOLE_TAB_TOOLTIP_KEY);
    let icon = crate::icon::terminal().size(CHAT_MESSAGE_SIZE);

    let button_style = if is_active {
        chat_tab_active_style()
    } else {
        iced::widget::button::secondary
    };

    tooltip(
        button(icon)
            .on_press(Message::SwitchChatTab(tab))
            .padding(INPUT_PADDING)
            .style(button_style),
        container(shaped_text(tooltip_text).size(TOOLTIP_TEXT_SIZE))
            .padding(TOOLTIP_BACKGROUND_PADDING)
            .style(tooltip_container_style),
        tooltip::Position::Bottom,
    )
    .gap(TOOLTIP_GAP)
    .padding(TOOLTIP_PADDING)
    .into()
}

/// Create an inactive tab button (bold if unread)
fn create_inactive_tab_button(
    tab: ChatTab,
    label: String,
    has_unread: bool,
) -> Element<'static, Message> {
    match &tab {
        ChatTab::Console => create_console_tab_button(tab, false),
        _ => {
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
    }
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
    /// Baked theme colors from the dependency struct
    colors: &'a ChatColors,
    /// Whether the sender is an admin
    is_admin: bool,
    /// Whether the sender is a shared account user
    is_shared: bool,
    /// Font size for the message
    font_size: f32,
    /// Action type for chat messages (Normal or Me)
    action: ChatAction,
}

/// Build a rich text element for a single message line
fn render_message_line(ctx: MessageRenderContext<'_>) -> Element<'static, Message> {
    let timestamp_color = ctx.colors.timestamp;
    let link_color = ctx.colors.link;

    match ctx.message_type {
        MessageType::System => {
            let color = ctx.colors.system;
            let style = MessageStyle {
                timestamp_color,
                prefix_color: color,
                content_color: color,
                link_color,
                font_size: ctx.font_size,
                italic: false,
            };
            styled_message(
                ctx.time_str.as_deref(),
                format!("{} ", t("chat-prefix-system")),
                ctx.line,
                &style,
            )
        }
        MessageType::Error => {
            let color = ctx.colors.error;
            let style = MessageStyle {
                timestamp_color,
                prefix_color: color,
                content_color: color,
                link_color,
                font_size: ctx.font_size,
                italic: false,
            };
            styled_message(
                ctx.time_str.as_deref(),
                format!("{} ", t("chat-prefix-error")),
                ctx.line,
                &style,
            )
        }
        MessageType::Info => {
            let color = ctx.colors.info;
            let style = MessageStyle {
                timestamp_color,
                prefix_color: color,
                content_color: color,
                link_color,
                font_size: ctx.font_size,
                italic: false,
            };
            styled_message(
                ctx.time_str.as_deref(),
                format!("{} ", t("chat-prefix-info")),
                ctx.line,
                &style,
            )
        }
        MessageType::Broadcast => {
            let color = ctx.colors.broadcast;
            let style = MessageStyle {
                timestamp_color,
                prefix_color: color,
                content_color: color,
                link_color,
                font_size: ctx.font_size,
                italic: false,
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
                ctx.colors.admin
            } else if ctx.is_shared {
                ctx.colors.shared
            } else {
                ctx.colors.text
            };
            let text_color = ctx.colors.text;

            // Handle action messages (/me)
            let (prefix, is_action) = match ctx.action {
                ChatAction::Normal => {
                    (format!("{}{}", ctx.nickname, CHAT_MESSAGE_SEPARATOR), false)
                }
                ChatAction::Me => (format!("{}{} ", CHAT_ACTION_PREFIX, ctx.nickname), true),
            };

            let style = MessageStyle {
                timestamp_color,
                prefix_color: username_color,
                content_color: text_color,
                link_color,
                font_size: ctx.font_size,
                italic: is_action,
            };
            styled_message(ctx.time_str.as_deref(), prefix, ctx.line, &style)
        }
    }
}

// ============================================================================
// Message List
// ============================================================================

/// Messages of the active chat tab
fn active_tab_messages(conn: &ServerConnection) -> &[ChatMessage] {
    match &conn.active_chat_tab {
        ChatTab::Console => conn.console_messages.as_slice(),
        ChatTab::Channel(channel) => {
            let channel_lower = fold_name(channel);
            conn.channels
                .get(&channel_lower)
                .map(|ch| ch.messages.as_slice())
                .unwrap_or(&[])
        }
        ChatTab::UserMessage(nickname) => conn
            .user_messages_for(nickname)
            .map(|v| v.as_slice())
            .unwrap_or(&[]),
    }
}

/// Render the message list column from resolved deps only (called inside
/// `lazy()`).
fn render_message_list(deps: &ChatMessagesDeps<'_>) -> Element<'static, Message> {
    let font_size = f32::from(deps.font_size);
    let mut chat_column = Column::new().spacing(CHAT_SPACING).padding(INPUT_PADDING);

    for msg in deps.messages {
        let time_str = deps.timestamps.format(&msg.get_timestamp());

        // Split message into lines to prevent spoofing via embedded newlines
        // Each line is displayed with the same timestamp/username prefix
        for line in msg.message.split('\n') {
            let display = render_message_line(MessageRenderContext {
                time_str: time_str.clone(),
                nickname: &msg.nickname,
                line,
                message_type: msg.message_type,
                colors: &deps.colors,
                is_admin: msg.is_admin,
                is_shared: msg.is_shared,
                font_size,
                action: msg.action,
            });
            chat_column = chat_column.push(display);
        }
    }

    // Coordinate drag-selection across all message lines so one drag
    // (and Ctrl+C) can span multiple messages. `Link = String` matches
    // the `Rich<'_, String, Message>` widgets built above.
    selectable_group::<String, _, _, _>(chat_column).into()
}

// ============================================================================
// Tab Bar
// ============================================================================

/// Build the tab bar with Console, channel, and user message tabs
fn build_tab_bar(conn: &ServerConnection) -> (iced::widget::Row<'static, Message>, bool) {
    let mut tab_row = row![].spacing(SMALL_SPACING);

    // Console tab (always present, cannot be closed)
    let is_console_active = conn.active_chat_tab == ChatTab::Console;
    let console_has_unread = conn.unread_tabs.contains(&ChatTab::Console);
    let console_tab_button = create_tab_button(
        ChatTab::Console,
        t("console-tab"),
        is_console_active,
        console_has_unread,
    );
    tab_row = tab_row.push(console_tab_button);

    // Channel tabs (in join order)
    for channel in &conn.channel_tabs {
        let channel_tab = ChatTab::Channel(channel.clone());
        let is_active = conn.active_chat_tab == channel_tab;
        let has_unread = conn.unread_tabs.contains(&channel_tab);
        let channel_tab_button =
            create_tab_button(channel_tab, channel.clone(), is_active, has_unread);
        tab_row = tab_row.push(channel_tab_button);
    }

    // User message tabs (in creation order)
    let has_pm_tabs = !conn.user_message_tabs.is_empty();

    for nickname in &conn.user_message_tabs {
        let pm_tab = ChatTab::UserMessage(nickname.clone());
        let is_active = conn.active_chat_tab == pm_tab;
        let has_unread = conn.unread_tabs.contains(&pm_tab);
        let pm_tab_button = create_tab_button(pm_tab, nickname.clone(), is_active, has_unread);
        tab_row = tab_row.push(pm_tab_button);
    }

    // Has closeable tabs if there are channels or PMs
    let has_closeable_tabs = !conn.channel_tabs.is_empty() || has_pm_tabs;

    (tab_row, has_closeable_tabs)
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
/// The send input is not permission-gated; send permission is enforced per tab
/// at send time (chat_send for channels, user_message for DMs) and by the server.
///
/// # Voice Chat Integration
///
/// A voice button appears in the input row when the user has voice permissions
/// and is on a channel or user message tab. When in a voice session, a voice bar
/// appears above the input showing the target and participant count.
pub fn chat_view<'a>(
    conn: &'a ServerConnection,
    message_input: &'a str,
    style: ChatViewStyle,
    voice: ChatViewVoice,
) -> Element<'a, Message> {
    let ChatViewStyle {
        theme,
        font_size,
        timestamps,
    } = style;
    let ChatViewVoice {
        target,
        is_local_speaking,
        is_deafened,
        mic_level,
    } = voice;
    // Build tab bar
    let (tab_row, has_closeable_tabs) = build_tab_bar(conn);
    let tab_bar = tab_row.wrap();

    // Lazy message list: the column is rebuilt only when the dependency
    // hash changes. The scrollable stays outside the cache so its id,
    // on_scroll, and snap-to-bottom behavior are untouched.
    let deps = ChatMessagesDeps {
        messages: active_tab_messages(conn),
        font_size,
        timestamps,
        colors: ChatColors::from_theme(&theme),
    };
    let chat_scrollable = scrollable(lazy(deps, |deps| render_message_list(deps)))
        .id(ScrollableId::ChatMessages)
        .on_scroll(Message::ChatScrolled)
        .direction(Direction::Vertical(Scrollbar::default()))
        .width(Fill)
        .height(Fill);

    // Downstream call sites need the font size as f32; shadow with the
    // converted value so we don't carry a `_f32` suffix everywhere.
    let font_size = f32::from(font_size);

    // `voice` gates client support, `voice_listen` gates joining/listening,
    // and `voice_talk` only gates transmit.
    let has_voice_permission =
        conn.has_feature(FEATURE_VOICE) && conn.has_permission(PERMISSION_VOICE_LISTEN);

    // Build input row with voice button
    let input_row =
        build_input_row_with_voice(message_input, font_size, conn, has_voice_permission, target);

    // Build the bottom section (voice bar + input row)
    let bottom_section = if let Some(ref session) = conn.voice_session {
        // Show voice bar above input when in a voice session
        let voice_bar = build_voice_bar(session, is_local_speaking, is_deafened, mic_level, &theme);
        column![voice_bar, input_row]
            .spacing(SMALL_SPACING)
            .width(Fill)
    } else {
        column![input_row].width(Fill)
    };

    // Chat content with background
    let chat_content = container(
        column![chat_scrollable, bottom_section]
            .spacing(SMALL_SPACING)
            .padding(SMALL_PADDING),
    )
    .width(Fill)
    .height(Fill)
    .style(content_background_style);

    // Only show tab bar if there are closeable tabs (channels or PMs)
    if has_closeable_tabs {
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

#[cfg(test)]
mod tests {
    use std::hash::DefaultHasher;

    use chrono::{DateTime, Local};

    use super::*;

    #[test]
    fn make_openable_url_preserves_allowed_schemes() {
        assert_eq!(
            make_openable_url("http://example.com"),
            "http://example.com"
        );
        assert_eq!(
            make_openable_url("https://example.com"),
            "https://example.com"
        );
        assert_eq!(
            make_openable_url("ftp://example.com/file"),
            "ftp://example.com/file"
        );
        assert_eq!(
            make_openable_url("ftps://example.com/file"),
            "ftps://example.com/file"
        );
        assert_eq!(
            make_openable_url("sftp://example.com/file"),
            "sftp://example.com/file"
        );
        assert_eq!(
            make_openable_url("nexus://example.com/news"),
            "nexus://example.com/news"
        );
    }

    #[test]
    fn make_openable_url_defaults_schemeless_to_https() {
        assert_eq!(make_openable_url("example.com"), "https://example.com");
    }

    // ------------------------------------------------------------------
    // Deps hash sensitivity
    //
    // The lazy() cache reuses the message-list widget tree whenever the
    // deps hash is unchanged, so every rendered fact must move the hash.
    // Each case mutates one dependency and asserts the hash changes.
    // ------------------------------------------------------------------

    fn ts(secs: i64) -> DateTime<Local> {
        DateTime::from_timestamp(secs, 0)
            .expect("valid test timestamp")
            .with_timezone(&Local)
    }

    fn test_message(text: &str) -> ChatMessage {
        ChatMessage {
            nickname: "alice".to_string(),
            message: text.to_string(),
            message_type: MessageType::Chat,
            timestamp: Some(ts(1_700_000_000)),
            is_admin: false,
            is_shared: false,
            action: ChatAction::Normal,
        }
    }

    fn test_timestamp_settings() -> TimestampSettings {
        TimestampSettings {
            show_timestamps: true,
            use_24_hour_time: true,
            show_seconds: false,
        }
    }

    fn hash_deps(
        messages: &[ChatMessage],
        font_size: u8,
        timestamps: TimestampSettings,
        colors: &ChatColors,
    ) -> u64 {
        let deps = ChatMessagesDeps {
            messages,
            font_size,
            timestamps,
            colors: colors.clone(),
        };
        let mut hasher = DefaultHasher::new();
        deps.hash(&mut hasher);
        hasher.finish()
    }

    #[test]
    fn deps_hash_changes_on_message_content() {
        let colors = ChatColors::from_theme(&Theme::Dark);
        let settings = test_timestamp_settings();
        let base = vec![test_message("hello"), test_message("world")];
        let base_hash = hash_deps(&base, 14, settings, &colors);

        // Message text
        let mut messages = base.clone();
        messages[0].message = "hello!".to_string();
        assert_ne!(
            hash_deps(&messages, 14, settings, &colors),
            base_hash,
            "message text"
        );

        // Sender nickname
        let mut messages = base.clone();
        messages[0].nickname = "bob".to_string();
        assert_ne!(
            hash_deps(&messages, 14, settings, &colors),
            base_hash,
            "nickname"
        );

        // Message type (rendering style)
        let mut messages = base.clone();
        messages[0].message_type = MessageType::System;
        assert_ne!(
            hash_deps(&messages, 14, settings, &colors),
            base_hash,
            "message type"
        );

        // Sender admin/shared flags (nickname color)
        let mut messages = base.clone();
        messages[0].is_admin = true;
        assert_ne!(
            hash_deps(&messages, 14, settings, &colors),
            base_hash,
            "admin flag"
        );
        let mut messages = base.clone();
        messages[1].is_shared = true;
        assert_ne!(
            hash_deps(&messages, 14, settings, &colors),
            base_hash,
            "shared flag"
        );

        // Action (/me formatting)
        let mut messages = base.clone();
        messages[0].action = ChatAction::Me;
        assert_ne!(
            hash_deps(&messages, 14, settings, &colors),
            base_hash,
            "action"
        );

        // Timestamp (rendered prefix)
        let mut messages = base.clone();
        messages[0].timestamp = Some(ts(1_700_000_060));
        assert_ne!(
            hash_deps(&messages, 14, settings, &colors),
            base_hash,
            "timestamp"
        );

        // Message added / removed
        let mut messages = base.clone();
        messages.push(test_message("third"));
        assert_ne!(
            hash_deps(&messages, 14, settings, &colors),
            base_hash,
            "message added"
        );
        let mut messages = base.clone();
        messages.pop();
        assert_ne!(
            hash_deps(&messages, 14, settings, &colors),
            base_hash,
            "message removed"
        );
    }

    #[test]
    fn deps_hash_changes_on_presentation_settings() {
        let colors = ChatColors::from_theme(&Theme::Dark);
        let settings = test_timestamp_settings();
        let base = vec![test_message("hello")];
        let base_hash = hash_deps(&base, 14, settings, &colors);

        // Font size
        assert_ne!(
            hash_deps(&base, 16, settings, &colors),
            base_hash,
            "font size"
        );

        // Each timestamp setting
        let mut changed = settings;
        changed.show_timestamps = false;
        assert_ne!(
            hash_deps(&base, 14, changed, &colors),
            base_hash,
            "show_timestamps"
        );
        let mut changed = settings;
        changed.use_24_hour_time = false;
        assert_ne!(
            hash_deps(&base, 14, changed, &colors),
            base_hash,
            "use_24_hour_time"
        );
        let mut changed = settings;
        changed.show_seconds = true;
        assert_ne!(
            hash_deps(&base, 14, changed, &colors),
            base_hash,
            "show_seconds"
        );

        // Theme color change
        let mut changed_colors = colors.clone();
        changed_colors.link = Color::from_rgb(0.1, 0.2, 0.3);
        assert_ne!(
            hash_deps(&base, 14, settings, &changed_colors),
            base_hash,
            "link color"
        );
    }
}
