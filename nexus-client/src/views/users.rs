//! User management panel view (list, create, edit, delete users)

use std::hash::{Hash, Hasher};

use iced::font;
use iced::widget::button as btn;
use iced::widget::text::Wrapping;
use iced::widget::{
    Column, Id, Row, Space, button, checkbox, column, container, lazy, pick_list, row, scrollable,
    table, text, text_input, tooltip,
};
use iced::{Center, Element, Fill, Theme, alignment};
use iced_aw::{TabLabel, Tabs};
use nexus_common::is_shared_account_permission;
use nexus_common::protocol::{GroupInfo, UserInfo};

use super::constants::{
    PERMISSION_GROUP_CREATE, PERMISSION_USER_CREATE, PERMISSION_USER_DELETE, PERMISSION_USER_EDIT,
};
use super::groups::{group_form_view, group_list_content};
use super::helpers::t_args;
use super::layout::scrollable_panel;
use crate::i18n::{t, translate_permission};
use crate::icon;
use crate::style::{
    BUTTON_PADDING, CONTENT_MAX_WIDTH, CONTENT_PADDING, CONTEXT_MENU_ITEM_PADDING,
    CONTEXT_MENU_MIN_WIDTH, CONTEXT_MENU_PADDING, CONTEXT_MENU_SEPARATOR_HEIGHT,
    CONTEXT_MENU_SEPARATOR_MARGIN, ELEMENT_SPACING, ICON_BUTTON_PADDING, INPUT_PADDING, NO_SPACING,
    SCROLLBAR_PADDING, SEPARATOR_HEIGHT, SIDEBAR_ACTION_ICON_SIZE, SORT_ICON_LEFT_MARGIN,
    SORT_ICON_RIGHT_MARGIN, SORT_ICON_SIZE, SPACER_SIZE_LARGE, SPACER_SIZE_MEDIUM,
    SPACER_SIZE_SMALL, TAB_LABEL_PADDING, TEXT_SIZE, TITLE_SIZE, TOOLTIP_BACKGROUND_PADDING,
    TOOLTIP_GAP, TOOLTIP_PADDING, TOOLTIP_TEXT_SIZE, chat, content_background_style,
    context_menu_container_style, error_text_style, menu_button_danger_style, menu_button_style,
    muted_text_style, panel_title, separator_style, shaped_text, shaped_text_wrapped,
    tooltip_container_style, transparent_icon_button_style,
};
use crate::types::InputId;
use crate::types::{
    GroupManagementMode, Message, ServerConnection, UserManagementMode, UserManagementSortColumn,
    UserManagementState, UserManagementTab,
};
use crate::widgets::{LazyContextMenu, MenuButton};

// ============================================================================
// Edit User Context
// ============================================================================

/// Guest account username (case-insensitive comparison)
const GUEST_USERNAME: &str = "guest";

// ============================================================================
// Group Option (for pick_list dropdown)
// ============================================================================

/// Represents a group option in the group dropdown (pick_list).
/// "None" option has id=None, groups have id=Some(i64).
#[derive(Debug, Clone)]
struct GroupOption {
    id: Option<i64>,
    label: String,
}

impl PartialEq for GroupOption {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl std::fmt::Display for GroupOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label)
    }
}

/// Build filtered group options for the pick_list dropdown.
///
/// Filters by:
/// - Shared compatibility: shared accounts → shared groups only, regular → non-shared only
/// - Non-admin delegation: non-admin users only see groups where they have all permissions
fn build_group_options(
    available_groups: Option<&[GroupInfo]>,
    conn: &ServerConnection,
    is_shared: bool,
) -> Vec<GroupOption> {
    let mut options = vec![GroupOption {
        id: None,
        label: t("user-management-group-none"),
    }];

    if let Some(groups) = available_groups {
        for group in groups {
            // Filter by shared compatibility
            if group.is_shared != is_shared {
                continue;
            }

            // Non-admin delegation: only show groups where user has all permissions
            if !conn.is_admin && !group.permissions.iter().all(|p| conn.has_permission(p)) {
                continue;
            }

            options.push(GroupOption {
                id: Some(group.id),
                label: group.name.clone(),
            });
        }
    }

    options
}

/// Context for rendering the edit user form
struct EditUserContext<'a> {
    /// Connection state (for permission checking)
    conn: &'a ServerConnection,
    /// User management state (for error display and available_groups)
    user_management: &'a UserManagementState,
    /// Original username (for display and update request)
    original_username: &'a str,
    /// New username (editable field)
    new_username: &'a str,
    /// New password (optional, empty = don't change)
    new_password: &'a str,
    /// Is admin flag
    is_admin: bool,
    /// Is shared account flag (immutable - display only)
    is_shared: bool,
    /// Is guest account (username/password cannot be changed)
    is_guest: bool,
    /// Enabled flag
    enabled: bool,
    /// Permissions list with enabled state (effective permissions)
    permissions: &'a [(String, bool)],
    /// Assigned group ID (None = no group)
    group_id: Option<i64>,
    /// Group's base permissions (for computing inherited vs override styling)
    group_permissions: &'a [String],
}

