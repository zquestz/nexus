//! User management panel view (list, create, edit, delete users)

use std::hash::{Hash, Hasher};

use iced::font;
use iced::widget::button as btn;
use iced::widget::text::Wrapping;
use iced::widget::{
    Column, Id, Space, button, checkbox, column, container, lazy, pick_list, row, scrollable,
    table, text, text_input,
};
use iced::{Center, Element, Fill, Length, Theme};
use iced_aw::{ContextMenu, NumberInput, TabLabel, Tabs};
use nexus_common::is_shared_account_permission;
use nexus_common::names::fold_name;
use nexus_common::protocol::{GroupInfo, UserInfo};
use nexus_common::validators::{MIN_BANDWIDTH_WEIGHT, resolve_bandwidth_weight};

use super::constants::{PERMISSION_USER_CREATE, PERMISSION_USER_DELETE, PERMISSION_USER_EDIT};
use super::groups::{group_form_view, group_list_content};
use super::helpers::{sort_icon_or_placeholder, t_args, tab_toolbar_icon_button};
use super::layout::scrollable_panel;
use super::password_strength::password_strength_bar;
use crate::i18n::{t, translate_permission};
use crate::icon;
use crate::style::{
    BUTTON_PADDING, CONTENT_MAX_WIDTH, CONTENT_PADDING, CONTEXT_MENU_ITEM_PADDING,
    CONTEXT_MENU_MIN_WIDTH, CONTEXT_MENU_PADDING, CONTEXT_MENU_SEPARATOR_HEIGHT,
    CONTEXT_MENU_SEPARATOR_MARGIN, ELEMENT_SPACING, INPUT_PADDING, NO_SPACING, SCROLLBAR_PADDING,
    SEPARATOR_HEIGHT, SORT_ICON_LEFT_MARGIN, SORT_ICON_RIGHT_MARGIN, SPACER_SIZE_LARGE,
    SPACER_SIZE_MEDIUM, SPACER_SIZE_SMALL, TAB_LABEL_PADDING, TEXT_SIZE, chat,
    clickable_text_style, content_background_style, context_menu_container_style, error_text_style,
    menu_button_danger_style, menu_button_style, muted_text_style, panel_title, separator_style,
    shaped_text, shaped_text_wrapped, transparent_icon_button_style,
};
use crate::types::InputId;
use crate::types::{
    GroupManagementMode, Message, ServerConnection, UserManagementMode, UserManagementSortColumn,
    UserManagementState, UserManagementTab,
};
use crate::widgets::MenuButton;

// ============================================================================
// Edit User Context
// ============================================================================

/// Guest account username (case-insensitive comparison)
const GUEST_USERNAME: &str = "guest";

fn hash_color<H: Hasher>(color: iced::Color, state: &mut H) {
    color.r.to_bits().hash(state);
    color.g.to_bits().hash(state);
    color.b.to_bits().hash(state);
    color.a.to_bits().hash(state);
}

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

