//! Connection Monitor panel view
//!
//! Displays tabs for active connections and file transfers.
//! Connections tab shows nickname, username, IP address, and connection time.
//! Transfers tab shows user, direction, path, progress, and time.
//! Supports right-click context menu for actions (permission-gated).

use std::hash::{Hash, Hasher};

use crate::widgets::MenuButton;
use iced::widget::text::Wrapping;
use iced::widget::{Space, button, column, container, lazy, row, scrollable, table, tooltip};
use iced::{Center, Element, Fill, Length, Theme, alignment};
use iced_aw::{ContextMenu, TabLabel, Tabs};
use nexus_common::names::fold_name;
use nexus_common::protocol::{ConnectionInfo, TransferInfo};

use super::constants::{PERMISSION_BAN_CREATE, PERMISSION_USER_INFO, PERMISSION_USER_KICK};
use super::helpers::{format_bytes, sort_icon_or_placeholder};
use crate::i18n::t;
use crate::icon;
use crate::style::{
    CONTENT_MAX_WIDTH, CONTENT_PADDING, CONTEXT_MENU_ITEM_PADDING, CONTEXT_MENU_MIN_WIDTH,
    CONTEXT_MENU_PADDING, CONTEXT_MENU_SEPARATOR_HEIGHT, CONTEXT_MENU_SEPARATOR_MARGIN,
    HEADING_BUTTON_PADDING, ICON_SIZE, NO_SPACING, SCROLLBAR_PADDING, SEPARATOR_HEIGHT,
    SORT_ICON_LEFT_MARGIN, SORT_ICON_RIGHT_MARGIN, SPACER_SIZE_LARGE, SPACER_SIZE_MEDIUM,
    SPACER_SIZE_SMALL, TAB_LABEL_PADDING, TEXT_SIZE, TITLE_SIZE, TOOLTIP_BACKGROUND_PADDING,
    TOOLTIP_GAP, TOOLTIP_PADDING, TOOLTIP_TEXT_SIZE, chat, content_background_style,
    context_menu_container_style, error_text_style, menu_button_danger_style, menu_button_style,
    muted_text_style, separator_style, shaped_text, shaped_text_wrapped, tooltip_container_style,
    transparent_icon_button_style,
};
use crate::types::{
    ConnectionMonitorSortColumn, ConnectionMonitorState, ConnectionMonitorTab, Message,
    ServerConnection, TransferSortColumn,
};

/// Permissions for connection monitor context menu
#[derive(Clone, Copy, Hash)]
struct ConnectionMonitorPermissions {
    user_info: bool,
    user_kick: bool,
    ban_create: bool,
}

/// Dependencies for lazy connection table rendering
#[derive(Clone)]
struct ConnectionTableDeps {
    connections: Vec<ConnectionInfo>,
    sort_column: ConnectionMonitorSortColumn,
    sort_ascending: bool,
    admin_color: iced::Color,
    shared_color: iced::Color,
    permissions: ConnectionMonitorPermissions,
}

impl Hash for ConnectionTableDeps {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.connections.len().hash(state);
        for conn in &self.connections {
            conn.nickname.hash(state);
            conn.username.hash(state);
            conn.ip.hash(state);
            conn.login_time.hash(state);
            conn.is_admin.hash(state);
            conn.is_shared.hash(state);
        }
        self.sort_column.hash(state);
        self.sort_ascending.hash(state);
        self.permissions.hash(state);
        // Colors don't need hashing - they're derived from theme which doesn't change per-render
    }
}

/// Dependencies for lazy transfer table rendering
#[derive(Clone)]
struct TransferTableDeps {
    transfers: Vec<TransferInfo>,
    sort_column: TransferSortColumn,
    sort_ascending: bool,
    admin_color: iced::Color,
    shared_color: iced::Color,
}

impl Hash for TransferTableDeps {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.transfers.len().hash(state);
        for transfer in &self.transfers {
            transfer.nickname.hash(state);
            transfer.username.hash(state);
            transfer.ip.hash(state);
            transfer.direction.hash(state);
            transfer.path.hash(state);
            transfer.total_size.hash(state);
            transfer.bytes_transferred.hash(state);
            transfer.started_at.hash(state);
            transfer.is_admin.hash(state);
            transfer.is_shared.hash(state);
        }
        self.sort_column.hash(state);
        self.sort_ascending.hash(state);
        // Colors don't need hashing - they're derived from theme which doesn't change per-render
    }
}

