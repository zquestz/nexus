//! Group management views (list, create, edit, delete groups)
//!
//! Part of the User Management panel's Groups tab.
//! Shows group list, create/edit forms, and delete confirmation.

use iced::widget::button as btn;
use iced::widget::{
    Column, Row, Space, button, checkbox, column, container, row, scrollable, text, text_input,
    tooltip,
};
use iced::{Center, Element, Fill, Theme, alignment};
use nexus_common::is_shared_account_permission;

use super::constants::{PERMISSION_GROUP_CREATE, PERMISSION_GROUP_DELETE, PERMISSION_GROUP_EDIT};
use super::layout::scrollable_panel;
use crate::i18n::{t, translate_permission};
use crate::icon;
use crate::style::{
    BUTTON_PADDING, CONTENT_MAX_WIDTH, CONTENT_PADDING, ELEMENT_SPACING, ICON_BUTTON_PADDING,
    INPUT_PADDING, NO_SPACING, SCROLLBAR_PADDING, SERVER_LIST_BUTTON_HEIGHT,
    SERVER_LIST_DISCONNECT_ICON_SIZE, SERVER_LIST_ITEM_SPACING, SERVER_LIST_TEXT_SIZE,
    SIDEBAR_ACTION_ICON_SIZE, SPACER_SIZE_MEDIUM, SPACER_SIZE_SMALL, TEXT_SIZE,
    TOOLTIP_BACKGROUND_PADDING, TOOLTIP_GAP, TOOLTIP_PADDING, TOOLTIP_TEXT_SIZE,
    alternating_row_style, danger_icon_button_style, error_text_style, muted_text_style,
    panel_title, shaped_text, shaped_text_wrapped, tooltip_container_style,
    transparent_icon_button_style,
};
use crate::types::{GroupManagementMode, Message, ServerConnection, UserManagementState};

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
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Helper function to create transparent edit icon buttons
fn transparent_edit_button(
    icon: iced::widget::Text<'_>,
    message: Message,
) -> button::Button<'_, Message> {
    button(icon.size(SERVER_LIST_DISCONNECT_ICON_SIZE))
        .on_press(message)
        .width(SERVER_LIST_BUTTON_HEIGHT)
        .height(SERVER_LIST_BUTTON_HEIGHT)
        .style(transparent_icon_button_style)
}

/// Helper function to create danger icon buttons (for delete)
fn danger_delete_button(
    icon: iced::widget::Text<'_>,
    message: Message,
) -> button::Button<'_, Message> {
    button(icon.size(SERVER_LIST_DISCONNECT_ICON_SIZE))
        .on_press(message)
        .width(SERVER_LIST_BUTTON_HEIGHT)
        .height(SERVER_LIST_BUTTON_HEIGHT)
        .style(danger_icon_button_style)
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

fn t_args(key: &str, args: &[(&str, &str)]) -> String {
    crate::i18n::t_args(key, args)
}

// ============================================================================
// List Content (for Groups tab)
// ============================================================================