/// Build the bandwidth-weight row used by both create and edit user forms.
///
/// The Inherit checkbox is always visible. Baseline resolution mirrors the
/// server (`get_resolved_bandwidth_weight`):
/// - **Target is admin** → `DEFAULT_ADMIN_BANDWIDTH_WEIGHT` (admins skip
///   group lookup, even if `group_weight` is somehow set).
/// - **Otherwise** → selected group's weight, falling back to
///   `DEFAULT_BANDWIDTH_WEIGHT`.
///
/// - **Inherit checked** → number input disabled, displays the baseline;
///   the form submit sends `inherit_bandwidth_weight: Some(true)`.
/// - **Inherit unchecked** → number input enabled, displays the typed
///   override (falling back to the baseline when no override is set
///   yet). Renders **bold** when the value differs from the baseline,
///   mirroring the permission-override convention.
fn build_bandwidth_row<'a>(
    override_value: Option<u16>,
    inherit: bool,
    group_weight: Option<u16>,
    target_is_admin: bool,
    on_value_changed: fn(u16) -> Message,
    on_inherit_toggled: fn(bool) -> Message,
    input_id: InputId,
) -> Element<'a, Message> {
    let bold_font = iced::Font {
        weight: font::Weight::Bold,
        ..iced::Font::default()
    };
    let label = shaped_text(t("label-bandwidth-weight")).size(TEXT_SIZE);

    // Baseline = the effective weight the resolver would return if the
    // user had no per-user override. Shared with the server's
    // `resolve_bandwidth_weight` so the displayed baseline matches what
    // the scheduler will apply.
    let baseline = resolve_bandwidth_weight(None, group_weight, target_is_admin);
    let disable_input = inherit;

    // Number-input display value picks the right source for the active
    // state: when inheriting we want the baseline visible (read-only),
    // otherwise the user's typed override (or the baseline as a sensible
    // starting point if no override is set yet).
    let display_value: u16 = if disable_input {
        baseline
    } else {
        override_value.unwrap_or(baseline)
    };

    let mut input = NumberInput::new(
        &display_value,
        MIN_BANDWIDTH_WEIGHT..=u16::MAX,
        on_value_changed,
    );
    if disable_input {
        input = input.on_input_maybe(None::<fn(u16) -> Message>);
    }
    let number_input: Element<'a, Message> =
        input.id(Id::from(input_id)).padding(INPUT_PADDING).into();

    // Bold marker for "override differs from baseline" — only meaningful
    // when Inherit is unchecked (otherwise the input is showing the
    // baseline read-only and there's nothing to highlight).
    let is_override_marker = !inherit && override_value.is_some_and(|o| o != baseline);

    let label_or_marker: Element<'a, Message> = if is_override_marker {
        shaped_text(t("label-bandwidth-weight"))
            .size(TEXT_SIZE)
            .font(bold_font)
            .into()
    } else {
        label.into()
    };

    let inherit_box = checkbox(inherit)
        .label(t("label-inherit-bandwidth-weight"))
        .on_toggle(on_inherit_toggled)
        .size(TEXT_SIZE)
        .text_shaping(text::Shaping::Advanced);

    iced::widget::Row::with_children(vec![
        label_or_marker,
        Space::new().width(ELEMENT_SPACING).into(),
        number_input,
        Space::new().width(ELEMENT_SPACING).into(),
        inherit_box.into(),
    ])
    .align_y(Center)
    .into()
}

/// Build filtered group options for the pick_list dropdown.
///
/// Filters by:
/// - Admin XOR group invariant: admins cannot be members of any group; "None" only.
/// - Shared compatibility: shared accounts → shared groups only, regular → non-shared only
/// - Non-admin delegation: non-admin users only see groups where they have all permissions
fn build_group_options(
    available_groups: Option<&[GroupInfo]>,
    conn: &ServerConnection,
    is_shared: bool,
    is_admin: bool,
) -> Vec<GroupOption> {
    let mut options = vec![GroupOption {
        id: None,
        label: t("user-management-group-none"),
    }];

    // Admin XOR group: target is admin → no group options.
    if is_admin {
        return options;
    }

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
    /// Bandwidth weight override (form value; None = no individual override).
    bandwidth_weight_override: Option<u16>,
    /// "Inherit from group" checkbox state.
    bandwidth_weight_inherit: bool,
    /// Whether the current form contains an effective update.
    has_effective_changes: bool,
}

/// Build permission checkboxes split into two columns.
///
/// When `is_shared` is true, permissions not in `SHARED_ACCOUNT_PERMISSIONS` are disabled.
/// When `group_permissions` is Some, bold text indicates overrides (permission differs from group).
fn build_permission_columns<'a, F>(
    permissions: &'a [(String, bool)],
    conn: &'a ServerConnection,
    is_shared: bool,
    target_is_admin: bool,
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

        // Admins resolve permissions via the admin override; any stored
        // grants/revokes are wiped on save. Show the stored state but
        // disable the toggle so we don't pretend it's editable.
        let checkbox_widget = if user_can_toggle && !forbidden_for_shared && !target_is_admin {
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
        hash_color(self.admin_color, state);
        hash_color(self.shared_color, state);
    }
}