/// Format a Unix timestamp as relative time (e.g., "5m", "2h", "3d")
fn format_elapsed_time(timestamp: i64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let elapsed_secs = now.saturating_sub(timestamp);

    if elapsed_secs < 60 {
        format!("{}s", elapsed_secs)
    } else if elapsed_secs < 3600 {
        format!("{}m", elapsed_secs / 60)
    } else if elapsed_secs < 86400 {
        format!("{}h", elapsed_secs / 3600)
    } else {
        format!("{}d", elapsed_secs / 86400)
    }
}

/// Format transfer progress as percentage
fn format_progress_percent(bytes_transferred: u64, total_size: u64) -> String {
    if total_size == 0 {
        "0%".to_string()
    } else {
        let percent = (bytes_transferred as f64 / total_size as f64 * 100.0).min(100.0);
        format!("{:.0}%", percent)
    }
}

/// Format transfer progress tooltip showing bytes transferred / total
fn format_progress_tooltip(bytes_transferred: u64, total_size: u64) -> String {
    format!(
        "{} / {}",
        format_bytes(bytes_transferred),
        format_bytes(total_size)
    )
}

/// Build a context menu with Info, Copy, Kick, Ban actions for connections
///
/// Menu structure:
/// - Info (if user_info permission)
/// - ─── separator ─── (if Info visible)
/// - Copy (always)
/// - ─── separator ─── (if Kick or Ban visible)
/// - Kick (if user_kick permission, hidden for admin rows)
/// - Ban (if ban_create permission, hidden for admin rows)
fn build_connection_context_menu(
    nickname: String,
    value: String,
    is_admin_row: bool,
    permissions: ConnectionMonitorPermissions,
) -> Element<'static, Message> {
    let mut menu_items: Vec<Element<'_, Message>> = vec![];

    // Info (if permission) - available for all users
    let show_info = permissions.user_info;
    if show_info {
        let nickname_for_info = nickname.clone();
        menu_items.push(
            MenuButton::new(shaped_text(t("menu-info")).size(TEXT_SIZE))
                .padding(CONTEXT_MENU_ITEM_PADDING)
                .width(Fill)
                .style(menu_button_style)
                .on_press(Message::ConnectionMonitorInfo(nickname_for_info))
                .into(),
        );
    }

    // First separator (after Info, before Copy)
    if show_info {
        menu_items.push(
            container(Space::new())
                .width(Fill)
                .height(CONTEXT_MENU_SEPARATOR_HEIGHT)
                .style(separator_style)
                .into(),
        );
    }

    // Copy (always available)
    menu_items.push(
        MenuButton::new(shaped_text(t("menu-copy")).size(TEXT_SIZE))
            .padding(CONTEXT_MENU_ITEM_PADDING)
            .width(Fill)
            .style(menu_button_style)
            .on_press(Message::ConnectionMonitorCopy(value))
            .into(),
    );

    // Kick/Ban only shown for non-admin rows
    let show_kick = permissions.user_kick && !is_admin_row;
    let show_ban = permissions.ban_create && !is_admin_row;

    // Second separator (after Copy, before Kick/Ban)
    if show_kick || show_ban {
        menu_items.push(
            container(Space::new())
                .width(Fill)
                .height(CONTEXT_MENU_SEPARATOR_HEIGHT)
                .style(separator_style)
                .into(),
        );
    }

    // Kick (if permission and not admin row) - danger style
    if show_kick {
        let nickname_for_kick = nickname.clone();
        menu_items.push(
            MenuButton::new(shaped_text(t("menu-kick")).size(TEXT_SIZE))
                .padding(CONTEXT_MENU_ITEM_PADDING)
                .width(Fill)
                .style(menu_button_danger_style)
                .on_press(Message::ConnectionMonitorKick(nickname_for_kick))
                .into(),
        );
    }

    // Ban (if permission and not admin row) - danger style
    if show_ban {
        menu_items.push(
            MenuButton::new(shaped_text(t("menu-ban")).size(TEXT_SIZE))
                .padding(CONTEXT_MENU_ITEM_PADDING)
                .width(Fill)
                .style(menu_button_danger_style)
                .on_press(Message::ConnectionMonitorBan(nickname))
                .into(),
        );
    }

    container(
        iced::widget::Column::with_children(menu_items).spacing(CONTEXT_MENU_SEPARATOR_MARGIN),
    )
    .width(CONTEXT_MENU_MIN_WIDTH)
    .padding(CONTEXT_MENU_PADDING)
    .style(context_menu_container_style)
    .into()
}

/// Build a simple context menu with Copy action for transfers
fn build_transfer_context_menu(value: String) -> Element<'static, Message> {
    container(
        MenuButton::new(shaped_text(t("menu-copy")).size(TEXT_SIZE))
            .padding(CONTEXT_MENU_ITEM_PADDING)
            .width(Fill)
            .style(menu_button_style)
            .on_press(Message::ConnectionMonitorCopy(value)),
    )
    .width(CONTEXT_MENU_MIN_WIDTH)
    .padding(CONTEXT_MENU_PADDING)
    .style(context_menu_container_style)
    .into()
}