/// Build permission checkboxes split into two columns.
///
/// When `is_shared` is true, permissions not in `SHARED_ACCOUNT_PERMISSIONS` are disabled.
/// When `group_permissions` is Some, bold text indicates overrides (permission differs from group).
fn build_permission_columns<'a, F>(
    permissions: &'a [(String, bool)],
    conn: &'a ServerConnection,
    is_shared: bool,
    on_toggle: F,
    group_permissions: Option<&'a [String]>,
) -> Element<'a, Message>
where
    F: Fn(String, bool) -> Message + 'a + Clone,
{
    let bold_font = iced::Font {
        weight: font::Weight::Bold,
        ..iced::Font::default()
    };

    let mut left_column = Column::new().spacing(SPACER_SIZE_SMALL);
    let mut right_column = Column::new().spacing(SPACER_SIZE_SMALL);

    for (index, (permission, enabled)) in permissions.iter().enumerate() {
        let perm_name = permission.clone();
        let display_name = translate_permission(permission);
        let on_toggle_clone = on_toggle.clone();

        // Check if this permission is allowed for the current user
        let user_can_toggle = conn.has_permission(permission);

        // Check if this permission is forbidden for shared accounts
        let forbidden_for_shared = is_shared && !is_shared_account_permission(permission);

        // Determine if this permission is an override (differs from group).
        // Bold = user's effective state differs from what the group provides.
        // ☑ inherited (normal) | ☑ grant override (bold) | ☐ revoke override (bold) | ☐ normal
        let is_override = group_permissions
            .map(|gp| *enabled != gp.iter().any(|p| p == permission))
            .unwrap_or(false);

        // Build checkbox: base with label, size, shaping, then conditionally bold + on_toggle
        let base = checkbox(*enabled)
            .label(display_name)
            .size(TEXT_SIZE)
            .text_shaping(text::Shaping::Advanced);

        let base = if is_override {
            base.font(bold_font)
        } else {
            base
        };

        let checkbox_widget = if user_can_toggle && !forbidden_for_shared {
            base.on_toggle(move |checked| on_toggle_clone(perm_name.clone(), checked))
        } else {
            base
        };

        // Alternate between left and right columns
        if index % 2 == 0 {
            left_column = left_column.push(checkbox_widget);
        } else {
            right_column = right_column.push(checkbox_widget);
        }
    }

    row![left_column.width(Fill), right_column.width(Fill)]
        .spacing(ELEMENT_SPACING)
        .width(Fill)
        .into()
}

// ============================================================================
// User Table (lazy)
// ============================================================================

/// Dependencies for lazy user table rendering
#[derive(Clone)]
struct UserTableDeps {
    users: Vec<UserInfo>,
    sort_column: UserManagementSortColumn,
    sort_ascending: bool,
    admin_color: iced::Color,
    shared_color: iced::Color,
    can_edit: bool,
    can_delete: bool,
    is_admin: bool,
    current_username: String,
}

impl Hash for UserTableDeps {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.users.len().hash(state);
        for user in &self.users {
            user.id.hash(state);
            user.username.hash(state);
            user.is_admin.hash(state);
            user.is_shared.hash(state);
            user.group_name.hash(state);
        }
        self.sort_column.hash(state);
        self.sort_ascending.hash(state);
        self.can_edit.hash(state);
        self.can_delete.hash(state);
        self.is_admin.hash(state);
        self.current_username.hash(state);
    }
}

/// Sort users by column and direction
fn sort_users(users: &mut [UserInfo], column: UserManagementSortColumn, ascending: bool) {
    users.sort_by(|a, b| {
        let cmp = match column {
            UserManagementSortColumn::Username => {
                a.username.to_lowercase().cmp(&b.username.to_lowercase())
            }
            UserManagementSortColumn::Group => {
                let a_group = a.group_name.as_deref().unwrap_or("");
                let b_group = b.group_name.as_deref().unwrap_or("");
                a_group.to_lowercase().cmp(&b_group.to_lowercase())
            }
        };
        if ascending { cmp } else { cmp.reverse() }
    });
}

