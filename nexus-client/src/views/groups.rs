//! Group management views (list, create, edit, delete groups)
//!
//! Part of the User Management panel's Groups tab.
//! Shows group list, create/edit forms, and delete confirmation.

use std::hash::{Hash, Hasher};

use iced::widget::button as btn;
use iced::widget::text::Wrapping;
use iced::widget::{
    Column, Id, Space, button, checkbox, column, container, lazy, row, scrollable, table, text,
    text_input,
};
use iced::{Center, Element, Fill, Length};
use iced_aw::NumberInput;
use nexus_common::is_shared_account_permission;
use nexus_common::names::fold_name;
use nexus_common::protocol::GroupInfo;
use nexus_common::validators::MIN_BANDWIDTH_WEIGHT;

use super::constants::{PERMISSION_GROUP_CREATE, PERMISSION_GROUP_DELETE, PERMISSION_GROUP_EDIT};
use super::helpers::{sort_icon_or_placeholder, t_args, tab_toolbar_icon_button};
use super::layout::scrollable_panel;
use crate::i18n::{t, translate_permission};
use crate::icon;
use crate::style::{
    BUTTON_PADDING, CONTENT_MAX_WIDTH, CONTENT_PADDING, CONTEXT_MENU_ITEM_PADDING,
    CONTEXT_MENU_MIN_WIDTH, CONTEXT_MENU_PADDING, CONTEXT_MENU_SEPARATOR_HEIGHT,
    CONTEXT_MENU_SEPARATOR_MARGIN, ELEMENT_SPACING, INPUT_PADDING, NO_SPACING, SCROLLBAR_PADDING,
    SEPARATOR_HEIGHT, SORT_ICON_LEFT_MARGIN, SORT_ICON_RIGHT_MARGIN, SPACER_SIZE_MEDIUM,
    SPACER_SIZE_SMALL, TEXT_SIZE, context_menu_container_style, error_text_style,
    menu_button_danger_style, menu_button_style, muted_text_style, panel_title, separator_style,
    shaped_text, shaped_text_wrapped, transparent_icon_button_style,
};
use crate::types::{
    GroupManagementMode, GroupManagementSortColumn, InputId, Message, ServerConnection,
    UserManagementState,
};
use crate::widgets::{LazyContextMenu, MenuButton};

// ============================================================================
// Edit Group Context
// ============================================================================

/// Context for rendering the edit group form
struct EditGroupContext<'a> {
    /// Connection state (for permission checking)
    conn: &'a ServerConnection,
    /// Edit error message (if any)
    edit_error: Option<&'a String>,
    /// Original group name (for display)
    original_name: &'a str,
    /// New group name (editable field)
    new_name: &'a str,
    /// Whether this group is for shared accounts
    is_shared: bool,
    /// Number of members (determines if is_shared can be toggled)
    member_count: u32,
    /// Permissions list with enabled state
    permissions: &'a [(String, bool)],
    /// Bandwidth weight for the scheduler (1..=65535)
    bandwidth_weight: u16,
    /// Whether a submit request is in flight
    is_submitting: bool,
}

// ============================================================================
// Table Dependencies & Sorting
// ============================================================================

/// Dependencies for lazy group table rendering
#[derive(Clone)]
struct GroupTableDeps {
    groups: Vec<GroupInfo>,
    sort_column: GroupManagementSortColumn,
    sort_ascending: bool,
    can_edit: bool,
    can_delete: bool,
}

impl Hash for GroupTableDeps {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.groups.len().hash(state);
        for group in &self.groups {
            group.id.hash(state);
            group.name.hash(state);
            group.is_shared.hash(state);
            group.member_count.hash(state);
        }
        self.sort_column.hash(state);
        self.sort_ascending.hash(state);
        self.can_edit.hash(state);
        self.can_delete.hash(state);
        // group.permissions not hashed — not displayed in table (only name, shared, member_count)
    }
}

fn sort_groups(groups: &mut [GroupInfo], column: GroupManagementSortColumn, ascending: bool) {
    groups.sort_by(|a, b| {
        let cmp = match column {
            GroupManagementSortColumn::Name => fold_name(&a.name).cmp(&fold_name(&b.name)),
            GroupManagementSortColumn::Members => a.member_count.cmp(&b.member_count),
        };
        if ascending { cmp } else { cmp.reverse() }
    });
}