/// Build the lazy connection table using table widget
fn lazy_connection_table(deps: ConnectionTableDeps) -> Element<'static, Message> {
    let permissions = deps.permissions;

    lazy(deps, move |deps| {
        // Nickname column header
        let nickname_sort_icon = sort_icon_or_placeholder(
            deps.sort_column == ConnectionMonitorSortColumn::Nickname,
            deps.sort_ascending,
        );
        let nickname_header_content: Element<'static, Message> = row![
            shaped_text(t("col-nickname"))
                .size(TEXT_SIZE)
                .style(muted_text_style)
                .wrapping(Wrapping::Word),
            Space::new().width(Fill),
            Space::new().width(SORT_ICON_LEFT_MARGIN),
            nickname_sort_icon,
            Space::new().width(SORT_ICON_RIGHT_MARGIN),
        ]
        .align_y(Center)
        .into();
        let nickname_header: Element<'static, Message> = button(nickname_header_content)
            .padding(NO_SPACING)
            .width(Length::Shrink)
            .style(transparent_icon_button_style)
            .on_press(Message::ConnectionMonitorSortBy(
                ConnectionMonitorSortColumn::Nickname,
            ))
            .into();

        // Nickname column - with admin/shared coloring
        let admin_color = deps.admin_color;
        let shared_color = deps.shared_color;
        let nickname_column = table::column(nickname_header, move |conn: ConnectionInfo| {
            let nickname_for_menu = conn.nickname.clone();
            let nickname_for_value = conn.nickname.clone();
            let is_admin_row = conn.is_admin;

            // Apply color based on user type: admin (red), shared (muted), regular (default)
            let content: Element<'static, Message> = if conn.is_admin {
                shaped_text(conn.nickname)
                    .size(TEXT_SIZE)
                    .wrapping(Wrapping::WordOrGlyph)
                    .color(admin_color)
                    .into()
            } else if conn.is_shared {
                shaped_text(conn.nickname)
                    .size(TEXT_SIZE)
                    .wrapping(Wrapping::WordOrGlyph)
                    .color(shared_color)
                    .into()
            } else {
                shaped_text(conn.nickname)
                    .size(TEXT_SIZE)
                    .wrapping(Wrapping::WordOrGlyph)
                    .into()
            };

            // Wrap in context menu with full actions
            ContextMenu::new(content, move || {
                build_connection_context_menu(
                    nickname_for_menu.clone(),
                    nickname_for_value.clone(),
                    is_admin_row,
                    permissions,
                )
            })
        })
        .width(Length::FillPortion(1));

        // Username column header
        let username_sort_icon = sort_icon_or_placeholder(
            deps.sort_column == ConnectionMonitorSortColumn::Username,
            deps.sort_ascending,
        );
        let username_header_content: Element<'static, Message> = row![
            shaped_text(t("col-username"))
                .size(TEXT_SIZE)
                .style(muted_text_style)
                .wrapping(Wrapping::Word),
            Space::new().width(Fill),
            Space::new().width(SORT_ICON_LEFT_MARGIN),
            username_sort_icon,
            Space::new().width(SORT_ICON_RIGHT_MARGIN),
        ]
        .align_y(Center)
        .into();
        let username_header: Element<'static, Message> = button(username_header_content)
            .padding(NO_SPACING)
            .width(Length::Shrink)
            .style(transparent_icon_button_style)
            .on_press(Message::ConnectionMonitorSortBy(
                ConnectionMonitorSortColumn::Username,
            ))
            .into();

        // Username column - with admin/shared coloring
        let username_column = table::column(username_header, move |conn: ConnectionInfo| {
            let nickname_for_menu = conn.nickname.clone();
            let username_for_value = conn.username.clone();
            let is_admin_row = conn.is_admin;

            // Apply color based on user type: admin (red), shared (muted), regular (muted)
            let content: Element<'static, Message> = if conn.is_admin {
                shaped_text(conn.username)
                    .size(TEXT_SIZE)
                    .wrapping(Wrapping::Glyph)
                    .color(admin_color)
                    .into()
            } else if conn.is_shared {
                shaped_text(conn.username)
                    .size(TEXT_SIZE)
                    .wrapping(Wrapping::Glyph)
                    .color(shared_color)
                    .into()
            } else {
                shaped_text(conn.username)
                    .size(TEXT_SIZE)
                    .wrapping(Wrapping::Glyph)
                    .into()
            };

            ContextMenu::new(content, move || {
                build_connection_context_menu(
                    nickname_for_menu.clone(),
                    username_for_value.clone(),
                    is_admin_row,
                    permissions,
                )
            })
        })
        .width(Length::FillPortion(1));

        // IP Address column header
        let ip_sort_icon = sort_icon_or_placeholder(
            deps.sort_column == ConnectionMonitorSortColumn::Ip,
            deps.sort_ascending,
        );
        let ip_header_content: Element<'static, Message> = row![
            shaped_text(t("col-ip-address"))
                .size(TEXT_SIZE)
                .style(muted_text_style)
                .wrapping(Wrapping::Word),
            Space::new().width(Fill),
            Space::new().width(SORT_ICON_LEFT_MARGIN),
            ip_sort_icon,
            Space::new().width(SORT_ICON_RIGHT_MARGIN),
        ]
        .align_y(Center)
        .into();
        let ip_header: Element<'static, Message> = button(ip_header_content)
            .padding(NO_SPACING)
            .width(Length::Shrink)
            .style(transparent_icon_button_style)
            .on_press(Message::ConnectionMonitorSortBy(
                ConnectionMonitorSortColumn::Ip,
            ))
            .into();

        // IP Address column
        let ip_column = table::column(ip_header, move |conn: ConnectionInfo| {
            let nickname_for_menu = conn.nickname.clone();
            let ip_for_value = conn.ip.clone();
            let is_admin_row = conn.is_admin;

            let content: Element<'static, Message> = shaped_text(conn.ip)
                .size(TEXT_SIZE)
                .style(muted_text_style)
                .wrapping(Wrapping::WordOrGlyph)
                .into();

            ContextMenu::new(content, move || {
                build_connection_context_menu(
                    nickname_for_menu.clone(),
                    ip_for_value.clone(),
                    is_admin_row,
                    permissions,
                )
            })
        })
        .width(Length::FillPortion(1));

        // Connected time column header
        let connected_sort_icon = sort_icon_or_placeholder(
            deps.sort_column == ConnectionMonitorSortColumn::Connected,
            deps.sort_ascending,
        );
        let connected_header_content: Element<'static, Message> = row![
            shaped_text(t("col-time"))
                .size(TEXT_SIZE)
                .style(muted_text_style)
                .wrapping(Wrapping::Word),
            Space::new().width(Fill),
            Space::new().width(SORT_ICON_LEFT_MARGIN),
            connected_sort_icon,
            Space::new().width(SORT_ICON_RIGHT_MARGIN),
            // Trailing gap so the rightmost column doesn't abut the scrollbar.
            Space::new().width(SCROLLBAR_PADDING),
        ]
        .align_y(Center)
        .into();
        let connected_header: Element<'static, Message> = button(connected_header_content)
            .padding(NO_SPACING)
            .width(Length::Shrink)
            .style(transparent_icon_button_style)
            .on_press(Message::ConnectionMonitorSortBy(
                ConnectionMonitorSortColumn::Connected,
            ))
            .into();

        // Connected time column. Trailing Space matches the header so the
        // rightmost column doesn't abut the scrollbar.
        let connected_column = table::column(connected_header, |conn: ConnectionInfo| {
            let time_str = format_elapsed_time(conn.login_time);
            row![
                shaped_text(time_str)
                    .size(TEXT_SIZE)
                    .style(muted_text_style)
                    .wrapping(Wrapping::Word),
                Space::new().width(SCROLLBAR_PADDING),
            ]
            .align_y(Center)
        })
        .width(Length::Shrink);

        // Build the table
        let columns = [
            nickname_column,
            username_column,
            ip_column,
            connected_column,
        ];

        table(columns, deps.connections.clone())
            .width(Fill)
            .padding_x(SPACER_SIZE_SMALL)
            .padding_y(SPACER_SIZE_SMALL)
            .separator_x(NO_SPACING)
            .separator_y(SEPARATOR_HEIGHT)
    })
    .into()
}

