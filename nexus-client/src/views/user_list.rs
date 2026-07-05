//! User list panel (right sidebar)
//!
//! Shows contextual user list based on the active chat tab:
//! - Console tab: All online users
//! - Channel tab: Only channel members
//! - User message tab: You + the other user (or just you if they're offline)
//!
//! Voice indicators:
//! - Headphones icon: User is in voice for the visible channel/DM
//! - Speaker icon with highlight: User is currently speaking
//! - Mute button: Client-side mute (stops hearing that user)
//!
//! The whole sidebar is wrapped in `lazy()` behind [`UserListDeps`]: every
//! rendered fact is resolved into the deps up front, so the render closure
//! cannot read state that isn't hashed. Iced calls `view()` on every message
//! (each keystroke, each incoming chat line); without the cache, every user
//! row (button, tooltip, rich-text spans, avatar handle) is rebuilt each time.

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

use iced::widget::text::Wrapping;
use iced::widget::{
    Column, Row, Space, button, column, container, lazy, rich_text, row, scrollable, span, tooltip,
};
use iced::{Center, Color, Element, Fill, Font, Theme};
use nexus_common::names::fold_name;

use super::constants::{
    PERMISSION_BAN_CREATE, PERMISSION_USER_INFO, PERMISSION_USER_KICK, PERMISSION_USER_MESSAGE,
    PERMISSION_VOICE_LISTEN,
};
use crate::avatar::{avatar_cache_key, generate_identicon};
use crate::i18n::t;
use crate::icon;
use crate::image::CachedImage;
use crate::network::FEATURE_CHAT;
use crate::style::{
    CONTENT_PADDING, HEADING_BUTTON_PADDING, ICON_SIZE, INPUT_PADDING, NO_SPACING,
    SCROLLBAR_PADDING, SEPARATOR_HEIGHT, TOOLBAR_CONTAINER_PADDING, TOOLTIP_BACKGROUND_PADDING,
    TOOLTIP_GAP, TOOLTIP_PADDING, TOOLTIP_TEXT_SIZE, USER_LIST_AVATAR_SIZE,
    USER_LIST_AVATAR_SPACING, USER_LIST_ITEM_SPACING, USER_LIST_PANEL_WIDTH,
    USER_LIST_SMALL_TEXT_SIZE, USER_LIST_SPACING, USER_LIST_TEXT_SIZE, USER_LIST_TITLE_SIZE,
    alternating_row_style, chat, clickable_text_style, disabled_icon_button_style,
    icon_button_with_hover_style, muted_text_style, shaped_text, sidebar_panel_style,
    tooltip_container_style, ui, user_toolbar_separator_style,
};
use crate::types::ActivePanel;
use crate::types::{ChatTab, Message, ServerConnection, UserInfo, VoiceState};

// ============================================================================
// Helper Functions
// ============================================================================

/// Create an icon container with consistent sizing and alignment
fn icon_container(icon: iced::widget::Text<'_>) -> iced::widget::Container<'_, Message> {
    container(icon.size(ICON_SIZE))
        .width(ICON_SIZE)
        .height(ICON_SIZE)
        .align_x(iced::alignment::Horizontal::Center)
        .align_y(iced::alignment::Vertical::Center)
}

/// Container for mute/volume icons that aligns left to prevent icon shifting
fn mute_icon_container(icon: iced::widget::Text<'_>) -> iced::widget::Container<'_, Message> {
    container(icon.size(ICON_SIZE))
        .width(ICON_SIZE)
        .height(ICON_SIZE)
        .align_x(iced::alignment::Horizontal::Left)
        .align_y(iced::alignment::Vertical::Center)
}

/// Create an enabled icon button with hover effect
fn enabled_icon_button<'a>(
    icon: iced::widget::Container<'a, Message>,
    message: Message,
    hover_color: Color,
    normal_color: Color,
) -> button::Button<'a, Message> {
    button(icon)
        .on_press(message)
        .padding(HEADING_BUTTON_PADDING)
        .style(icon_button_with_hover_style(hover_color, normal_color))
}