/// Build a lazy user table widget
fn lazy_user_table(deps: UserTableDeps) -> Element<'static, Message> {
    let can_edit = deps.can_edit;
    let can_delete = deps.can_delete;
    let is_admin = deps.is_admin;
    let current_username = deps.current_username.clone();

    lazy(deps, move |deps| {
        let admin_color = deps.admin_color;
        let shared_color = deps.shared_color;

        // Username column header (sortable, stable layout)
        let username_sort_icon: Element<'static, Message> =
            if deps.sort_column == UserManagementSortColumn::Username {
                let icon = if deps.sort_ascending {
                    icon::down_dir()
                } else {
                    icon::up_dir()
                };
                icon.size(SORT_ICON_SIZE).style(muted_text_style).into()
            } else {
                Space::new().width(SORT_ICON_SIZE).into()
            };
        let username_header_content: Element<'static, Message> = row![
            shaped_text(t("col-username"))
                .size(TEXT_SIZE)
                .wrapping(Wrapping::Word)
                .style(muted_text_style),
            Space::new().width(Fill),
            Space::new().width(SORT_ICON_LEFT_MARGIN),
            username_sort_icon,
            Space::new().width(SORT_ICON_RIGHT_MARGIN),
        ]
        .align_y(Center)
        .into();
        let username_header: Element<'static, Message> = button(username_header_content)
            .padding(NO_SPACING)
            .width(Fill)
            .style(transparent_icon_button_style)
            .on_press(Message::UserManagementSortBy(
                UserManagementSortColumn::Username,
            ))
            .into();

        // Username column - with admin/shared coloring and context menu
        let current_username_for_col = current_username.clone();
        let username_column = table::column(username_header, move |user: UserInfo| {
            let user_id = user.id;
            let username_for_menu = user.username.clone();
            let is_self = user.username.to_lowercase() == current_username_for_col.to_lowercase();
            let is_guest = user.username.to_lowercase() == GUEST_USERNAME;
            let can_edit_this = can_edit && !is_self && (is_admin || !user.is_admin);
            let can_delete_this =
                can_delete && !is_self && !is_guest && (is_admin || !user.is_admin);

            let text_widget = if user.is_admin {
                shaped_text(user.username)
                    .size(TEXT_SIZE)
                    .wrapping(Wrapping::Glyph)
                    .color(admin_color)
            } else if user.is_shared {
                shaped_text(user.username)
                    .size(TEXT_SIZE)
                    .wrapping(Wrapping::Glyph)
                    .color(shared_color)
            } else {
                shaped_text(user.username)
                    .size(TEXT_SIZE)
                    .wrapping(Wrapping::Glyph)
            };

            let content: Element<'static, Message> = text_widget.into();

            if can_edit_this || can_delete_this {
                LazyContextMenu::new(content, move || {
                    build_user_context_menu(
                        user_id,
                        username_for_menu.clone(),
                        can_edit_this,
                        can_delete_this,
                    )
                })
                .into()
            } else {
                content
            }
        })
        .width(Fill);

        // Group column header (sortable, stable layout)
        let group_sort_icon: Element<'static, Message> =
            if deps.sort_column == UserManagementSortColumn::Group {
                let icon = if deps.sort_ascending {
                    icon::down_dir()
                } else {
                    icon::up_dir()
                };
                icon.size(SORT_ICON_SIZE).style(muted_text_style).into()
            } else {
                Space::new().width(SORT_ICON_SIZE).into()
            };
        let group_header_content: Element<'static, Message> = row![
            shaped_text(t("col-group"))
                .size(TEXT_SIZE)
                .wrapping(Wrapping::Word)
                .style(muted_text_style),
            Space::new().width(Fill),
            Space::new().width(SORT_ICON_LEFT_MARGIN),
            group_sort_icon,
            Space::new().width(SORT_ICON_RIGHT_MARGIN),
        ]
        .align_y(Center)
        .into();
        let group_header: Element<'static, Message> = button(group_header_content)
            .padding(NO_SPACING)
            .width(Fill)
            .style(transparent_icon_button_style)
            .on_press(Message::UserManagementSortBy(
                UserManagementSortColumn::Group,
            ))
            .into();

        // Group column
        let group_column = table::column(group_header, |user: UserInfo| {
            let label = user.group_name.unwrap_or_else(|| "—".to_string());
            shaped_text(label)
                .size(TEXT_SIZE)
                .wrapping(Wrapping::WordOrGlyph)
                .style(muted_text_style)
        })
        .width(Fill);

        // Build the table
        let columns = [username_column, group_column];

        table(columns, deps.users.clone())
            .width(Fill)
            .padding_x(SPACER_SIZE_SMALL)
            .padding_y(SPACER_SIZE_SMALL)
            .separator_x(NO_SPACING)
            .separator_y(SEPARATOR_HEIGHT)
    })
    .into()
}