/// Build the group list content for inside the Groups tab
///
/// Returns a self-contained element with create button and scrollable group list.
pub fn group_list_content<'a>(
    conn: &'a ServerConnection,
    user_management: &'a UserManagementState,
    _theme: &Theme,
) -> Element<'a, Message> {
    let can_create = conn.has_permission(PERMISSION_GROUP_CREATE);
    let can_edit = conn.has_permission(PERMISSION_GROUP_EDIT);
    let can_delete = conn.has_permission(PERMISSION_GROUP_DELETE);

    // Build scrollable content (group list or status message)
    let scroll_content_inner: Element<'a, Message> = {
        if let Some(error) = &user_management.group_management.list_error {
            // Error state
            shaped_text_wrapped(error)
                .size(TEXT_SIZE)
                .width(Fill)
                .align_x(Center)
                .style(error_text_style)
                .into()
        } else if let Some(groups) = &user_management.available_groups {
            if groups.is_empty() {
                // Empty state
                shaped_text(t("group-management-no-groups"))
                    .size(TEXT_SIZE)
                    .width(Fill)
                    .align_x(Center)
                    .style(muted_text_style)
                    .into()
            } else {
                // Group rows
                let mut group_rows = Column::new().spacing(SERVER_LIST_ITEM_SPACING);

                for (index, group) in groups.iter().enumerate() {
                    // Group name text
                    let name_text = shaped_text(&group.name).size(SERVER_LIST_TEXT_SIZE);

                    let name_container = container(name_text)
                        .width(Fill)
                        .height(SERVER_LIST_BUTTON_HEIGHT)
                        .padding(INPUT_PADDING)
                        .align_y(alignment::Vertical::Center);

                    let mut group_row = Row::new()
                        .spacing(NO_SPACING)
                        .align_y(alignment::Vertical::Center)
                        .push(name_container);

                    // Shared badge (if applicable)
                    if group.is_shared {
                        let shared_text = shaped_text(t("group-management-shared"))
                            .size(SERVER_LIST_TEXT_SIZE)
                            .style(muted_text_style);
                        let shared_container = container(shared_text)
                            .height(SERVER_LIST_BUTTON_HEIGHT)
                            .padding(INPUT_PADDING)
                            .align_y(alignment::Vertical::Center);
                        group_row = group_row.push(shared_container);
                    }

                    // Member count
                    let count_text = shaped_text(group.member_count.to_string())
                        .size(SERVER_LIST_TEXT_SIZE)
                        .style(muted_text_style);
                    let count_container = container(count_text)
                        .height(SERVER_LIST_BUTTON_HEIGHT)
                        .padding(INPUT_PADDING)
                        .align_y(alignment::Vertical::Center);
                    group_row = group_row.push(count_container);

                    // Edit button (gated by group_edit permission)
                    if can_edit {
                        let edit_btn = tooltip(
                            transparent_edit_button(
                                icon::edit(),
                                Message::GroupManagementEditClicked(group.id, group.name.clone()),
                            ),
                            container(shaped_text(t("tooltip-edit")).size(TOOLTIP_TEXT_SIZE))
                                .padding(TOOLTIP_BACKGROUND_PADDING)
                                .style(tooltip_container_style),
                            tooltip::Position::Top,
                        )
                        .gap(TOOLTIP_GAP)
                        .padding(TOOLTIP_PADDING);
                        group_row = group_row.push(edit_btn);
                    }

                    // Delete button (gated by group_delete permission)
                    if can_delete {
                        let delete_btn = tooltip(
                            danger_delete_button(
                                icon::trash(),
                                Message::GroupManagementDeleteClicked(group.id, group.name.clone()),
                            ),
                            container(shaped_text(t("tooltip-delete")).size(TOOLTIP_TEXT_SIZE))
                                .padding(TOOLTIP_BACKGROUND_PADDING)
                                .style(tooltip_container_style),
                            tooltip::Position::Top,
                        )
                        .gap(TOOLTIP_GAP)
                        .padding(TOOLTIP_PADDING);
                        group_row = group_row.push(delete_btn);
                    }

                    // Alternating row backgrounds
                    let is_even = index % 2 == 0;
                    let row_container = container(group_row)
                        .width(Fill)
                        .style(alternating_row_style(is_even));

                    group_rows = group_rows.push(row_container);
                }

                group_rows.width(Fill).into()
            }
        } else {
            // Loading state
            shaped_text(t("group-management-loading"))
                .size(TEXT_SIZE)
                .width(Fill)
                .align_x(Center)
                .style(muted_text_style)
                .into()
        }
    };

    // Create group button (optional, right-aligned)
    let button_row: Element<'a, Message> = if can_create {
        let add_icon = container(icon::user_plus().size(SIDEBAR_ACTION_ICON_SIZE))
            .width(SIDEBAR_ACTION_ICON_SIZE)
            .height(SIDEBAR_ACTION_ICON_SIZE)
            .align_x(alignment::Horizontal::Center)
            .align_y(alignment::Vertical::Center);

        let create_btn = tooltip(
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
        .padding(TOOLTIP_PADDING);

        row![Space::new().width(Fill), create_btn]
            .align_y(Center)
            .into()
    } else {
        Space::new().height(SPACER_SIZE_SMALL).into()
    };

    // Scrollable content with symmetric padding for scrollbar space
    let scroll_inner = container(scroll_content_inner).width(Fill);
    let padded_scroll_content = row![
        Space::new().width(SCROLLBAR_PADDING),
        scroll_inner,
        Space::new().width(SCROLLBAR_PADDING),
    ];

    // Build the complete tab content
    column![
        container(button_row).padding(iced::Padding {
            top: 0.0,
            right: SCROLLBAR_PADDING,
            bottom: 0.0,
            left: SCROLLBAR_PADDING,
        }),
        scrollable(padded_scroll_content).height(Fill),
    ]
    .spacing(ELEMENT_SPACING)
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

    let can_create = !gm.name.trim().is_empty();

    let name_input = text_input(&t("group-management-name"), &gm.name)
        .on_input(Message::GroupManagementNameChanged)
        .on_submit(Message::GroupManagementCreatePressed)
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE);

    // Shared checkbox — any user with group_create can set this
    let shared_checkbox = checkbox(gm.is_shared)
        .label(t("group-management-shared"))
        .on_toggle(Message::GroupManagementIsSharedToggled)
        .size(TEXT_SIZE)
        .text_shaping(text::Shaping::Advanced);

    let permissions_title = shaped_text(t("label-permissions")).size(TEXT_SIZE);
    let permissions_row = build_group_permission_columns(
        &gm.permissions,
        conn,
        gm.is_shared,
        Message::GroupManagementPermissionToggled,
    );

    let create_button = if can_create {
        button(shaped_text(t("button-create")).size(TEXT_SIZE))
            .on_press(Message::GroupManagementCreatePressed)
            .padding(BUTTON_PADDING)
    } else {
        button(shaped_text(t("button-create")).size(TEXT_SIZE)).padding(BUTTON_PADDING)
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

    let can_update = !ctx.new_name.trim().is_empty();

    let name_input = text_input(&t("group-management-name"), ctx.new_name)
        .on_input(Message::GroupManagementEditNameChanged)
        .on_submit(Message::GroupManagementUpdatePressed)
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

    let permissions_title = shaped_text(t("label-permissions")).size(TEXT_SIZE);
    let permissions_row = build_group_permission_columns(
        ctx.permissions,
        ctx.conn,
        ctx.is_shared,
        Message::GroupManagementEditPermissionToggled,
    );

    let update_button = if can_update {
        button(shaped_text(t("button-update")).size(TEXT_SIZE))
            .on_press(Message::GroupManagementUpdatePressed)
            .padding(BUTTON_PADDING)
    } else {
        button(shaped_text(t("button-update")).size(TEXT_SIZE)).padding(BUTTON_PADDING)
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
fn confirm_delete_modal<'a>(name: &'a str, error: Option<&'a String>) -> Element<'a, Message> {
    let title = panel_title(t("title-confirm-delete"));

    let message = shaped_text_wrapped(t_args("confirm-delete-group", &[("name", name)]))
        .size(TEXT_SIZE)
        .width(Fill)
        .align_x(Center);

    let confirm_button = button(shaped_text(t("button-delete")).size(TEXT_SIZE))
        .on_press(Message::GroupManagementConfirmDelete)
        .padding(BUTTON_PADDING)
        .style(btn::danger);

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
        } => edit_view(EditGroupContext {
            conn,
            edit_error: user_management.group_management.edit_error.as_ref(),
            original_name,
            new_name,
            is_shared: *is_shared,
            member_count: *member_count,
            permissions,
        }),
        GroupManagementMode::ConfirmDelete { id: _, name } => {
            confirm_delete_modal(name, user_management.group_management.delete_error.as_ref())
        }
        GroupManagementMode::List => {
            // Should not be called in List mode — return empty space as safety net
            Space::new().into()
        }
    }
}