/// Create a disabled icon button (greyed out)
fn disabled_icon_button(icon: iced::widget::Container<'_, Message>) -> button::Button<'_, Message> {
    button(icon)
        .padding(HEADING_BUTTON_PADDING)
        .style(disabled_icon_button_style)
}

/// Wrap a button in a tooltip
fn with_tooltip<'a>(
    btn: button::Button<'a, Message>,
    tooltip_text: String,
) -> tooltip::Tooltip<'a, Message> {
    tooltip(
        btn,
        container(shaped_text(tooltip_text).size(TOOLTIP_TEXT_SIZE))
            .padding(TOOLTIP_BACKGROUND_PADDING)
            .style(tooltip_container_style),
        tooltip::Position::Bottom,
    )
    .gap(TOOLTIP_GAP)
    .padding(TOOLTIP_PADDING)
}

/// Create a horizontal separator line (primary color)
fn toolbar_separator<'a>() -> iced::widget::Container<'a, Message> {
    container(Space::new().width(Fill).height(SEPARATOR_HEIGHT))
        .width(Fill)
        .height(SEPARATOR_HEIGHT)
        .style(user_toolbar_separator_style)
}

fn visible_voice_target(active_chat_tab: &ChatTab) -> Option<String> {
    match active_chat_tab {
        ChatTab::Channel(name) | ChatTab::UserMessage(name) => Some(fold_name(name)),
        ChatTab::Console => None,
    }
}

fn active_voice_session_contains(
    active_chat_tab: &ChatTab,
    voice_session: Option<&VoiceState>,
    nickname_lower: &str,
) -> bool {
    let Some(tab_target) = visible_voice_target(active_chat_tab) else {
        return false;
    };

    voice_session.is_some_and(|session| {
        fold_name(&session.target) == tab_target
            && session
                .participants
                .iter()
                .any(|participant| fold_name(participant) == nickname_lower)
    })
}

fn is_user_in_visible_voice(
    active_chat_tab: &ChatTab,
    voice_session: Option<&VoiceState>,
    channel_voiced: &HashMap<String, HashSet<String>>,
    user_message_voiced: &HashSet<String>,
    nickname: &str,
) -> bool {
    let nickname_lower = fold_name(nickname);
    let tracked_presence = match active_chat_tab {
        ChatTab::Channel(channel_name) => channel_voiced
            .get(&fold_name(channel_name))
            .is_some_and(|users| users.contains(&nickname_lower)),
        ChatTab::UserMessage(other_nickname) => {
            fold_name(other_nickname) == nickname_lower
                && user_message_voiced.contains(&nickname_lower)
        }
        ChatTab::Console => false,
    };

    tracked_presence
        || active_voice_session_contains(active_chat_tab, voice_session, &nickname_lower)
}

fn is_user_speaking_in_visible_voice(
    active_chat_tab: &ChatTab,
    voice_session: Option<&VoiceState>,
    nickname: &str,
) -> bool {
    let Some(tab_target) = visible_voice_target(active_chat_tab) else {
        return false;
    };

    voice_session.is_some_and(|session| {
        fold_name(&session.target) == tab_target && session.is_speaking(nickname)
    })
}

// ============================================================================
// User Filtering
// ============================================================================

/// Get the list of users to display based on the active view
///
/// Returns a filtered and sorted list of users:
/// - Panel open (Files, News, etc.): All online users
/// - Console: All online users
/// - Channel: Only channel members (matched by nickname, case-insensitive)
/// - User message: You + the other user (if they're online)
fn get_contextual_users(conn: &ServerConnection) -> Vec<&UserInfo> {
    // When a panel is open, always show all users
    if conn.active_panel != ActivePanel::None {
        return conn.online_users.iter().collect();
    }

    // Use server-confirmed nickname
    let current_nickname = &conn.nickname;

    match &conn.active_chat_tab {
        ChatTab::Console => {
            // Show all online users
            conn.online_users.iter().collect()
        }
        ChatTab::Channel(channel_name) => {
            // Show only channel members
            if let Some(channel_state) = conn.get_channel_state(channel_name) {
                // Filter online_users to only those who are members of this channel
                conn.online_users
                    .iter()
                    .filter(|user| {
                        let user_nickname_lower = fold_name(&user.nickname);
                        channel_state
                            .members
                            .iter()
                            .any(|m| fold_name(m) == user_nickname_lower)
                    })
                    .collect()
            } else {
                // Channel not found, show empty list
                Vec::new()
            }
        }
        ChatTab::UserMessage(other_nickname) => {
            // Show you + the other user (if they're online)
            let other_lower = fold_name(other_nickname);
            let current_lower = fold_name(current_nickname);

            conn.online_users
                .iter()
                .filter(|user| {
                    let nickname_lower = fold_name(&user.nickname);
                    nickname_lower == current_lower || nickname_lower == other_lower
                })
                .collect()
        }
    }
}