/// Build a context menu for a user row (edit / delete actions).
fn build_user_context_menu(
    user_id: i64,
    username: String,
    can_edit_this: bool,
    can_delete_this: bool,
) -> Element<'static, Message> {
    let mut menu_items: Vec<Element<'_, Message>> = vec![];

    if can_edit_this {
        menu_items.push(
            MenuButton::new(shaped_text(t("button-edit")).size(TEXT_SIZE))
                .padding(CONTEXT_MENU_ITEM_PADDING)
                .width(Fill)
                .style(menu_button_style)
                .on_press(Message::UserManagementEditClicked(
                    user_id,
                    username.clone(),
                ))
                .into(),
        );
    }

    // Separator only when both actions are present
    if can_edit_this && can_delete_this {
        menu_items.push(
            container(Space::new())
                .width(Fill)
                .height(CONTEXT_MENU_SEPARATOR_HEIGHT)
                .style(separator_style)
                .into(),
        );
    }

    if can_delete_this {
        menu_items.push(
            MenuButton::new(shaped_text(t("button-delete")).size(TEXT_SIZE))
                .padding(CONTEXT_MENU_ITEM_PADDING)
                .width(Fill)
                .style(menu_button_danger_style)
                .on_press(Message::UserManagementDeleteClicked(
                    user_id,
                    username.clone(),
                ))
                .into(),
        );
    }

    container(Column::with_children(menu_items).spacing(CONTEXT_MENU_SEPARATOR_MARGIN))
        .width(CONTEXT_MENU_MIN_WIDTH)
        .padding(CONTEXT_MENU_PADDING)
        .style(context_menu_container_style)
        .into()
}

// ============================================================================
// List View
// ============================================================================