/// Build permission checkboxes split into two columns (for group forms)
///
/// When `is_shared` is true, permissions not in `SHARED_ACCOUNT_PERMISSIONS` are disabled.
/// Non-admin delegation: user can only toggle permissions they have.
fn build_group_permission_columns<'a, F>(
    permissions: &'a [(String, bool)],
    conn: &'a ServerConnection,
    is_shared: bool,
    on_toggle: F,
) -> Element<'a, Message>
where
    F: Fn(String, bool) -> Message + 'a + Clone,
{
    let mut left_column = Column::new().spacing(SPACER_SIZE_SMALL);
    let mut right_column = Column::new().spacing(SPACER_SIZE_SMALL);

    for (index, (permission, enabled)) in permissions.iter().enumerate() {
        let perm_name = permission.clone();
        let display_name = translate_permission(permission);
        let on_toggle_clone = on_toggle.clone();

        // Check if this permission is allowed for the current user (non-admin delegation)
        let user_can_toggle = conn.has_permission(permission);

        // Check if this permission is forbidden for shared groups
        let forbidden_for_shared = is_shared && !is_shared_account_permission(permission);

        let checkbox_widget = if user_can_toggle && !forbidden_for_shared {
            // Can toggle: user has permission and it's not forbidden for shared groups
            checkbox(*enabled)
                .label(display_name)
                .on_toggle(move |checked| on_toggle_clone(perm_name.clone(), checked))
                .size(TEXT_SIZE)
                .text_shaping(text::Shaping::Advanced)
        } else {
            // Cannot toggle: either user doesn't have permission or it's forbidden for shared
            checkbox(*enabled)
                .label(display_name)
                .size(TEXT_SIZE)
                .text_shaping(text::Shaping::Advanced)
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
// Lazy Group Table
// ============================================================================

/// Build the lazy group table with sortable columns
fn build_group_context_menu(
    group_id: i64,
    group_name: String,
    can_edit: bool,
    can_delete: bool,
) -> Element<'static, Message> {
    let mut menu_items: Vec<Element<'_, Message>> = vec![];

    if can_edit {
        menu_items.push(
            MenuButton::new(shaped_text(t("button-edit")).size(TEXT_SIZE))
                .padding(CONTEXT_MENU_ITEM_PADDING)
                .width(Fill)
                .style(menu_button_style)
                .on_press(Message::GroupManagementEditClicked(
                    group_id,
                    group_name.clone(),
                ))
                .into(),
        );
    }

    if can_edit && can_delete {
        menu_items.push(
            container(Space::new())
                .width(Fill)
                .height(CONTEXT_MENU_SEPARATOR_HEIGHT)
                .style(separator_style)
                .into(),
        );
    }

    if can_delete {
        menu_items.push(
            MenuButton::new(shaped_text(t("button-delete")).size(TEXT_SIZE))
                .padding(CONTEXT_MENU_ITEM_PADDING)
                .width(Fill)
                .style(menu_button_danger_style)
                .on_press(Message::GroupManagementDeleteClicked(
                    group_id,
                    group_name.clone(),
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

fn lazy_group_table(deps: GroupTableDeps) -> Element<'static, Message> {
    let can_edit = deps.can_edit;
    let can_delete = deps.can_delete;

    lazy(deps, move |deps| {
        // Name column header (sortable, stable layout)
        let name_sort_icon = sort_icon_or_placeholder(
            deps.sort_column == GroupManagementSortColumn::Name,
            deps.sort_ascending,
        );
        let name_header_content: Element<'static, Message> = row![
            shaped_text(t("col-name"))
                .size(TEXT_SIZE)
                .wrapping(Wrapping::Word)
                .style(muted_text_style),
            Space::new().width(Fill),
            Space::new().width(SORT_ICON_LEFT_MARGIN),
            name_sort_icon,
            Space::new().width(SORT_ICON_RIGHT_MARGIN),
        ]
        .align_y(Center)
        .into();
        let name_header: Element<'static, Message> = button(name_header_content)
            .padding(NO_SPACING)
            .width(Length::Shrink)
            .style(transparent_icon_button_style)
            .on_press(Message::GroupManagementSortBy(
                GroupManagementSortColumn::Name,
            ))
            .into();

        // Name column cell (left-click to edit, right-click context menu)
        let name_column = table::column(name_header, move |group: GroupInfo| {
            let group_id = group.id;
            let group_name_for_click = group.name.clone();
            let group_name_for_menu = group.name.clone();

            let text_widget = shaped_text(group.name)
                .size(TEXT_SIZE)
                .wrapping(Wrapping::WordOrGlyph);

            let content: Element<'static, Message> = if can_edit {
                button(text_widget)
                    .padding(NO_SPACING)
                    .style(transparent_icon_button_style)
                    .on_press(Message::GroupManagementEditClicked(
                        group_id,
                        group_name_for_click,
                    ))
                    .into()
            } else {
                text_widget.into()
            };

            if can_edit || can_delete {
                LazyContextMenu::new(content, move || {
                    build_group_context_menu(
                        group_id,
                        group_name_for_menu.clone(),
                        can_edit,
                        can_delete,
                    )
                })
                .into()
            } else {
                content
            }
        })
        .width(Length::FillPortion(1));

        // Members column header (sortable, stable layout)
        let members_sort_icon = sort_icon_or_placeholder(
            deps.sort_column == GroupManagementSortColumn::Members,
            deps.sort_ascending,
        );
        let members_header_content: Element<'static, Message> = row![
            shaped_text(t("col-members"))
                .size(TEXT_SIZE)
                .wrapping(Wrapping::Word)
                .style(muted_text_style),
            Space::new().width(Fill),
            Space::new().width(SORT_ICON_LEFT_MARGIN),
            members_sort_icon,
            Space::new().width(SORT_ICON_RIGHT_MARGIN),
            // Trailing gap so the rightmost column doesn't abut the scrollbar.
            Space::new().width(SCROLLBAR_PADDING),
        ]
        .align_y(Center)
        .into();
        let members_header: Element<'static, Message> = button(members_header_content)
            .padding(NO_SPACING)
            .width(Length::Shrink)
            .style(transparent_icon_button_style)
            .on_press(Message::GroupManagementSortBy(
                GroupManagementSortColumn::Members,
            ))
            .into();

        // Members column cell. Trailing Space matches the header so the
        // rightmost column doesn't abut the scrollbar.
        let members_column = table::column(members_header, |group: GroupInfo| {
            row![
                shaped_text(group.member_count.to_string())
                    .size(TEXT_SIZE)
                    .wrapping(Wrapping::Word)
                    .style(muted_text_style),
                Space::new().width(SCROLLBAR_PADDING),
            ]
            .align_y(Center)
        })
        .width(Length::Shrink);

        // Build the table
        let columns = [name_column, members_column];

        table(columns, deps.groups.clone())
            .width(Fill)
            .padding_x(SPACER_SIZE_SMALL)
            .padding_y(SPACER_SIZE_SMALL)
            .separator_x(NO_SPACING)
            .separator_y(SEPARATOR_HEIGHT)
    })
    .into()
}

// ============================================================================
// List Content (for Groups tab)
// ============================================================================

/// Build the Groups tab toolbar (Create Group + Refresh icon buttons).
///
/// Create Group is permission-gated on `group_create`. Refresh is
/// disabled while a list fetch is in flight (`available_groups.is_none()`).
/// After a failed fetch, `available_groups` is `Some(Err(_))` so refresh
/// re-enables for retry — same shape as the users-list flow.
fn groups_tab_toolbar<'a>(
    conn: &'a ServerConnection,
    user_management: &'a UserManagementState,
) -> Element<'a, Message> {
    let create_btn = tab_toolbar_icon_button(
        icon::plus_circled(),
        "tooltip-group-create",
        Message::GroupManagementShowCreate,
        conn.has_permission(PERMISSION_GROUP_CREATE),
    );
    let refresh_btn = tab_toolbar_icon_button(
        icon::refresh(),
        "tooltip-refresh",
        Message::UserManagementRefreshGroups,
        user_management.available_groups.is_some(),
    );

    row![create_btn, refresh_btn]
        .spacing(SPACER_SIZE_SMALL)
        .into()
}

/// Build the group list content for inside the Groups tab
///
/// Returns a self-contained element with a sortable table of groups.
pub fn group_list_content<'a>(
    conn: &'a ServerConnection,
    user_management: &'a UserManagementState,
) -> Element<'a, Message> {
    let can_edit = conn.has_permission(PERMISSION_GROUP_EDIT);
    let can_delete = conn.has_permission(PERMISSION_GROUP_DELETE);

    let toolbar = groups_tab_toolbar(conn, user_management);

    let body: Element<'a, Message> = match &user_management.available_groups {
        None => {
            // Loading state
            container(
                shaped_text(t("group-management-loading"))
                    .size(TEXT_SIZE)
                    .style(muted_text_style),
            )
            .width(Fill)
            .center_x(Fill)
            .padding(SPACER_SIZE_SMALL)
            .into()
        }
        Some(Err(error)) => {
            // Error state — list fetch failed
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
        Some(Ok(groups)) => {
            if groups.is_empty() {
                // Empty state
                container(
                    shaped_text(t("group-management-no-groups"))
                        .size(TEXT_SIZE)
                        .style(muted_text_style),
                )
                .width(Fill)
                .center_x(Fill)
                .padding(SPACER_SIZE_SMALL)
                .into()
            } else {
                // Sort groups based on current settings
                let gm = &user_management.group_management;
                let mut sorted_groups = groups.clone();
                sort_groups(&mut sorted_groups, gm.sort_column, gm.sort_ascending);

                let deps = GroupTableDeps {
                    groups: sorted_groups,
                    sort_column: gm.sort_column,
                    sort_ascending: gm.sort_ascending,
                    can_edit,
                    can_delete,
                };

                scrollable(lazy_group_table(deps)).height(Fill).into()
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

/// Build the create group form (full-panel view)
fn create_view<'a>(
    conn: &'a ServerConnection,
    user_management: &'a UserManagementState,
) -> Element<'a, Message> {
    let gm = &user_management.group_management;

    let title = panel_title(t("title-group-create"));

    let can_create = !gm.name.trim().is_empty() && !gm.is_submitting;

    // Helper for on_submit
    let submit_action = if can_create {
        Message::GroupManagementCreatePressed
    } else {
        Message::ValidateGroupManagementCreate
    };

    let name_input = text_input(&t("group-form-name"), &gm.name)
        .on_input(Message::GroupManagementNameChanged)
        .on_submit(submit_action)
        .id(Id::from(InputId::CreateGroupName))
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE);

    // Shared checkbox — any user with group_create can set this
    let shared_checkbox = checkbox(gm.is_shared)
        .label(t("group-management-shared"))
        .on_toggle(Message::GroupManagementIsSharedToggled)
        .size(TEXT_SIZE)
        .text_shaping(text::Shaping::Advanced);

    // Bandwidth weight (1..=u16::MAX). Sits between Shared and Permissions.
    // The server enforces a delegation rule: non-admins can set it only
    // at or below their own resolved weight; admins bypass.
    let bandwidth_label = shaped_text(t("label-bandwidth-weight")).size(TEXT_SIZE);
    let bandwidth_input: Element<'a, Message> = NumberInput::new(
        &gm.bandwidth_weight,
        MIN_BANDWIDTH_WEIGHT..=u16::MAX,
        Message::GroupManagementBandwidthWeightChanged,
    )
    .id(Id::from(InputId::CreateGroupBandwidthWeight))
    .padding(INPUT_PADDING)
    .into();
    let bandwidth_row: Element<'a, Message> = row![
        bandwidth_label,
        Space::new().width(ELEMENT_SPACING),
        bandwidth_input,
    ]
    .align_y(Center)
    .into();

    let permissions_title = shaped_text(t("label-permissions")).size(TEXT_SIZE);
    let permissions_row = build_group_permission_columns(
        &gm.permissions,
        conn,
        gm.is_shared,
        Message::GroupManagementPermissionToggled,
    );

    let create_button = if can_create {
        button(shaped_text(t("button-save")).size(TEXT_SIZE))
            .on_press(Message::GroupManagementCreatePressed)
            .padding(BUTTON_PADDING)
    } else {
        button(shaped_text(t("button-save")).size(TEXT_SIZE)).padding(BUTTON_PADDING)
    };

    let cancel_button = button(shaped_text(t("button-cancel")).size(TEXT_SIZE))
        .on_press(Message::CancelGroupManagement)
        .padding(BUTTON_PADDING)
        .style(btn::secondary);

    let mut items: Vec<Element<'a, Message>> = vec![title.into()];

    // Show error if present
    if let Some(error) = &gm.create_error {
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
        name_input.into(),
        shared_checkbox.into(),
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

/// Build the edit group form (full-panel view)
fn edit_view(ctx: EditGroupContext<'_>) -> Element<'_, Message> {
    let title = panel_title(t("title-group-edit"));

    let subtitle = shaped_text_wrapped(ctx.original_name)
        .size(TEXT_SIZE)
        .width(Fill)
        .align_x(Center)
        .style(muted_text_style);

    let can_update = !ctx.new_name.trim().is_empty() && !ctx.is_submitting;

    // Helper for on_submit
    let submit_action = if can_update {
        Message::GroupManagementUpdatePressed
    } else {
        Message::ValidateGroupManagementEdit
    };

    let name_input = text_input(&t("group-form-name"), ctx.new_name)
        .on_input(Message::GroupManagementEditNameChanged)
        .on_submit(submit_action)
        .id(Id::from(InputId::EditGroupName))
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE);

    // Shared checkbox — only toggleable when group has no members
    let shared_checkbox = if ctx.member_count == 0 {
        checkbox(ctx.is_shared)
            .label(t("group-management-shared"))
            .on_toggle(Message::GroupManagementEditIsSharedToggled)
            .size(TEXT_SIZE)
            .text_shaping(text::Shaping::Advanced)
    } else {
        // Disabled: group has members, cannot toggle shared status
        checkbox(ctx.is_shared)
            .label(t("group-management-shared"))
            .size(TEXT_SIZE)
            .text_shaping(text::Shaping::Advanced)
    };

    // Bandwidth weight (1..=u16::MAX) — same shape as the create form.
    let bandwidth_label = shaped_text(t("label-bandwidth-weight")).size(TEXT_SIZE);
    let bandwidth_input: Element<'_, Message> = NumberInput::new(
        &ctx.bandwidth_weight,
        MIN_BANDWIDTH_WEIGHT..=u16::MAX,
        Message::GroupManagementEditBandwidthWeightChanged,
    )
    .id(Id::from(InputId::EditGroupBandwidthWeight))
    .padding(INPUT_PADDING)
    .into();
    let bandwidth_row: Element<'_, Message> = row![
        bandwidth_label,
        Space::new().width(ELEMENT_SPACING),
        bandwidth_input,
    ]
    .align_y(Center)
    .into();

    let permissions_title = shaped_text(t("label-permissions")).size(TEXT_SIZE);
    let permissions_row = build_group_permission_columns(
        ctx.permissions,
        ctx.conn,
        ctx.is_shared,
        Message::GroupManagementEditPermissionToggled,
    );

    let update_button = if can_update {
        button(shaped_text(t("button-save")).size(TEXT_SIZE))
            .on_press(Message::GroupManagementUpdatePressed)
            .padding(BUTTON_PADDING)
    } else {
        button(shaped_text(t("button-save")).size(TEXT_SIZE)).padding(BUTTON_PADDING)
    };

    let cancel_button = button(shaped_text(t("button-cancel")).size(TEXT_SIZE))
        .on_press(Message::CancelGroupManagement)
        .padding(BUTTON_PADDING)
        .style(btn::secondary);

    let mut items: Vec<Element<'_, Message>> = vec![title.into(), subtitle.into()];

    // Show error if present
    if let Some(error) = ctx.edit_error {
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
        name_input.into(),
        shared_checkbox.into(),
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

/// Build the delete group confirmation modal (full-panel view)
fn confirm_delete_modal<'a>(
    name: &'a str,
    error: Option<&'a String>,
    is_delete_submitting: bool,
) -> Element<'a, Message> {
    let title = panel_title(t("title-confirm-delete"));

    let message = shaped_text_wrapped(t_args("confirm-delete-group", &[("name", name)]))
        .size(TEXT_SIZE)
        .width(Fill)
        .align_x(Center);

    let confirm_button = if !is_delete_submitting {
        button(shaped_text(t("button-delete")).size(TEXT_SIZE))
            .on_press(Message::GroupManagementConfirmDelete)
            .padding(BUTTON_PADDING)
            .style(btn::danger)
    } else {
        button(shaped_text(t("button-delete")).size(TEXT_SIZE))
            .padding(BUTTON_PADDING)
            .style(btn::danger)
    };

    let cancel_button = button(shaped_text(t("button-cancel")).size(TEXT_SIZE))
        .on_press(Message::GroupManagementCancelDelete)
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
// Main Form Dispatch
// ============================================================================