/// Get the title for the user list based on the active view
fn get_user_list_title(conn: &ServerConnection) -> String {
    // When a panel is open, show generic "Users" title
    if conn.active_panel != ActivePanel::None {
        return t("title-users");
    }

    match &conn.active_chat_tab {
        ChatTab::Console => t("title-users"),
        ChatTab::Channel(_) | ChatTab::UserMessage(_) => t("title-channel-members"),
    }
}

/// Build tooltip text for a user, including away/status information
fn build_user_tooltip(nickname: &str, is_away: bool, status: Option<&str>) -> String {
    match (is_away, status) {
        (true, Some(status_msg)) => format!("{} 💤\n{}", nickname, status_msg),
        (true, None) => format!("{} 💤", nickname),
        (false, Some(status_msg)) => format!("{}\n{}", nickname, status_msg),
        (false, None) => nickname.to_string(),
    }
}

// ============================================================================
// Lazy Dependencies
// ============================================================================

/// Theme colors baked into the widget tree at build time (rich-text span
/// colors and the parameterized button/text styles). Hashed by exact color
/// bits so any palette change — including edits to a custom theme — rebuilds.
/// Styles that take the theme as a closure argument are evaluated at draw
/// time and need no dependency here.
#[derive(Clone)]
struct ThemeColors {
    admin: Color,
    shared: Color,
    speaking: Color,
    muted: Color,
    icon: Color,
    primary: Color,
    danger: Color,
}

impl ThemeColors {
    fn from_theme(theme: &Theme) -> Self {
        Self {
            admin: chat::admin(theme),
            shared: chat::shared(theme),
            speaking: theme.extended_palette().success.base.color,
            muted: ui::muted_text_color(theme),
            icon: ui::icon_color(theme),
            primary: theme.palette().primary,
            danger: theme.palette().danger,
        }
    }
}

fn hash_color<H: Hasher>(color: &Color, state: &mut H) {
    color.r.to_bits().hash(state);
    color.g.to_bits().hash(state);
    color.b.to_bits().hash(state);
    color.a.to_bits().hash(state);
}

impl Hash for ThemeColors {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Full destructure: adding a field fails to compile until it is
        // explicitly hashed here.
        let Self {
            admin,
            shared,
            speaking,
            muted,
            icon,
            primary,
            danger,
        } = self;
        hash_color(admin, state);
        hash_color(shared, state);
        hash_color(speaking, state);
        hash_color(muted, state);
        hash_color(icon, state);
        hash_color(primary, state);
        hash_color(danger, state);
    }
}

/// One rendered sidebar row, with every per-row fact resolved.
#[derive(Clone)]
struct UserRowDeps {
    nickname: String,
    is_admin: bool,
    is_shared: bool,
    is_away: bool,
    status: Option<String>,
    /// Change-detection proxy for `avatar` — see the manual `Hash` impl.
    avatar_hash: Option<[u8; 32]>,
    /// Cached avatar handle for rendering; `None` falls back to a
    /// deterministic identicon generated from `nickname`.
    avatar: Option<CachedImage>,
    in_voice: bool,
    speaking: bool,
    expanded: bool,
}

/// `avatar` (the widget handle) is skipped: handles aren't hashable, and
/// `avatar_hash` exists precisely to detect avatar-content changes. The two
/// are populated together from the same user entry, so hashing the hash is
/// hashing the image.
impl Hash for UserRowDeps {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Full destructure: adding a field fails to compile until it is
        // explicitly hashed here or excluded like `avatar`.
        let Self {
            nickname,
            is_admin,
            is_shared,
            is_away,
            status,
            avatar_hash,
            avatar: _,
            in_voice,
            speaking,
            expanded,
        } = self;
        nickname.hash(state);
        is_admin.hash(state);
        is_shared.hash(state);
        is_away.hash(state);
        status.hash(state);
        avatar_hash.hash(state);
        in_voice.hash(state);
        speaking.hash(state);
        expanded.hash(state);
    }
}

/// Everything the expanded user's action toolbar renders. `None` whenever no
/// visible row is expanded — so permission, feature, and mute changes only
/// trigger a rebuild while a toolbar is actually showing.
#[derive(Clone, Hash)]
struct ToolbarDeps {
    nickname: String,
    is_self: bool,
    target_is_admin: bool,
    can_user_info: bool,
    can_user_message: bool,
    can_disconnect: bool,
    in_voice_with_us: bool,
    is_muted: bool,
    has_voice_listen: bool,
}