/// Build the lazy transfer table using table widget
fn lazy_transfer_table(deps: TransferTableDeps) -> Element<'static, Message> {
    lazy(deps, move |deps| {
        let admin_color = deps.admin_color;
        let shared_color = deps.shared_color;

        // User column header
        let user_sort_icon = sort_icon_or_placeholder(
            deps.sort_column == TransferSortColumn::User,
            deps.sort_ascending,
        );
        let user_header_content: Element<'static, Message> = row![
            shaped_text(t("col-nickname"))
                .size(TEXT_SIZE)
                .style(muted_text_style)
                .wrapping(Wrapping::Word),
            Space::new().width(Fill),
            Space::new().width(SORT_ICON_LEFT_MARGIN),
            user_sort_icon,
            Space::new().width(SORT_ICON_RIGHT_MARGIN),
        ]
        .align_y(Center)
        .into();
        let user_header: Element<'static, Message> = button(user_header_content)
            .padding(NO_SPACING)
            .width(Length::Shrink)
            .style(transparent_icon_button_style)
            .on_press(Message::ConnectionMonitorTransferSortBy(
                TransferSortColumn::User,
            ))
            .into();

        // User column - with admin/shared coloring
        let user_column = table::column(user_header, move |transfer: TransferInfo| {
            let nickname_for_value = transfer.nickname.clone();

            // Apply color based on user type
            let content: Element<'static, Message> = if transfer.is_admin {
                shaped_text(transfer.nickname)
                    .size(TEXT_SIZE)
                    .wrapping(Wrapping::WordOrGlyph)
                    .color(admin_color)
                    .into()
            } else if transfer.is_shared {
                shaped_text(transfer.nickname)
                    .size(TEXT_SIZE)
                    .wrapping(Wrapping::WordOrGlyph)
                    .color(shared_color)
                    .into()
            } else {
                shaped_text(transfer.nickname)
                    .size(TEXT_SIZE)
                    .wrapping(Wrapping::WordOrGlyph)
                    .into()
            };

            ContextMenu::new(content, move || {
                build_transfer_context_menu(nickname_for_value.clone())
            })
        })
        .width(Length::FillPortion(1));

        // IP Address column header
        let ip_sort_icon = sort_icon_or_placeholder(
            deps.sort_column == TransferSortColumn::Ip,
            deps.sort_ascending,
        );
        let ip_header_content: Element<'static, Message> = row![
            shaped_text(t("col-ip-address"))
                .size(TEXT_SIZE)
                .style(muted_text_style)
                .wrapping(Wrapping::Word),
            Space::new().width(Fill),
            Space::new().width(SORT_ICON_LEFT_MARGIN),
            ip_sort_icon,
            Space::new().width(SORT_ICON_RIGHT_MARGIN),
        ]
        .align_y(Center)
        .into();
        let ip_header: Element<'static, Message> = button(ip_header_content)
            .padding(NO_SPACING)
            .width(Length::Shrink)
            .style(transparent_icon_button_style)
            .on_press(Message::ConnectionMonitorTransferSortBy(
                TransferSortColumn::Ip,
            ))
            .into();

        // IP Address column
        let ip_column = table::column(ip_header, move |transfer: TransferInfo| {
            let ip_for_value = transfer.ip.clone();

            let content: Element<'static, Message> = shaped_text(transfer.ip)
                .size(TEXT_SIZE)
                .style(muted_text_style)
                .wrapping(Wrapping::WordOrGlyph)
                .into();

            ContextMenu::new(content, move || {
                build_transfer_context_menu(ip_for_value.clone())
            })
        })
        .width(Length::FillPortion(1));

        // Direction column header (use exchange icon instead of text)
        let direction_sort_icon = sort_icon_or_placeholder(
            deps.sort_column == TransferSortColumn::Direction,
            deps.sort_ascending,
        );
        let direction_header_content: Element<'static, Message> = row![
            icon::exchange().size(TEXT_SIZE).style(muted_text_style),
            Space::new().width(SPACER_SIZE_MEDIUM),
            direction_sort_icon,
        ]
        .align_y(Center)
        .into();
        let direction_header: Element<'static, Message> = button(direction_header_content)
            .padding(NO_SPACING)
            .width(Length::Shrink)
            .style(transparent_icon_button_style)
            .on_press(Message::ConnectionMonitorTransferSortBy(
                TransferSortColumn::Direction,
            ))
            .into();

        // Direction column - show ↓ for download, ↑ for upload
        let direction_column = table::column(direction_header, move |transfer: TransferInfo| {
            // "download" means server is sending to client (client downloading)
            // "upload" means client is sending to server (client uploading)
            if transfer.direction == "download" {
                icon::download().size(TEXT_SIZE)
            } else {
                icon::upload().size(TEXT_SIZE)
            }
        })
        .width(Length::Shrink);

        // Path column header
        let path_sort_icon = sort_icon_or_placeholder(
            deps.sort_column == TransferSortColumn::Path,
            deps.sort_ascending,
        );
        let path_header_content: Element<'static, Message> = row![
            shaped_text(t("col-path"))
                .size(TEXT_SIZE)
                .style(muted_text_style)
                .wrapping(Wrapping::Word),
            Space::new().width(Fill),
            Space::new().width(SORT_ICON_LEFT_MARGIN),
            path_sort_icon,
            Space::new().width(SORT_ICON_RIGHT_MARGIN),
        ]
        .align_y(Center)
        .into();
        let path_header: Element<'static, Message> = button(path_header_content)
            .padding(NO_SPACING)
            .width(Length::Shrink)
            .style(transparent_icon_button_style)
            .on_press(Message::ConnectionMonitorTransferSortBy(
                TransferSortColumn::Path,
            ))
            .into();

        // Path column
        let path_column = table::column(path_header, move |transfer: TransferInfo| {
            let path_for_value = transfer.path.clone();

            let content: Element<'static, Message> = shaped_text(transfer.path)
                .size(TEXT_SIZE)
                .style(muted_text_style)
                .wrapping(Wrapping::WordOrGlyph)
                .into();

            ContextMenu::new(content, move || {
                build_transfer_context_menu(path_for_value.clone())
            })
        })
        .width(Length::FillPortion(1));

        // Progress column header
        let progress_sort_icon = sort_icon_or_placeholder(
            deps.sort_column == TransferSortColumn::Progress,
            deps.sort_ascending,
        );
        let progress_header_content: Element<'static, Message> = row![
            shaped_text(t("col-progress"))
                .size(TEXT_SIZE)
                .style(muted_text_style)
                .wrapping(Wrapping::Word),
            Space::new().width(Fill),
            Space::new().width(SORT_ICON_LEFT_MARGIN),
            progress_sort_icon,
            Space::new().width(SORT_ICON_RIGHT_MARGIN),
        ]
        .align_y(Center)
        .into();
        let progress_header: Element<'static, Message> = button(progress_header_content)
            .padding(NO_SPACING)
            .width(Length::Shrink)
            .style(transparent_icon_button_style)
            .on_press(Message::ConnectionMonitorTransferSortBy(
                TransferSortColumn::Progress,
            ))
            .into();

        // Progress column (percentage with tooltip showing bytes)
        let progress_column = table::column(progress_header, move |transfer: TransferInfo| {
            let progress_str =
                format_progress_percent(transfer.bytes_transferred, transfer.total_size);
            let tooltip_str =
                format_progress_tooltip(transfer.bytes_transferred, transfer.total_size);

            let content = shaped_text(progress_str)
                .size(TEXT_SIZE)
                .style(muted_text_style)
                .wrapping(Wrapping::Word);

            tooltip(
                content,
                container(shaped_text(tooltip_str).size(TOOLTIP_TEXT_SIZE))
                    .padding(TOOLTIP_BACKGROUND_PADDING)
                    .style(tooltip_container_style),
                tooltip::Position::Top,
            )
            .gap(TOOLTIP_GAP)
            .padding(TOOLTIP_PADDING)
        })
        .width(Length::Shrink);

        // Time column header
        let time_sort_icon = sort_icon_or_placeholder(
            deps.sort_column == TransferSortColumn::Time,
            deps.sort_ascending,
        );
        let time_header_content: Element<'static, Message> = row![
            shaped_text(t("col-time"))
                .size(TEXT_SIZE)
                .style(muted_text_style)
                .wrapping(Wrapping::Word),
            Space::new().width(Fill),
            Space::new().width(SORT_ICON_LEFT_MARGIN),
            time_sort_icon,
            Space::new().width(SORT_ICON_RIGHT_MARGIN),
            // Trailing gap so the rightmost column doesn't abut the scrollbar.
            Space::new().width(SCROLLBAR_PADDING),
        ]
        .align_y(Center)
        .into();
        let time_header: Element<'static, Message> = button(time_header_content)
            .padding(NO_SPACING)
            .width(Length::Shrink)
            .style(transparent_icon_button_style)
            .on_press(Message::ConnectionMonitorTransferSortBy(
                TransferSortColumn::Time,
            ))
            .into();

        // Time column. Trailing Space matches the header so the rightmost
        // column doesn't abut the scrollbar.
        let time_column = table::column(time_header, |transfer: TransferInfo| {
            let time_str = format_elapsed_time(transfer.started_at);
            row![
                shaped_text(time_str)
                    .size(TEXT_SIZE)
                    .style(muted_text_style)
                    .wrapping(Wrapping::Word),
                Space::new().width(SCROLLBAR_PADDING),
            ]
            .align_y(Center)
        })
        .width(Length::Shrink);

        // Build the table
        let columns = [
            direction_column,
            user_column,
            ip_column,
            path_column,
            progress_column,
            time_column,
        ];

        table(columns, deps.transfers.clone())
            .width(Fill)
            .padding_x(SPACER_SIZE_SMALL)
            .padding_y(SPACER_SIZE_SMALL)
            .separator_x(NO_SPACING)
            .separator_y(SEPARATOR_HEIGHT)
    })
    .into()
}