/// Build the user list view as a sortable table
fn list_view<'a>(
    conn: &'a ServerConnection,
    user_management: &'a UserManagementState,
    theme: &Theme,
) -> Element<'a, Message> {
    match &user_management.all_users {
        None => {
            // Loading state
            container(
                shaped_text(t("user-management-loading"))
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
        Some(Ok(users)) => {
            if users.is_empty() {
                container(
                    shaped_text(t("user-management-no-users"))
                        .size(TEXT_SIZE)
                        .style(muted_text_style),
                )
                .width(Fill)
                .center_x(Fill)
                .padding(SPACER_SIZE_SMALL)
                .into()
            } else {
                let mut sorted_users = users.clone();
                sort_users(
                    &mut sorted_users,
                    user_management.sort_column,
                    user_management.sort_ascending,
                );

                let deps = UserTableDeps {
                    users: sorted_users,
                    sort_column: user_management.sort_column,
                    sort_ascending: user_management.sort_ascending,
                    admin_color: chat::admin(theme),
                    shared_color: chat::shared(theme),
                    can_edit: conn.has_permission(PERMISSION_USER_EDIT),
                    can_delete: conn.has_permission(PERMISSION_USER_DELETE),
                    is_admin: conn.is_admin,
                    current_username: conn.connection_info.username.clone(),
                };

                scrollable(lazy_user_table(deps)).height(Fill).into()
            }
        }
    }
}

// ============================================================================
// Create View
// ============================================================================

/// Build the create user form
fn create_view<'a>(
    conn: &'a ServerConnection,
    user_management: &'a UserManagementState,
) -> Element<'a, Message> {
    let title = panel_title(t("title-user-create"));

    let can_create =
        !user_management.username.trim().is_empty() && !user_management.password.trim().is_empty();

    // Helper for on_submit
    let submit_action = if can_create {
        Message::UserManagementCreatePressed
    } else {
        Message::ValidateUserManagementCreate
    };

    let username_input = text_input(&t("placeholder-username"), &user_management.username)
        .on_input(Message::UserManagementUsernameChanged)
        .on_submit(submit_action.clone())
        .id(Id::from(InputId::AdminUsername))
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE);

    let password_input = text_input(&t("placeholder-password"), &user_management.password)
        .on_input(Message::UserManagementPasswordChanged)
        .on_submit(submit_action)
        .id(Id::from(InputId::AdminPassword))
        .secure(true)
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE);

    // Admin checkbox - disabled when is_shared is checked (shared accounts can't be admin)
    let admin_checkbox = if conn.is_admin && !user_management.is_shared {
        checkbox(user_management.is_admin)
            .label(t("label-admin"))
            .on_toggle(Message::UserManagementIsAdminToggled)
            .size(TEXT_SIZE)
            .text_shaping(text::Shaping::Advanced)
    } else {
        checkbox(user_management.is_admin)
            .label(t("label-admin"))
            .size(TEXT_SIZE)
            .text_shaping(text::Shaping::Advanced)
    };

    // Shared account checkbox - only admins can create shared accounts
    let shared_checkbox = if conn.is_admin {
        checkbox(user_management.is_shared)
            .label(t("label-shared-account"))
            .on_toggle(Message::UserManagementIsSharedToggled)
            .size(TEXT_SIZE)
            .text_shaping(text::Shaping::Advanced)
    } else {
        checkbox(user_management.is_shared)
            .label(t("label-shared-account"))
            .size(TEXT_SIZE)
            .text_shaping(text::Shaping::Advanced)
    };

    let enabled_checkbox = if conn.is_admin {
        checkbox(user_management.enabled)
            .label(t("label-enabled"))
            .on_toggle(Message::UserManagementEnabledToggled)
            .size(TEXT_SIZE)
            .text_shaping(text::Shaping::Advanced)
    } else {
        checkbox(user_management.enabled)
            .label(t("label-enabled"))
            .size(TEXT_SIZE)
            .text_shaping(text::Shaping::Advanced)
    };

    // Group dropdown
    let group_label = shaped_text(t("user-management-group")).size(TEXT_SIZE);
    let group_options = build_group_options(
        user_management.available_groups.as_deref(),
        conn,
        user_management.is_shared,
    );
    let selected_group = group_options
        .iter()
        .find(|o| o.id == user_management.create_group_id)
        .cloned();
    let group_picker = pick_list(group_options, selected_group, |option| {
        Message::UserManagementGroupSelected(option.id)
    })
    .text_size(TEXT_SIZE);
    let group_row = row![group_label, group_picker]
        .spacing(ELEMENT_SPACING)
        .align_y(Center);

    // Compute group permissions for override UI (if a group is selected).
    // Reference directly from available_groups to avoid lifetime issues.
    let create_group_perms: Option<&[String]> = user_management.create_group_id.and_then(|gid| {
        user_management
            .available_groups
            .as_ref()
            .and_then(|groups| groups.iter().find(|g| g.id == gid))
            .map(|g| g.permissions.as_slice())
    });

    let permissions_title = shaped_text(t("label-permissions")).size(TEXT_SIZE);
    let permissions_row = build_permission_columns(
        &user_management.permissions,
        conn,
        user_management.is_shared,
        Message::UserManagementPermissionToggled,
        create_group_perms,
    );

    let create_button = if can_create {
        button(shaped_text(t("button-create")).size(TEXT_SIZE))
            .on_press(Message::UserManagementCreatePressed)
            .padding(BUTTON_PADDING)
    } else {
        button(shaped_text(t("button-create")).size(TEXT_SIZE)).padding(BUTTON_PADDING)
    };

    let cancel_button = button(shaped_text(t("button-cancel")).size(TEXT_SIZE))
        .on_press(Message::CancelUserManagement)
        .padding(BUTTON_PADDING)
        .style(btn::secondary);

    let mut items: Vec<Element<'a, Message>> = vec![title.into()];

    // Show error if present
    if let Some(error) = &user_management.create_error {
        items.push(
            shaped_text_wrapped(error)
                .size(TEXT_SIZE)
                .width(Fill)
                .align_x(Center)
                .style(error_text_style)
                .into(),
        );
        items.push(Space::new().height(SPACER_SIZE_SMALL).into());
    } else {
        items.push(Space::new().height(SPACER_SIZE_MEDIUM).into());
    }

    items.extend([
        username_input.into(),
        password_input.into(),
        admin_checkbox.into(),
        shared_checkbox.into(),
        enabled_checkbox.into(),
        group_row.into(),
        Space::new().height(SPACER_SIZE_SMALL).into(),
        permissions_title.into(),
        permissions_row,
        Space::new().height(SPACER_SIZE_MEDIUM).into(),
        row![Space::new().width(Fill), cancel_button, create_button]
            .spacing(ELEMENT_SPACING)
            .into(),
    ]);

    let form = Column::with_children(items)
        .spacing(ELEMENT_SPACING)
        .padding(CONTENT_PADDING)
        .max_width(CONTENT_MAX_WIDTH);

    scrollable_panel(form)
}

// ============================================================================
// Edit View
// ============================================================================