/// Dependencies for the lazy user-list sidebar.
///
/// When the hash of these values changes, the sidebar is rebuilt; otherwise
/// the cached widget tree is reused. The render closure reads *only* this
/// struct, so it cannot depend on state that isn't hashed. The i18n locale is
/// fixed at launch, so pre-resolved strings (title, empty message) and the
/// `t()` lookups inside the closure cannot go stale.
#[derive(Clone, Hash)]
struct UserListDeps {
    rows: Vec<UserRowDeps>,
    title: String,
    empty_message: String,
    toolbar: Option<ToolbarDeps>,
    colors: ThemeColors,
}

/// Resolve everything the sidebar renders into a [`UserListDeps`] value.
fn build_user_list_deps(conn: &ServerConnection, theme: &Theme) -> UserListDeps {
    let current_nickname = &conn.nickname;
    let expanded_user = conn.expanded_user.as_deref();

    let rows: Vec<UserRowDeps> = get_contextual_users(conn)
        .into_iter()
        .map(|user| {
            let nickname = &user.nickname;
            UserRowDeps {
                nickname: nickname.clone(),
                is_admin: user.is_admin,
                is_shared: user.is_shared,
                is_away: user.is_away,
                status: user.status.clone(),
                avatar_hash: user.avatar_hash,
                avatar: conn.avatar_cache.get(&avatar_cache_key(nickname)).cloned(),
                in_voice: is_user_in_visible_voice(
                    &conn.active_chat_tab,
                    conn.voice_session.as_ref(),
                    &conn.channel_voiced,
                    &conn.user_message_voiced,
                    nickname,
                ),
                speaking: is_user_speaking_in_visible_voice(
                    &conn.active_chat_tab,
                    conn.voice_session.as_ref(),
                    nickname,
                ),
                expanded: expanded_user == Some(nickname.as_str()),
            }
        })
        .collect();

    // Toolbar deps only when the expanded user is actually visible.
    let toolbar = rows.iter().find(|row| row.expanded).map(|row| {
        let nickname = row.nickname.as_str();
        let nickname_lower = fold_name(nickname);
        let in_voice_with_us = conn.voice_session.as_ref().is_some_and(|s| {
            s.participants
                .iter()
                .any(|p| fold_name(p) == nickname_lower)
        });
        ToolbarDeps {
            nickname: row.nickname.clone(),
            is_self: nickname == current_nickname,
            target_is_admin: row.is_admin,
            can_user_info: conn.has_permission(PERMISSION_USER_INFO),
            can_user_message: conn.has_feature(FEATURE_CHAT)
                && conn.has_permission(PERMISSION_USER_MESSAGE),
            can_disconnect: conn.has_permission(PERMISSION_USER_KICK)
                || conn.has_permission(PERMISSION_BAN_CREATE),
            in_voice_with_us,
            is_muted: conn
                .voice_session
                .as_ref()
                .is_some_and(|s| s.is_muted(nickname)),
            has_voice_listen: conn.has_permission(PERMISSION_VOICE_LISTEN),
        }
    });

    let empty_message = match &conn.active_chat_tab {
        ChatTab::Console | ChatTab::UserMessage(_) => t("empty-no-users"),
        ChatTab::Channel(_) => t("empty-no-channel-members"),
    };

    UserListDeps {
        rows,
        title: get_user_list_title(conn),
        empty_message,
        toolbar,
        colors: ThemeColors::from_theme(theme),
    }
}

// ============================================================================
// User Toolbar
// ============================================================================