/// Sort connections based on column and direction
fn sort_connections(
    connections: &mut [ConnectionInfo],
    column: ConnectionMonitorSortColumn,
    ascending: bool,
) {
    connections.sort_by(|a, b| {
        let cmp = match column {
            ConnectionMonitorSortColumn::Nickname => {
                fold_name(&a.nickname).cmp(&fold_name(&b.nickname))
            }
            ConnectionMonitorSortColumn::Username => {
                fold_name(&a.username).cmp(&fold_name(&b.username))
            }
            ConnectionMonitorSortColumn::Ip => a.ip.cmp(&b.ip),
            ConnectionMonitorSortColumn::Connected => a.login_time.cmp(&b.login_time),
        };
        if ascending { cmp } else { cmp.reverse() }
    });
}

/// Sort transfers based on column and direction
fn sort_transfers(transfers: &mut [TransferInfo], column: TransferSortColumn, ascending: bool) {
    transfers.sort_by(|a, b| {
        let cmp = match column {
            TransferSortColumn::User => fold_name(&a.nickname).cmp(&fold_name(&b.nickname)),
            TransferSortColumn::Ip => a.ip.cmp(&b.ip),
            TransferSortColumn::Direction => a.direction.cmp(&b.direction),
            TransferSortColumn::Path => a.path.to_lowercase().cmp(&b.path.to_lowercase()),
            TransferSortColumn::Progress => a.bytes_transferred.cmp(&b.bytes_transferred),
            TransferSortColumn::Time => a.started_at.cmp(&b.started_at),
        };
        if ascending { cmp } else { cmp.reverse() }
    });
}