/// Build the edit user form
fn edit_view<'a>(ctx: EditUserContext<'a>) -> Element<'a, Message> {
    let title = panel_title(t("title-update-user"));

    let subtitle = shaped_text_wrapped(ctx.original_username)
        .size(TEXT_SIZE)
        .width(Fill)
        .align_x(Center)
        .style(muted_text_style);

    let can_update = !ctx.new_username.trim().is_empty();

    // Helper for on_submit
    let submit_action = if can_update {
        Message::UserManagementUpdatePressed
    } else {
        Message::ValidateUserManagementEdit
    };

    // Username input - disabled for guest account (cannot be renamed)
    let username_input = if ctx.is_guest {
        text_input(&t("placeholder-username"), ctx.new_username)
            .id(Id::from(InputId::EditNewUsername))
            .padding(INPUT_PADDING)
            .size(TEXT_SIZE)
    } else {
        text_input(&t("placeholder-username"), ctx.new_username)
            .on_input(Message::UserManagementEditUsernameChanged)
            .on_submit(submit_action.clone())
            .id(Id::from(InputId::EditNewUsername))
            .padding(INPUT_PADDING)
            .size(TEXT_SIZE)
    };

    // Password input - disabled for guest account (password cannot be changed)
    let password_input = if ctx.is_guest {
        text_input(&t("placeholder-password-keep-current"), ctx.new_password)
            .id(Id::from(InputId::EditNewPassword))
            .secure(true)
            .padding(INPUT_PADDING)
            .size(TEXT_SIZE)
    } else {
        text_input(&t("placeholder-password-keep-current"), ctx.new_password)
            .on_input(Message::UserManagementEditPasswordChanged)
            .on_submit(submit_action)
            .id(Id::from(InputId::EditNewPassword))
            .secure(true)
            .padding(INPUT_PADDING)
            .size(TEXT_SIZE)
    };

    // Admin checkbox - disabled when is_shared (shared accounts can't be admin)
    let admin_checkbox = if ctx.conn.is_admin && !ctx.is_shared {
        checkbox(ctx.is_admin)
            .label(t("label-admin"))
            .on_toggle(Message::UserManagementEditIsAdminToggled)
            .size(TEXT_SIZE)
            .text_shaping(text::Shaping::Advanced)
    } else {
        checkbox(ctx.is_admin)
            .label(t("label-admin"))
            .size(TEXT_SIZE)
            .text_shaping(text::Shaping::Advanced)
    };

    // Shared account checkbox - always disabled in edit mode (is_shared is immutable)
    let shared_checkbox = checkbox(ctx.is_shared)
        .label(t("label-shared-account"))
        .size(TEXT_SIZE)
        .text_shaping(text::Shaping::Advanced);

    let enabled_checkbox = if ctx.conn.is_admin {
        checkbox(ctx.enabled)
            .label(t("label-enabled"))
            .on_toggle(Message::UserManagementEditEnabledToggled)
            .size(TEXT_SIZE)
            .text_shaping(text::Shaping::Advanced)
    } else {
        checkbox(ctx.enabled)
            .label(t("label-enabled"))
            .size(TEXT_SIZE)
            .text_shaping(text::Shaping::Advanced)
    };

    // Group dropdown
    let group_label = shaped_text(t("user-management-group")).size(TEXT_SIZE);
    let group_options = build_group_options(
        ctx.user_management.available_groups.as_deref(),
        ctx.conn,
        ctx.is_shared,
    );
    let selected_group = group_options.iter().find(|o| o.id == ctx.group_id).cloned();
    let group_picker = pick_list(group_options, selected_group, |option| {
        Message::UserManagementEditGroupSelected(option.id)
    })
    .text_size(TEXT_SIZE);
    let group_row = row![group_label, group_picker]
        .spacing(ELEMENT_SPACING)
        .align_y(Center);

    // Override UI: pass group_permissions when user has a group assigned
    let group_perms = if ctx.group_id.is_some() {
        Some(ctx.group_permissions)
    } else {
        None
    };

    let permissions_title = shaped_text(t("label-permissions")).size(TEXT_SIZE);
    let permissions_row = build_permission_columns(
        ctx.permissions,
        ctx.conn,
        ctx.is_shared,
        Message::UserManagementEditPermissionToggled,
        group_perms,
    );

    let update_button = if can_update {
        button(shaped_text(t("button-update")).size(TEXT_SIZE))
            .on_press(Message::UserManagementUpdatePressed)
            .padding(BUTTON_PADDING)
    } else {
        button(shaped_text(t("button-update")).size(TEXT_SIZE)).padding(BUTTON_PADDING)
    };

    let cancel_button = button(shaped_text(t("button-cancel")).size(TEXT_SIZE))
        .on_press(Message::CancelUserManagement)
        .padding(BUTTON_PADDING)
        .style(btn::secondary);

    let mut items: Vec<Element<'a, Message>> = vec![title.into(), subtitle.into()];

    // Show error if present
    if let Some(error) = &ctx.user_management.edit_error {
        items.push(
            shaped_text_wrapped(error)
                .size(TEXT_SIZE)
                .width(Fill)
                .align_x(Center)
                .style(error_text_style)
                .into(),
        );
        items.push(Space::new().height(SPACER_SIZE_SMALL).into());
    } else {
        items.push(Space::new().height(SPACER_SIZE_MEDIUM).into());
    }

    items.extend([
        username_input.into(),
        password_input.into(),
        admin_checkbox.into(),
        shared_checkbox.into(),
        enabled_checkbox.into(),
        group_row.into(),
        Space::new().height(SPACER_SIZE_SMALL).into(),
        permissions_title.into(),
        permissions_row,
        Space::new().height(SPACER_SIZE_MEDIUM).into(),
        row![Space::new().width(Fill), cancel_button, update_button]
            .spacing(ELEMENT_SPACING)
            .into(),
    ]);

    let form = Column::with_children(items)
        .spacing(ELEMENT_SPACING)
        .padding(CONTENT_PADDING)
        .max_width(CONTENT_MAX_WIDTH);

    scrollable_panel(form)
}