/// Create the action toolbar for the expanded user from resolved deps.
fn create_user_toolbar(toolbar: &ToolbarDeps, colors: &ThemeColors) -> Row<'static, Message> {
    let nickname_owned = toolbar.nickname.clone();

    // Build toolbar row
    let mut toolbar_row = row![].spacing(NO_SPACING).width(Fill);

    // Info button (always show, disabled if no permission)
    let info_icon = icon_container(icon::info());
    let info_button = if toolbar.can_user_info {
        enabled_icon_button(
            info_icon,
            Message::UserInfoIconClicked(nickname_owned.clone()),
            colors.primary,
            colors.icon,
        )
    } else {
        disabled_icon_button(info_icon)
    };
    toolbar_row = toolbar_row.push(with_tooltip(info_button, t("tooltip-info")));

    // Message button (only show if not self)
    if !toolbar.is_self {
        let message_icon = icon_container(icon::message());
        let message_button = if toolbar.can_user_message {
            enabled_icon_button(
                message_icon,
                Message::UserMessageIconClicked(nickname_owned.clone()),
                colors.primary,
                colors.icon,
            )
        } else {
            disabled_icon_button(message_icon)
        };
        toolbar_row = toolbar_row.push(with_tooltip(message_button, t("tooltip-message")));
    }

    // Mute/Unmute button (only show if not self, user is in voice with us, and we have voice_listen)
    if !toolbar.is_self && toolbar.in_voice_with_us && toolbar.has_voice_listen {
        if toolbar.is_muted {
            // User is muted - show unmute button (left-aligned to prevent cone shift)
            let unmute_icon = mute_icon_container(icon::volume_off());
            let unmute_button = enabled_icon_button(
                unmute_icon,
                Message::VoiceUserUnmute(nickname_owned.clone()),
                colors.primary,
                colors.danger, // Show in danger color when muted
            );
            toolbar_row = toolbar_row.push(with_tooltip(unmute_button, t("tooltip-unmute")));
        } else {
            // User is not muted - show mute button (left-aligned to prevent cone shift)
            let mute_icon = mute_icon_container(icon::volume_up());
            let mute_button = enabled_icon_button(
                mute_icon,
                Message::VoiceUserMute(nickname_owned.clone()),
                colors.primary,
                colors.icon,
            );
            toolbar_row = toolbar_row.push(with_tooltip(mute_button, t("tooltip-mute")));
        }
    }

    // Disconnect button (if not self, has kick or ban permission, and target is not admin)
    if !toolbar.is_self && toolbar.can_disconnect && !toolbar.target_is_admin {
        // Add spacer to push disconnect button to the right
        toolbar_row = toolbar_row.push(Space::new().width(Fill).height(SEPARATOR_HEIGHT));

        let disconnect_icon = icon_container(icon::kick());
        let disconnect_button = enabled_icon_button(
            disconnect_icon,
            Message::DisconnectIconClicked(nickname_owned),
            colors.danger,
            colors.icon,
        );
        toolbar_row = toolbar_row.push(with_tooltip(disconnect_button, t("tooltip-disconnect")));
    }

    toolbar_row
}

// ============================================================================
// User List Panel
// ============================================================================