/// Build the connections tab content
fn connections_tab_content<'a>(
    state: &'a ConnectionMonitorState,
    theme: &Theme,
    permissions: ConnectionMonitorPermissions,
) -> Element<'a, Message> {
    match &state.connections {
        None => {
            // Loading state
            container(
                shaped_text(t("connection-monitor-loading"))
                    .size(TEXT_SIZE)
                    .style(muted_text_style),
            )
            .width(Fill)
            .center_x(Fill)
            .padding(SPACER_SIZE_SMALL)
            .into()
        }
        Some(Err(error)) => {
            // Error state
            container(
                shaped_text_wrapped(error)
                    .size(TEXT_SIZE)
                    .style(error_text_style),
            )
            .width(Fill)
            .center_x(Fill)
            .padding(SPACER_SIZE_SMALL)
            .into()
        }
        Some(Ok(connections)) => {
            if connections.is_empty() {
                // Empty state (shouldn't happen since requester is connected)
                container(
                    shaped_text(t("connection-monitor-no-connections"))
                        .size(TEXT_SIZE)
                        .style(muted_text_style),
                )
                .width(Fill)
                .center_x(Fill)
                .padding(SPACER_SIZE_SMALL)
                .into()
            } else {
                // Sort connections based on current settings
                let mut sorted_connections = connections.clone();
                sort_connections(
                    &mut sorted_connections,
                    state.sort_column,
                    state.sort_ascending,
                );

                let deps = ConnectionTableDeps {
                    connections: sorted_connections,
                    sort_column: state.sort_column,
                    sort_ascending: state.sort_ascending,
                    admin_color: chat::admin(theme),
                    shared_color: chat::shared(theme),
                    permissions,
                };

                lazy_connection_table(deps)
            }
        }
    }
}