/// Sort users by column and direction
fn sort_users(users: &mut [UserInfo], column: UserManagementSortColumn, ascending: bool) {
    users.sort_by(|a, b| {
        let cmp = match column {
            UserManagementSortColumn::Username => {
                fold_name(&a.username).cmp(&fold_name(&b.username))
            }
            UserManagementSortColumn::Group => {
                let a_group = a.group_name.as_deref().unwrap_or("");
                let b_group = b.group_name.as_deref().unwrap_or("");
                fold_name(a_group).cmp(&fold_name(b_group))
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
        let username_sort_icon = sort_icon_or_placeholder(
            deps.sort_column == UserManagementSortColumn::Username,
            deps.sort_ascending,
        );
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
            .width(Length::Shrink)
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
            let is_self = fold_name(&user.username) == fold_name(&current_username_for_col);
            let is_guest = fold_name(&user.username) == GUEST_USERNAME;
            // Admins can edit themselves (primarily to manage their own
            // username or bandwidth weight); non-admin self-edit still
            // routes to the Change Password dialog only.
            let can_edit_this = can_edit && (is_admin || !is_self) && (is_admin || !user.is_admin);
            let can_delete_this =
                can_delete && !is_self && !is_guest && (is_admin || !user.is_admin);
            // Change Password is always self-service for any non-shared
            // self row, even when can_edit_this is also true (admin self).
            let can_change_own_password = is_self && !user.is_shared;

            let username_color = if user.is_admin {
                Some(admin_color)
            } else if user.is_shared {
                Some(shared_color)
            } else {
                None
            };

            let text_widget = shaped_text(user.username)
                .size(TEXT_SIZE)
                .wrapping(Wrapping::Glyph);

            let content: Element<'static, Message> = if can_edit_this {
                // Color via button style so hover effect works
                button(text_widget)
                    .padding(NO_SPACING)
                    .width(Fill)
                    .style(clickable_text_style(username_color))
                    .on_press(Message::UserManagementEditClicked(
                        user_id,
                        username_for_menu.clone(),
                    ))
                    .into()
            } else if can_change_own_password {
                // Own row uses the same clickable-text style so the hover
                // affordance matches other editable rows; clicking opens the
                // Change Password dialog (symmetric with Edit on other rows).
                button(text_widget)
                    .padding(NO_SPACING)
                    .width(Fill)
                    .style(clickable_text_style(username_color))
                    .on_press(Message::ChangePasswordPressed)
                    .into()
            } else if let Some(color) = username_color {
                text_widget.color(color).into()
            } else {
                text_widget.into()
            };

            if can_edit_this || can_delete_this || can_change_own_password {
                ContextMenu::new(content, move || {
                    build_user_context_menu(
                        user_id,
                        username_for_menu.clone(),
                        can_edit_this,
                        can_delete_this,
                        can_change_own_password,
                    )
                })
                .into()
            } else {
                content
            }
        })
        .width(Length::FillPortion(1));

        // Group column header (sortable, stable layout)
        let group_sort_icon = sort_icon_or_placeholder(
            deps.sort_column == UserManagementSortColumn::Group,
            deps.sort_ascending,
        );
        let group_header_content: Element<'static, Message> = row![
            shaped_text(t("col-group"))
                .size(TEXT_SIZE)
                .wrapping(Wrapping::Word)
                .style(muted_text_style),
            Space::new().width(Fill),
            Space::new().width(SORT_ICON_LEFT_MARGIN),
            group_sort_icon,
            Space::new().width(SORT_ICON_RIGHT_MARGIN),
            // Trailing gap so the rightmost column doesn't abut the scrollbar.
            Space::new().width(SCROLLBAR_PADDING),
        ]
        .align_y(Center)
        .into();
        let group_header: Element<'static, Message> = button(group_header_content)
            .padding(NO_SPACING)
            .width(Length::Shrink)
            .style(transparent_icon_button_style)
            .on_press(Message::UserManagementSortBy(
                UserManagementSortColumn::Group,
            ))
            .into();

        // Group column. Trailing Space matches the header so the rightmost
        // column doesn't abut the scrollbar.
        let group_column = table::column(group_header, |user: UserInfo| {
            let label = user.group_name.unwrap_or_else(|| "—".to_string());
            row![
                shaped_text(label)
                    .size(TEXT_SIZE)
                    .wrapping(Wrapping::WordOrGlyph)
                    .style(muted_text_style),
                Space::new().width(SCROLLBAR_PADDING),
            ]
            .align_y(Center)
        })
        .width(Length::FillPortion(1));

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

/// Build a context menu for a user row (edit / delete on other rows, change
/// own password on own row).
fn build_user_context_menu(
    user_id: i64,
    username: String,
    can_edit_this: bool,
    can_delete_this: bool,
    can_change_own_password: bool,
) -> Element<'static, Message> {
    let mut menu_items: Vec<Element<'_, Message>> = vec![];

    let separator = || -> Element<'static, Message> {
        container(Space::new())
            .width(Fill)
            .height(CONTEXT_MENU_SEPARATOR_HEIGHT)
            .style(separator_style)
            .into()
    };

    if can_change_own_password {
        menu_items.push(
            MenuButton::new(shaped_text(t("button-change-password")).size(TEXT_SIZE))
                .padding(CONTEXT_MENU_ITEM_PADDING)
                .width(Fill)
                .style(menu_button_style)
                .on_press(Message::ChangePasswordPressed)
                .into(),
        );
    }

    if can_change_own_password && can_edit_this {
        menu_items.push(separator());
    }

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

    if can_edit_this && can_delete_this {
        menu_items.push(separator());
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

/// Build the Users tab toolbar (Create User + Refresh icon buttons).
///
/// Create User is permission-gated on `user_create`. Refresh is disabled
/// while a list fetch is in flight (`all_users.is_none()`); after an
/// error response, `all_users` becomes `Some(Err(_))`, so `is_some()`
/// re-enables the button — don't change this to `is_some_and(Result::is_ok)`
/// or the button would lock up after a failed fetch.
fn users_tab_toolbar<'a>(
    conn: &'a ServerConnection,
    user_management: &'a UserManagementState,
) -> Element<'a, Message> {
    let create_btn = tab_toolbar_icon_button(
        icon::user_plus(),
        "tooltip-user-create",
        Message::UserManagementShowCreate,
        conn.has_permission(PERMISSION_USER_CREATE),
    );
    let refresh_btn = tab_toolbar_icon_button(
        icon::refresh(),
        "tooltip-refresh",
        Message::UserManagementRefreshUsers,
        user_management.all_users.is_some(),
    );

    row![create_btn, refresh_btn]
        .spacing(SPACER_SIZE_SMALL)
        .into()
}