/// Render the sidebar from resolved deps only (called inside `lazy()`).
fn render_user_list(deps: &UserListDeps) -> Element<'static, Message> {
    let title = shaped_text(deps.title.clone())
        .size(USER_LIST_TITLE_SIZE)
        .style(muted_text_style);

    let mut users_column = Column::new().spacing(USER_LIST_ITEM_SPACING);

    if deps.rows.is_empty() {
        users_column = users_column.push(
            shaped_text(deps.empty_message.clone())
                .size(USER_LIST_SMALL_TEXT_SIZE)
                .style(muted_text_style),
        );
    } else {
        for (index, user) in deps.rows.iter().enumerate() {
            let is_even = index % 2 == 0;
            let nickname = &user.nickname;

            // Cached avatar handle, or a deterministic identicon fallback.
            let avatar_element: Element<'static, Message> = match &user.avatar {
                Some(cached_avatar) => cached_avatar.render(USER_LIST_AVATAR_SIZE),
                None => generate_identicon(nickname).render(USER_LIST_AVATAR_SIZE),
            };

            // Nickname color based on user type
            let nickname_color = if user.is_admin {
                Some(deps.colors.admin)
            } else if user.is_shared {
                Some(deps.colors.shared)
            } else {
                None
            };

            // Build nickname as rich_text with inline away/voice icons
            // First icon after nickname uses regular space (word break point).
            // Subsequent icons use non-breaking space (glued to previous icon).
            let mut spans: Vec<iced::widget::text::Span<'static, String, Font>> =
                vec![span(nickname.clone())];
            let mut has_icon = false;

            // Away icon first (right after nickname)
            if user.is_away {
                spans.push(span(" 💤"));
                has_icon = true;
            }

            // Voice indicator after away
            if user.in_voice {
                let separator = if has_icon { "\u{00A0}" } else { " " };
                if user.speaking {
                    // Speaking - mic icon in green (success color)
                    spans.push(
                        span(format!("{separator}\u{F130}"))
                            .font(Font::with_name("icons"))
                            .color(deps.colors.speaking),
                    );
                } else {
                    // In voice but not speaking - headphones in muted color
                    spans.push(
                        span(format!("{separator}\u{1F3A7}"))
                            .font(Font::with_name("icons"))
                            .color(deps.colors.muted),
                    );
                }
            }

            let nickname_widget = rich_text(spans)
                .size(USER_LIST_TEXT_SIZE)
                .wrapping(Wrapping::WordOrGlyph);

            // Build user row: avatar <gap> nickname+icons
            let mut user_row = Row::new().spacing(USER_LIST_AVATAR_SPACING).align_y(Center);

            user_row = user_row.push(avatar_element);
            user_row = user_row.push(nickname_widget);

            let user_button = button(container(user_row).width(Fill))
                .on_press(Message::UserListItemClicked(nickname.clone()))
                .width(Fill)
                .padding(INPUT_PADDING)
                .style(clickable_text_style(nickname_color));

            // Tooltip: show nickname with away/status if set
            let tooltip_text = build_user_tooltip(nickname, user.is_away, user.status.as_deref());

            // Wrap button in tooltip showing full name (useful when truncated)
            let user_button_with_tooltip = tooltip(
                user_button,
                container(shaped_text(tooltip_text).size(TOOLTIP_TEXT_SIZE))
                    .padding(TOOLTIP_BACKGROUND_PADDING)
                    .style(tooltip_container_style),
                tooltip::Position::Left,
            )
            .gap(TOOLTIP_GAP)
            .padding(TOOLTIP_PADDING);

            // Create item column (username + optional toolbar)
            let mut item_column = Column::new().spacing(NO_SPACING);

            // Username button with tooltip
            item_column = item_column.push(user_button_with_tooltip);

            // Add toolbar if expanded
            if user.expanded
                && let Some(toolbar) = &deps.toolbar
            {
                // Primary color separator line
                item_column = item_column.push(toolbar_separator());

                // Toolbar
                let toolbar_row = container(create_user_toolbar(toolbar, &deps.colors))
                    .width(Fill)
                    .padding(TOOLBAR_CONTAINER_PADDING);
                item_column = item_column.push(toolbar_row);
            }

            // Wrap entire item (username + toolbar) in container with alternating background
            let item_container = container(item_column)
                .width(Fill)
                .style(alternating_row_style(is_even));

            users_column = users_column.push(item_container);
        }
    }

    // Add right padding to make room for scrollbar
    let users_column = container(users_column)
        .padding(iced::Padding {
            top: 0.0,
            right: SCROLLBAR_PADDING,
            bottom: 0.0,
            left: 0.0,
        })
        .width(Fill);

    let panel = column![title, scrollable(users_column).height(Fill),]
        .spacing(USER_LIST_SPACING)
        .padding(iced::Padding {
            top: CONTENT_PADDING,
            right: CONTENT_PADDING - SCROLLBAR_PADDING,
            bottom: CONTENT_PADDING,
            left: CONTENT_PADDING,
        })
        .width(USER_LIST_PANEL_WIDTH);

    container(panel)
        .height(Fill)
        .style(sidebar_panel_style)
        .into()
}

/// Displays online users as clickable buttons with expandable action toolbars
///
/// Shows a contextual list of users based on the active chat tab:
/// - Console tab: All online users
/// - Channel tab: Only channel members
/// - User message tab: You + the other user
///
/// Clicking a username expands it to show an action toolbar underneath.
/// Only one user can be expanded at a time.
/// Admin users are shown in red (using the chat admin color).
///
/// Note: This panel is only shown when the user has `user_list` permission.
/// Permission checking is done at the layout level.
pub fn user_list_panel<'a>(conn: &'a ServerConnection, theme: &Theme) -> Element<'a, Message> {
    let deps = build_user_list_deps(conn, theme);
    lazy(deps, render_user_list).into()
}

#[cfg(test)]
mod tests {
    use std::hash::DefaultHasher;

    use super::*;

    fn voiced_set(nicknames: &[&str]) -> HashSet<String> {
        nicknames
            .iter()
            .map(|nickname| fold_name(nickname))
            .collect()
    }