/// Build the transfers tab content
fn transfers_tab_content<'a>(
    state: &'a ConnectionMonitorState,
    theme: &Theme,
) -> Element<'a, Message> {
    match &state.transfers {
        None => {
            // Loading state
            container(
                shaped_text(t("connection-monitor-loading"))
                    .size(TEXT_SIZE)
                    .style(muted_text_style),
            )
            .width(Fill)
            .center_x(Fill)
            .padding(SPACER_SIZE_SMALL)
            .into()
        }
        Some(Err(error)) => {
            // Error state
            container(
                shaped_text_wrapped(error)
                    .size(TEXT_SIZE)
                    .style(error_text_style),
            )
            .width(Fill)
            .center_x(Fill)
            .padding(SPACER_SIZE_SMALL)
            .into()
        }
        Some(Ok(transfers)) => {
            if transfers.is_empty() {
                // Empty state
                container(
                    shaped_text(t("connection-monitor-no-transfers"))
                        .size(TEXT_SIZE)
                        .style(muted_text_style),
                )
                .width(Fill)
                .center_x(Fill)
                .padding(SPACER_SIZE_SMALL)
                .into()
            } else {
                // Sort transfers based on current settings
                let mut sorted_transfers = transfers.clone();
                sort_transfers(
                    &mut sorted_transfers,
                    state.transfer_sort_column,
                    state.transfer_sort_ascending,
                );

                let deps = TransferTableDeps {
                    transfers: sorted_transfers,
                    sort_column: state.transfer_sort_column,
                    sort_ascending: state.transfer_sort_ascending,
                    admin_color: chat::admin(theme),
                    shared_color: chat::shared(theme),
                };

                lazy_transfer_table(deps)
            }
        }
    }
}