// ============================================================================
// Delete Confirmation Modal
// ============================================================================

/// Build the delete confirmation modal
fn confirm_delete_modal<'a>(username: &'a str, error: Option<&'a String>) -> Element<'a, Message> {
    let title = panel_title(t("title-confirm-delete"));

    let message = shaped_text_wrapped(t_args("confirm-delete-user", &[("username", username)]))
        .size(TEXT_SIZE)
        .width(Fill)
        .align_x(Center);

    let confirm_button = button(shaped_text(t("button-delete")).size(TEXT_SIZE))
        .on_press(Message::UserManagementConfirmDelete)
        .padding(BUTTON_PADDING)
        .style(btn::danger);

    let cancel_button = button(shaped_text(t("button-cancel")).size(TEXT_SIZE))
        .on_press(Message::UserManagementCancelDelete)
        .padding(BUTTON_PADDING)
        .style(btn::secondary);

    let mut form_items: Vec<Element<'_, Message>> = vec![title.into()];

    // Show error if present
    if let Some(err) = error {
        form_items.push(
            shaped_text_wrapped(err)
                .size(TEXT_SIZE)
                .width(Fill)
                .align_x(Center)
                .style(error_text_style)
                .into(),
        );
        form_items.push(Space::new().height(SPACER_SIZE_SMALL).into());
    } else {
        form_items.push(Space::new().height(SPACER_SIZE_MEDIUM).into());
    }

    form_items.extend([
        message.into(),
        Space::new().height(SPACER_SIZE_MEDIUM).into(),
        row![Space::new().width(Fill), cancel_button, confirm_button]
            .spacing(ELEMENT_SPACING)
            .into(),
    ]);

    let form = Column::with_children(form_items)
        .spacing(ELEMENT_SPACING)
        .padding(CONTENT_PADDING)
        .max_width(CONTENT_MAX_WIDTH);

    scrollable_panel(form)
}

// ============================================================================
// Main View Function
// ============================================================================

/// Displays the user management panel.
///
/// Dispatches to one of:
/// - Tabbed list view (Users + Groups tabs) when both tabs are in List mode
/// - Full-panel user form (Create, Edit, ConfirmDelete) when user form is active
/// - Full-panel group form (Create, Edit, ConfirmDelete) when group form is active
pub fn users_view<'a>(
    conn: &'a ServerConnection,
    user_management: &'a UserManagementState,
    theme: &Theme,
) -> Element<'a, Message> {
    // If user management is in a form mode, show the form full-panel (no tabs)
    if user_management.mode != UserManagementMode::List {
        return match &user_management.mode {
            UserManagementMode::Create => create_view(conn, user_management),
            UserManagementMode::Edit {
                id: _,
                original_username,
                new_username,
                new_password,
                is_admin,
                is_shared,
                enabled,
                permissions,
                group_id,
                group_permissions,
                ..
            } => edit_view(EditUserContext {
                conn,
                user_management,
                original_username,
                new_username,
                new_password,
                is_admin: *is_admin,
                is_shared: *is_shared,
                is_guest: original_username.to_lowercase() == GUEST_USERNAME,
                enabled: *enabled,
                permissions,
                group_id: *group_id,
                group_permissions,
            }),
            UserManagementMode::ConfirmDelete { id: _, username } => {
                confirm_delete_modal(username, user_management.delete_error.as_ref())
            }
            UserManagementMode::List => unreachable!(),
        };
    }

    // If group management is in a form mode, show the group form full-panel (no tabs)
    if user_management.group_management.mode != GroupManagementMode::List {
        return group_form_view(conn, user_management);
    }

    // Both tabs are in List mode — show the tabbed list view
    tabbed_list_view(conn, user_management, theme)
}

// ============================================================================
// Tabbed List View
// ============================================================================