    #[test]
    fn channel_voice_presence_uses_visible_tab_while_in_different_voice() {
        let active_tab = ChatTab::Channel("#support".to_string());
        let voice_session = VoiceState::new("#general".to_string(), vec!["Alice".to_string()]);
        let mut channel_voiced = HashMap::new();
        channel_voiced.insert(fold_name("#support"), voiced_set(&["Bob"]));
        let user_message_voiced = HashSet::new();

        assert!(is_user_in_visible_voice(
            &active_tab,
            Some(&voice_session),
            &channel_voiced,
            &user_message_voiced,
            "Bob",
        ));
        assert!(!is_user_in_visible_voice(
            &active_tab,
            Some(&voice_session),
            &channel_voiced,
            &user_message_voiced,
            "Alice",
        ));
    }

    #[test]
    fn dm_voice_presence_uses_visible_tab_while_in_different_voice() {
        let active_tab = ChatTab::UserMessage("Bob".to_string());
        let voice_session = VoiceState::new("#general".to_string(), vec!["Alice".to_string()]);
        let channel_voiced = HashMap::new();
        let user_message_voiced = voiced_set(&["Bob"]);

        assert!(is_user_in_visible_voice(
            &active_tab,
            Some(&voice_session),
            &channel_voiced,
            &user_message_voiced,
            "Bob",
        ));
        assert!(!is_user_in_visible_voice(
            &active_tab,
            Some(&voice_session),
            &channel_voiced,
            &user_message_voiced,
            "Alice",
        ));
    }

    #[test]
    fn active_voice_session_supplements_visible_tab_presence() {
        let active_tab = ChatTab::Channel("#general".to_string());
        let voice_session = VoiceState::new("#general".to_string(), vec!["Alice".to_string()]);
        let channel_voiced = HashMap::new();
        let user_message_voiced = HashSet::new();

        assert!(is_user_in_visible_voice(
            &active_tab,
            Some(&voice_session),
            &channel_voiced,
            &user_message_voiced,
            "Alice",
        ));
    }

    #[test]
    fn speaking_indicator_stays_scoped_to_active_voice_session() {
        let mut voice_session = VoiceState::new("#general".to_string(), vec!["Bob".to_string()]);
        voice_session.set_speaking("Bob");

        assert!(is_user_speaking_in_visible_voice(
            &ChatTab::Channel("#general".to_string()),
            Some(&voice_session),
            "Bob",
        ));
        assert!(!is_user_speaking_in_visible_voice(
            &ChatTab::Channel("#support".to_string()),
            Some(&voice_session),
            "Bob",
        ));
    }

    // ------------------------------------------------------------------
    // Deps hash sensitivity
    //
    // The lazy() cache reuses the widget tree whenever the deps hash is
    // unchanged, so every rendered fact must move the hash. Each test
    // mutates one dependency and asserts the hash changes.
    // ------------------------------------------------------------------

    fn hash_of(deps: &UserListDeps) -> u64 {
        let mut hasher = DefaultHasher::new();
        deps.hash(&mut hasher);
        hasher.finish()
    }

    fn test_colors() -> ThemeColors {
        ThemeColors::from_theme(&Theme::Dark)
    }

    fn test_row(nickname: &str) -> UserRowDeps {
        UserRowDeps {
            nickname: nickname.to_string(),
            is_admin: false,
            is_shared: false,
            is_away: false,
            status: None,
            avatar_hash: None,
            avatar: None,
            in_voice: false,
            speaking: false,
            expanded: false,
        }
    }

    fn test_toolbar(nickname: &str) -> ToolbarDeps {
        ToolbarDeps {
            nickname: nickname.to_string(),
            is_self: false,
            target_is_admin: false,
            can_user_info: true,
            can_user_message: true,
            can_disconnect: false,
            in_voice_with_us: false,
            is_muted: false,
            has_voice_listen: false,
        }
    }

    fn test_deps() -> UserListDeps {
        UserListDeps {
            rows: vec![test_row("Alice"), test_row("Bob")],
            title: "Users".to_string(),
            empty_message: "No users online".to_string(),
            toolbar: None,
            colors: test_colors(),
        }
    }