/// Render the Connection Monitor panel
pub fn connection_monitor_view<'a>(
    conn: &'a ServerConnection,
    state: &'a ConnectionMonitorState,
    theme: Theme,
) -> Element<'a, Message> {
    // Build permissions struct for context menu
    let permissions = ConnectionMonitorPermissions {
        user_info: conn.has_permission(PERMISSION_USER_INFO),
        user_kick: conn.has_permission(PERMISSION_USER_KICK),
        ban_create: conn.has_permission(PERMISSION_BAN_CREATE),
    };

    // Refresh button with tooltip
    let refresh_btn: Element<'_, Message> = {
        let refresh_icon = container(icon::refresh().size(ICON_SIZE))
            .width(ICON_SIZE)
            .height(ICON_SIZE)
            .align_x(alignment::Horizontal::Center)
            .align_y(alignment::Vertical::Center);

        tooltip(
            button(refresh_icon)
                .on_press(Message::RefreshConnectionMonitor)
                .padding(HEADING_BUTTON_PADDING)
                .style(transparent_icon_button_style),
            container(shaped_text(t("tooltip-files-refresh")).size(TOOLTIP_TEXT_SIZE))
                .padding(TOOLTIP_BACKGROUND_PADDING)
                .style(tooltip_container_style),
            tooltip::Position::Top,
        )
        .gap(TOOLTIP_GAP)
        .padding(TOOLTIP_PADDING)
        .into()
    };

    // Title row with refresh button on the right.
    // No leading/trailing `SCROLLBAR_PADDING` gutters — the body
    // (table widgets inside tabs) reserves its scrollbar slot
    // internally. Button lands at `form_right - 20`, matching the
    // news/transfers buttons' inset.
    let button_width = ICON_SIZE + HEADING_BUTTON_PADDING.left + HEADING_BUTTON_PADDING.right;
    let title_row: Element<'_, Message> = row![
        Space::new().width(button_width),
        shaped_text(t("panel-connection-monitor"))
            .size(TITLE_SIZE)
            .width(Fill)
            .align_x(Center),
        refresh_btn,
    ]
    .align_y(Center)
    .into();

    // Build tab content
    let connections_content = connections_tab_content(state, &theme, permissions);
    let transfers_content = transfers_tab_content(state, &theme);

    // Build tab labels with counts
    let connections_count = match &state.connections {
        Some(Ok(conns)) => conns.len().to_string(),
        _ => "…".to_string(),
    };
    let transfers_count = match &state.transfers {
        Some(Ok(transfers)) => transfers.len().to_string(),
        _ => "…".to_string(),
    };
    let connections_label = format!("{} ({})", t("tab-connections"), connections_count);
    let transfers_label = format!("{} ({})", t("tab-transfers"), transfers_count);

    // Create tabs widget with spacer under tab bar
    let tabs = Tabs::new(Message::ConnectionMonitorTabSelected)
        .push(
            ConnectionMonitorTab::Connections,
            TabLabel::Text(connections_label),
            column![
                Space::new().height(SPACER_SIZE_MEDIUM),
                scrollable(connections_content).height(Fill),
            ]
            .height(Fill),
        )
        .push(
            ConnectionMonitorTab::Transfers,
            TabLabel::Text(transfers_label),
            column![
                Space::new().height(SPACER_SIZE_MEDIUM),
                scrollable(transfers_content).height(Fill),
            ]
            .height(Fill),
        )
        .set_active_tab(&state.active_tab)
        .tab_bar_position(iced_aw::TabBarPosition::Top)
        .text_size(TEXT_SIZE)
        .tab_label_padding(TAB_LABEL_PADDING);

    let form = column![
        title_row,
        Space::new().height(SPACER_SIZE_LARGE - SPACER_SIZE_MEDIUM),
        container(tabs).height(Fill),
    ]
    .spacing(SPACER_SIZE_MEDIUM)
    .align_x(Center)
    .padding(CONTENT_PADDING)
    .max_width(CONTENT_MAX_WIDTH)
    .height(Fill);

    // Center the form horizontally
    let centered_form = container(form).width(Fill).center_x(Fill);

    // Wrap everything in content background
    container(centered_form)
        .width(Fill)
        .height(Fill)
        .style(content_background_style)
        .into()
}