/// Renders the active group form (Create, Edit, or ConfirmDelete).
///
/// This is called when `GroupManagementMode` is NOT `List`, and the form
/// takes over the full panel (hiding the tabs).
///
/// Must NOT be called when mode is `List` — the caller should use
/// `group_list_content` for the Groups tab list view instead.
pub fn group_form_view<'a>(
    conn: &'a ServerConnection,
    user_management: &'a UserManagementState,
) -> Element<'a, Message> {
    match &user_management.group_management.mode {
        GroupManagementMode::Create => create_view(conn, user_management),
        GroupManagementMode::Edit {
            id: _,
            original_name,
            new_name,
            is_shared,
            member_count,
            permissions,
            bandwidth_weight,
            original_bandwidth_weight: _,
        } => edit_view(EditGroupContext {
            conn,
            edit_error: user_management.group_management.edit_error.as_ref(),
            original_name,
            new_name,
            is_shared: *is_shared,
            member_count: *member_count,
            permissions,
            bandwidth_weight: *bandwidth_weight,
            is_submitting: user_management.group_management.is_submitting,
        }),
        GroupManagementMode::ConfirmDelete { id: _, name } => confirm_delete_modal(
            name,
            user_management.group_management.delete_error.as_ref(),
            user_management.group_management.is_delete_submitting,
        ),
        GroupManagementMode::List => {
            // Should not be called in List mode — return empty space as safety net
            Space::new().into()
        }
    }
}