    #[test]
    fn deps_hash_changes_on_row_content() {
        let base = test_deps();
        let base_hash = hash_of(&base);

        // User added
        let mut deps = base.clone();
        deps.rows.push(test_row("Carol"));
        assert_ne!(hash_of(&deps), base_hash, "user added");

        // User removed
        let mut deps = base.clone();
        deps.rows.pop();
        assert_ne!(hash_of(&deps), base_hash, "user removed");

        // Rename
        let mut deps = base.clone();
        deps.rows[0].nickname = "Alicia".to_string();
        assert_ne!(hash_of(&deps), base_hash, "rename");

        // Admin flag (nickname color)
        let mut deps = base.clone();
        deps.rows[0].is_admin = true;
        assert_ne!(hash_of(&deps), base_hash, "admin flag");

        // Shared flag (nickname color)
        let mut deps = base.clone();
        deps.rows[1].is_shared = true;
        assert_ne!(hash_of(&deps), base_hash, "shared flag");

        // Away toggle (💤 icon + tooltip)
        let mut deps = base.clone();
        deps.rows[0].is_away = true;
        assert_ne!(hash_of(&deps), base_hash, "away toggle");

        // Status change (tooltip)
        let mut deps = base.clone();
        deps.rows[0].status = Some("brb".to_string());
        assert_ne!(hash_of(&deps), base_hash, "status change");

        // Avatar content change (via the avatar_hash proxy)
        let mut deps = base.clone();
        deps.rows[0].avatar_hash = Some([7u8; 32]);
        assert_ne!(hash_of(&deps), base_hash, "avatar hash");

        // Voice presence + speaking indicators
        let mut deps = base.clone();
        deps.rows[1].in_voice = true;
        assert_ne!(hash_of(&deps), base_hash, "in voice");
        deps.rows[1].speaking = true;
        assert_ne!(hash_of(&deps), base_hash, "speaking");

        // Expansion marker
        let mut deps = base.clone();
        deps.rows[0].expanded = true;
        assert_ne!(hash_of(&deps), base_hash, "expanded row");
    }

    #[test]
    fn deps_hash_changes_on_title_and_empty_message() {
        let base = test_deps();
        let base_hash = hash_of(&base);

        let mut deps = base.clone();
        deps.title = "Channel Members".to_string();
        assert_ne!(hash_of(&deps), base_hash, "title");

        let mut deps = base.clone();
        deps.empty_message = "No channel members".to_string();
        assert_ne!(hash_of(&deps), base_hash, "empty message");
    }

    #[test]
    fn deps_hash_changes_on_toolbar_state() {
        let mut base = test_deps();
        base.rows[0].expanded = true;
        base.toolbar = Some(test_toolbar("Alice"));
        let base_hash = hash_of(&base);

        // Toolbar disappearing entirely
        let mut deps = base.clone();
        deps.toolbar = None;
        assert_ne!(hash_of(&deps), base_hash, "toolbar removed");

        // Permission flips while the toolbar is showing
        let mut deps = base.clone();
        deps.toolbar.as_mut().unwrap().can_user_info = false;
        assert_ne!(hash_of(&deps), base_hash, "user_info permission");

        let mut deps = base.clone();
        deps.toolbar.as_mut().unwrap().can_user_message = false;
        assert_ne!(hash_of(&deps), base_hash, "user_message permission");

        let mut deps = base.clone();
        deps.toolbar.as_mut().unwrap().can_disconnect = true;
        assert_ne!(hash_of(&deps), base_hash, "disconnect permission");

        // Voice/mute state driving the mute button
        let mut deps = base.clone();
        let toolbar = deps.toolbar.as_mut().unwrap();
        toolbar.in_voice_with_us = true;
        toolbar.has_voice_listen = true;
        let unmuted_hash = hash_of(&deps);
        assert_ne!(unmuted_hash, base_hash, "voice with us");
        deps.toolbar.as_mut().unwrap().is_muted = true;
        assert_ne!(hash_of(&deps), unmuted_hash, "mute toggle");
    }

    #[test]
    fn deps_hash_changes_on_theme_colors() {
        let base = test_deps();
        let base_hash = hash_of(&base);

        let mut deps = base.clone();
        deps.colors.admin = Color::from_rgb(0.1, 0.2, 0.3);
        assert_ne!(hash_of(&deps), base_hash, "admin color");

        let mut deps = base.clone();
        deps.colors.speaking = Color::from_rgb(0.4, 0.5, 0.6);
        assert_ne!(hash_of(&deps), base_hash, "speaking color");
    }

    #[test]
    fn deps_hash_ignores_avatar_handle_alone() {
        // The CachedImage handle is excluded from the hash by design:
        // avatar_hash is its change-detection proxy, and the two are
        // populated together. A handle-only difference (e.g. a re-decoded
        // identical avatar) must NOT invalidate the cache.
        let base = test_deps();
        let mut deps = base.clone();
        deps.rows[0].avatar = Some(generate_identicon("Alice"));
        assert_eq!(hash_of(&deps), hash_of(&base));
    }
}