/// Build the user list view as a sortable table
fn list_view<'a>(
    conn: &'a ServerConnection,
    user_management: &'a UserManagementState,
    theme: &Theme,
) -> Element<'a, Message> {
    let toolbar = users_tab_toolbar(conn, user_management);

    let body: Element<'a, Message> = match &user_management.all_users {
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
    };

    column![toolbar, body]
        .spacing(SPACER_SIZE_SMALL)
        .height(Fill)
        .into()
}

// ============================================================================
// Create View
// ============================================================================

/// Build the create user form
fn create_view<'a>(
    conn: &'a ServerConnection,
    user_management: &'a UserManagementState,
    theme: &Theme,
) -> Element<'a, Message> {
    let title = panel_title(t("title-user-create"));

    let can_create = !user_management.username.trim().is_empty()
        && !user_management.password.trim().is_empty()
        && !user_management.is_submitting;

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
        user_management.loaded_groups(),
        conn,
        user_management.is_shared,
        user_management.is_admin,
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
    let create_group_perms: Option<&[String]> = user_management.create_group_id.and_then(|gid| {
        user_management
            .loaded_groups()
            .and_then(|groups| groups.iter().find(|g| g.id == gid))
            .map(|g| g.permissions.as_slice())
    });

    // Look up the selected group's bandwidth weight (for the override-vs-inherit display).
    let create_group_weight: Option<u16> = user_management.create_group_id.and_then(|gid| {
        user_management
            .loaded_groups()
            .and_then(|groups| groups.iter().find(|g| g.id == gid))
            .map(|g| g.bandwidth_weight)
    });
    let bandwidth_row = build_bandwidth_row(
        user_management.bandwidth_weight_override,
        user_management.bandwidth_weight_inherit,
        create_group_weight,
        user_management.is_admin,
        Message::UserManagementBandwidthWeightChanged,
        Message::UserManagementInheritBandwidthWeightToggled,
        InputId::CreateUserBandwidthWeight,
    );

    let permissions_title = shaped_text(t("label-permissions")).size(TEXT_SIZE);
    let permissions_row = build_permission_columns(
        &user_management.permissions,
        conn,
        user_management.is_shared,
        user_management.is_admin,
        Message::UserManagementPermissionToggled,
        create_group_perms,
    );

    let create_button = if can_create {
        button(shaped_text(t("button-save")).size(TEXT_SIZE))
            .on_press(Message::UserManagementCreatePressed)
            .padding(BUTTON_PADDING)
    } else {
        button(shaped_text(t("button-save")).size(TEXT_SIZE)).padding(BUTTON_PADDING)
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

    items.push(username_input.into());
    items.push(password_input.into());

    // Password strength bar (only shown when password is non-empty)
    if let Some(bar) = password_strength_bar(
        &user_management.password,
        &user_management.username,
        conn.min_password_strength,
        theme,
    ) {
        items.push(bar);
    }

    items.extend([
        admin_checkbox.into(),
        shared_checkbox.into(),
        enabled_checkbox.into(),
        group_row.into(),
        bandwidth_row,
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
fn edit_view<'a>(ctx: EditUserContext<'a>, theme: &Theme) -> Element<'a, Message> {
    let title = panel_title(t("title-user-edit"));

    let subtitle = shaped_text_wrapped(ctx.original_username)
        .size(TEXT_SIZE)
        .width(Fill)
        .align_x(Center)
        .style(muted_text_style);

    // Self-edit (only reachable as admin per row-level gate) routes
    // password change through the Change Password dialog and hard-disables
    // is_admin / enabled so the user can't accidentally lock themselves out.
    let is_self_edit =
        fold_name(ctx.original_username) == fold_name(&ctx.conn.connection_info.username);

    let can_update = ctx.has_effective_changes
        && !ctx.new_username.trim().is_empty()
        && !ctx.user_management.is_submitting
        && !ctx.user_management.edit_stale;

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
    // and for self-edit (Change Password dialog is the only path that
    // carries current_password verification).
    let password_input = if ctx.is_guest || is_self_edit {
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

    // Admin checkbox - disabled when shared (shared accounts can't be admin)
    // or when self-editing (server rejects self-demotion).
    let admin_checkbox = if ctx.conn.is_admin && !ctx.is_shared && !is_self_edit {
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

    // Enabled checkbox - admin-only, plus disabled for self (server rejects
    // self-disable).
    let enabled_checkbox = if ctx.conn.is_admin && !is_self_edit {
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
        ctx.user_management.loaded_groups(),
        ctx.conn,
        ctx.is_shared,
        ctx.is_admin,
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

    // Look up the selected group's bandwidth weight for the override
    // display (mirror of create_view).
    let edit_group_weight: Option<u16> = ctx.group_id.and_then(|gid| {
        ctx.user_management
            .loaded_groups()
            .and_then(|groups| groups.iter().find(|g| g.id == gid))
            .map(|g| g.bandwidth_weight)
    });
    let bandwidth_row = build_bandwidth_row(
        ctx.bandwidth_weight_override,
        ctx.bandwidth_weight_inherit,
        edit_group_weight,
        ctx.is_admin,
        Message::UserManagementEditBandwidthWeightChanged,
        Message::UserManagementEditInheritBandwidthWeightToggled,
        InputId::EditUserBandwidthWeight,
    );

    let permissions_title = shaped_text(t("label-permissions")).size(TEXT_SIZE);
    let permissions_row = build_permission_columns(
        ctx.permissions,
        ctx.conn,
        ctx.is_shared,
        ctx.is_admin,
        Message::UserManagementEditPermissionToggled,
        group_perms,
    );

    let update_button = if can_update {
        button(shaped_text(t("button-save")).size(TEXT_SIZE))
            .on_press(Message::UserManagementUpdatePressed)
            .padding(BUTTON_PADDING)
    } else {
        button(shaped_text(t("button-save")).size(TEXT_SIZE)).padding(BUTTON_PADDING)
    };

    let cancel_button = button(shaped_text(t("button-cancel")).size(TEXT_SIZE))
        .on_press(Message::CancelUserManagement)
        .padding(BUTTON_PADDING)
        .style(btn::secondary);

    let mut items: Vec<Element<'a, Message>> = vec![title.into(), subtitle.into()];

    let edit_banner = if ctx.user_management.edit_stale {
        Some(t("err-user-edit-stale"))
    } else {
        ctx.user_management.edit_error.clone()
    };

    // Show stale-edit notice or validation error if present.
    if let Some(error) = edit_banner {
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

    items.push(username_input.into());
    items.push(password_input.into());

    // Password strength bar (only shown when password is non-empty)
    if let Some(bar) = password_strength_bar(
        ctx.new_password,
        ctx.new_username,
        ctx.conn.min_password_strength,
        theme,
    ) {
        items.push(bar);
    }

    items.extend([
        admin_checkbox.into(),
        shared_checkbox.into(),
        enabled_checkbox.into(),
        group_row.into(),
        bandwidth_row,
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
fn confirm_delete_modal<'a>(
    username: &'a str,
    error: Option<&'a String>,
    is_delete_submitting: bool,
) -> Element<'a, Message> {
    let title = panel_title(t("title-confirm-delete"));

    let message = shaped_text_wrapped(t_args("confirm-delete-user", &[("username", username)]))
        .size(TEXT_SIZE)
        .width(Fill)
        .align_x(Center);

    let confirm_button = if is_delete_submitting {
        button(shaped_text(t("button-delete")).size(TEXT_SIZE))
            .padding(BUTTON_PADDING)
            .style(btn::danger)
    } else {
        button(shaped_text(t("button-delete")).size(TEXT_SIZE))
            .on_press(Message::UserManagementConfirmDelete)
            .padding(BUTTON_PADDING)
            .style(btn::danger)
    };

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
            UserManagementMode::Create => create_view(conn, user_management, theme),
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
                bandwidth_weight_override,
                bandwidth_weight_inherit,
                ..
            } => edit_view(
                EditUserContext {
                    conn,
                    user_management,
                    original_username,
                    new_username,
                    new_password,
                    is_admin: *is_admin,
                    is_shared: *is_shared,
                    is_guest: fold_name(original_username) == GUEST_USERNAME,
                    enabled: *enabled,
                    permissions,
                    group_id: *group_id,
                    group_permissions,
                    bandwidth_weight_override: *bandwidth_weight_override,
                    bandwidth_weight_inherit: *bandwidth_weight_inherit,
                    has_effective_changes: user_management.mode.has_effective_user_update_changes(
                        &conn.connection_info.username,
                        conn.is_admin,
                    ),
                },
                theme,
            ),
            UserManagementMode::ConfirmDelete { id: _, username } => confirm_delete_modal(
                username,
                user_management.delete_error.as_ref(),
                user_management.is_delete_submitting,
            ),
            // The outer `if mode != List` branch guards this function's
            // form-mode dispatch, so this arm isn't expected to fire.
            // Fall through to the tabbed list view (what the function
            // returns at its bottom for the List case) rather than
            // panicking — a future refactor should not be able to
            // crash the UI.
            UserManagementMode::List => tabbed_list_view(conn, user_management, theme),
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
        Some(Ok(groups)) => groups.len().to_string(),
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

    // Title row: clean centered title (Create / Refresh actions live
    // inside each tab's toolbar — see `users_tab_toolbar` and the
    // equivalent in `groups::group_list_content`).
    let title = panel_title(t("title-user-management"));

    // Panel-level action error (UserEdit or GroupEdit fetch failure).
    // Mutually exclusive between the two `list_error` fields, so at most
    // one is ever Some — see UserManagementState::set_user_list_error.
    let panel_error = user_management
        .list_error
        .as_ref()
        .or(user_management.group_management.list_error.as_ref());

    // Build the form with max_width constraint.
    //
    // Error spacing follows the in-form convention (see e.g.
    // `groups::create_view`): if an error is present, render it with a
    // small trailing spacer (the error contributes its own height to the
    // gap); otherwise emit the original `LARGE - MEDIUM` breathing
    // spacer so the steady-state title→tabs rhythm is unchanged.
    let mut form_items: Vec<Element<'_, Message>> = vec![Element::<'_, Message>::from(title)];
    if let Some(err) = panel_error {
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
        form_items.push(
            Space::new()
                .height(SPACER_SIZE_LARGE - SPACER_SIZE_MEDIUM)
                .into(),
        );
    }
    form_items.push(container(tabs).height(Fill).into());

    let form = Column::with_children(form_items)
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

#[cfg(test)]
mod tests {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    use super::*;

    fn sample_user() -> UserInfo {
        UserInfo {
            id: 1,
            username: "alice".to_string(),
            nickname: "alice".to_string(),
            login_time: 100,
            is_admin: false,
            is_shared: false,
            session_ids: vec![1],
            locale: "en".to_string(),
            avatar: None,
            is_away: false,
            status: None,
            group_id: Some(10),
            group_name: Some("users".to_string()),
            bandwidth_weight: 1,
        }
    }

    fn sample_deps() -> UserTableDeps {
        UserTableDeps {
            users: vec![sample_user()],
            sort_column: UserManagementSortColumn::Username,
            sort_ascending: true,
            admin_color: iced::Color::from_rgb(1.0, 0.0, 0.0),
            shared_color: iced::Color::from_rgb(0.5, 0.5, 0.5),
            can_edit: true,
            can_delete: true,
            is_admin: true,
            current_username: "quest".to_string(),
        }
    }

    fn deps_hash(deps: &UserTableDeps) -> u64 {
        let mut hasher = DefaultHasher::new();
        deps.hash(&mut hasher);
        hasher.finish()
    }

    fn assert_hash_changes(mutate: impl FnOnce(&mut UserTableDeps)) {
        let mut deps = sample_deps();
        let base = deps_hash(&deps);
        mutate(&mut deps);
        assert_ne!(deps_hash(&deps), base);
    }

    #[test]
    fn user_table_deps_hash_changes_on_rendered_and_action_fields() {
        assert_hash_changes(|d| d.users[0].id = 2);
        assert_hash_changes(|d| d.users[0].username = "bob".to_string());
        assert_hash_changes(|d| d.users[0].is_admin = true);
        assert_hash_changes(|d| d.users[0].is_shared = true);
        assert_hash_changes(|d| d.users[0].group_name = Some("admins".to_string()));
        assert_hash_changes(|d| d.sort_column = UserManagementSortColumn::Group);
        assert_hash_changes(|d| d.sort_ascending = false);
        assert_hash_changes(|d| d.can_edit = false);
        assert_hash_changes(|d| d.can_delete = false);
        assert_hash_changes(|d| d.is_admin = false);
        assert_hash_changes(|d| d.current_username = "alice".to_string());
        assert_hash_changes(|d| d.admin_color = iced::Color::from_rgb(0.0, 1.0, 0.0));
        assert_hash_changes(|d| d.shared_color = iced::Color::from_rgb(0.0, 0.0, 1.0));
    }
}