/// Build the tabbed list view with Users and Groups tabs.
///
/// Only shown when both user management and group management are in List mode.
fn tabbed_list_view<'a>(
    conn: &'a ServerConnection,
    user_management: &'a UserManagementState,
    theme: &Theme,
) -> Element<'a, Message> {
    // Build tab content
    let users_content = list_view(conn, user_management, theme);
    let groups_content = group_list_content(conn, user_management);

    // Tab labels with counts
    let users_count = match &user_management.all_users {
        Some(Ok(users)) => users.len().to_string(),
        _ => "…".to_string(),
    };
    let groups_count = match &user_management.available_groups {
        Some(groups) => groups.len().to_string(),
        _ => "…".to_string(),
    };
    let users_label = format!("{} ({})", t("tab-users"), users_count);
    let groups_label = format!("{} ({})", t("tab-groups"), groups_count);

    // Create tabs widget
    let tabs = Tabs::new(Message::UserManagementTabSelected)
        .push(
            UserManagementTab::Users,
            TabLabel::Text(users_label),
            column![Space::new().height(SPACER_SIZE_MEDIUM), users_content,].height(Fill),
        )
        .push(
            UserManagementTab::Groups,
            TabLabel::Text(groups_label),
            column![Space::new().height(SPACER_SIZE_MEDIUM), groups_content,].height(Fill),
        )
        .set_active_tab(&user_management.active_tab)
        .tab_bar_position(iced_aw::TabBarPosition::Top)
        .text_size(TEXT_SIZE)
        .tab_label_padding(TAB_LABEL_PADDING);

    // Add user button (permission-gated)
    let add_user_btn: Option<Element<'_, Message>> = if conn.has_permission(PERMISSION_USER_CREATE)
    {
        let add_icon = container(icon::user_plus().size(SIDEBAR_ACTION_ICON_SIZE))
            .width(SIDEBAR_ACTION_ICON_SIZE)
            .height(SIDEBAR_ACTION_ICON_SIZE)
            .align_x(alignment::Horizontal::Center)
            .align_y(alignment::Vertical::Center);

        Some(
            tooltip(
                button(add_icon)
                    .on_press(Message::UserManagementShowCreate)
                    .padding(ICON_BUTTON_PADDING)
                    .style(transparent_icon_button_style),
                container(shaped_text(t("tooltip-create-user")).size(TOOLTIP_TEXT_SIZE))
                    .padding(TOOLTIP_BACKGROUND_PADDING)
                    .style(tooltip_container_style),
                tooltip::Position::Top,
            )
            .gap(TOOLTIP_GAP)
            .padding(TOOLTIP_PADDING)
            .into(),
        )
    } else {
        None
    };

    // Add group button (permission-gated)
    let add_group_btn: Option<Element<'_, Message>> =
        if conn.has_permission(PERMISSION_GROUP_CREATE) {
            let add_icon = container(icon::plus_circled().size(SIDEBAR_ACTION_ICON_SIZE))
                .width(SIDEBAR_ACTION_ICON_SIZE)
                .height(SIDEBAR_ACTION_ICON_SIZE)
                .align_x(alignment::Horizontal::Center)
                .align_y(alignment::Vertical::Center);

            Some(
                tooltip(
                    button(add_icon)
                        .on_press(Message::GroupManagementShowCreate)
                        .padding(ICON_BUTTON_PADDING)
                        .style(transparent_icon_button_style),
                    container(shaped_text(t("tooltip-create-group")).size(TOOLTIP_TEXT_SIZE))
                        .padding(TOOLTIP_BACKGROUND_PADDING)
                        .style(tooltip_container_style),
                    tooltip::Position::Top,
                )
                .gap(TOOLTIP_GAP)
                .padding(TOOLTIP_PADDING)
                .into(),
            )
        } else {
            None
        };

    // Count visible buttons for balancing
    let button_count = [add_user_btn.is_some(), add_group_btn.is_some()]
        .iter()
        .filter(|&&b| b)
        .count();
    let button_width =
        SIDEBAR_ACTION_ICON_SIZE + ICON_BUTTON_PADDING.left + ICON_BUTTON_PADDING.right;
    let total_button_width = button_width * button_count as f32;

    // Title row: [scrollbar_pad][balancer]  Title  [+user][+group][scrollbar_pad]
    let mut title_row = Row::new().align_y(Center);
    title_row = title_row.push(Space::new().width(SCROLLBAR_PADDING));
    title_row = title_row.push(Space::new().width(total_button_width));
    title_row = title_row.push(
        shaped_text(t("title-user-management"))
            .size(TITLE_SIZE)
            .width(Fill)
            .align_x(Center),
    );
    if let Some(btn) = add_user_btn {
        title_row = title_row.push(btn);
    }
    if let Some(btn) = add_group_btn {
        title_row = title_row.push(btn);
    }
    title_row = title_row.push(Space::new().width(SCROLLBAR_PADDING));

    // Build the form with max_width constraint
    let form = column![
        Element::<'_, Message>::from(title_row),
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

    // Wrap in content background
    container(centered_form)
        .width(Fill)
        .height(Fill)
        .style(content_background_style)
        .into()
}
