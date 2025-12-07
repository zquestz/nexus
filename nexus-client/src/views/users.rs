//! User management panel (add, edit, delete users)

use super::constants::PERMISSION_USER_DELETE;
use crate::i18n::{t, translate_permission};
use crate::style::{
    BUTTON_PADDING, ELEMENT_SPACING, FORM_MAX_WIDTH, FORM_PADDING, INPUT_PADDING,
    SPACER_SIZE_MEDIUM, SPACER_SIZE_SMALL, TEXT_SIZE, TITLE_SIZE, content_background_style,
    error_text_style, shaped_text, shaped_text_wrapped,
};
use crate::types::{
    ActivePanel, InputId, Message, ServerConnection, UserEditState, UserManagementState,
};
use iced::widget::button as btn;
use iced::widget::{Column, Id, Space, button, checkbox, container, row, text, text_input};
use iced::{Center, Element, Fill};

// ============================================================================
// Helper Functions
// ============================================================================

/// Helper function to create an empty fallback panel
fn empty_panel<'a>() -> Element<'a, Message> {
    container(Space::new().width(Fill).height(Fill))
        .width(Fill)
        .height(Fill)
        .style(content_background_style)
        .into()
}

/// Wrap a form in a centered container with background styling
fn wrap_form<'a>(form: Column<'a, Message>) -> Element<'a, Message> {
    container(form)
        .width(Fill)
        .height(Fill)
        .center(Fill)
        .style(content_background_style)
        .into()
}

/// Build permission checkboxes split into two columns
fn build_permission_columns<'a, F>(
    permissions: &'a [(String, bool)],
    conn: &'a ServerConnection,
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

        let checkbox_widget = if conn.is_admin || conn.permissions.contains(permission) {
            // Can toggle permissions they have
            checkbox(*enabled)
                .label(display_name)
                .on_toggle(move |checked| on_toggle_clone(perm_name.clone(), checked))
                .size(TEXT_SIZE)
                .text_shaping(text::Shaping::Advanced)
        } else {
            // Cannot toggle permissions they don't have
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
// Add User View
// ============================================================================

/// Build the Add User form
fn add_user_view<'a>(
    conn: &'a ServerConnection,
    user_management: &'a UserManagementState,
) -> Element<'a, Message> {
    let create_title = shaped_text(t("title-user-create"))
        .size(TITLE_SIZE)
        .width(Fill)
        .align_x(Center);

    let can_create =
        !user_management.username.trim().is_empty() && !user_management.password.trim().is_empty();

    // Helper for on_submit
    let submit_action = if can_create {
        Message::CreateUserPressed
    } else {
        Message::ValidateCreateUser
    };

    let username_input = text_input(&t("placeholder-username"), &user_management.username)
        .on_input(Message::AdminUsernameChanged)
        .on_submit(submit_action.clone())
        .id(Id::from(InputId::AdminUsername))
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE);

    let password_input = text_input(&t("placeholder-password"), &user_management.password)
        .on_input(Message::AdminPasswordChanged)
        .on_submit(submit_action)
        .id(Id::from(InputId::AdminPassword))
        .secure(true)
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE);

    let admin_checkbox = if conn.is_admin {
        checkbox(user_management.is_admin)
            .label(t("label-admin"))
            .on_toggle(Message::AdminIsAdminToggled)
            .size(TEXT_SIZE)
            .text_shaping(text::Shaping::Advanced)
    } else {
        checkbox(user_management.is_admin)
            .label(t("label-admin"))
            .size(TEXT_SIZE)
            .text_shaping(text::Shaping::Advanced)
    };

    let enabled_checkbox = if conn.is_admin {
        checkbox(user_management.enabled)
            .label(t("label-enabled"))
            .on_toggle(Message::AdminEnabledToggled)
            .size(TEXT_SIZE)
            .text_shaping(text::Shaping::Advanced)
    } else {
        checkbox(user_management.enabled)
            .label(t("label-enabled"))
            .size(TEXT_SIZE)
            .text_shaping(text::Shaping::Advanced)
    };

    let permissions_title = shaped_text(t("label-permissions")).size(TEXT_SIZE);
    let permissions_row = build_permission_columns(
        &user_management.permissions,
        conn,
        Message::AdminPermissionToggled,
    );

    let create_button = if can_create {
        button(shaped_text(t("button-create")).size(TEXT_SIZE))
            .on_press(Message::CreateUserPressed)
            .padding(BUTTON_PADDING)
    } else {
        button(shaped_text(t("button-create")).size(TEXT_SIZE)).padding(BUTTON_PADDING)
    };

    let cancel_button = button(shaped_text(t("button-cancel")).size(TEXT_SIZE))
        .on_press(Message::CancelAddUser)
        .padding(BUTTON_PADDING)
        .style(btn::secondary);

    let mut create_items: Vec<Element<'a, Message>> = vec![create_title.into()];

    // Show error if present
    if let Some(error) = &user_management.create_error {
        create_items.push(
            shaped_text_wrapped(error)
                .size(TEXT_SIZE)
                .width(Fill)
                .align_x(Center)
                .style(error_text_style)
                .into(),
        );
        create_items.push(Space::new().height(SPACER_SIZE_SMALL).into());
    } else {
        create_items.push(Space::new().height(SPACER_SIZE_MEDIUM).into());
    }

    create_items.extend([
        username_input.into(),
        password_input.into(),
        admin_checkbox.into(),
        enabled_checkbox.into(),
        Space::new().height(SPACER_SIZE_SMALL).into(),
        permissions_title.into(),
        permissions_row,
        Space::new().height(SPACER_SIZE_MEDIUM).into(),
        row![Space::new().width(Fill), cancel_button, create_button]
            .spacing(ELEMENT_SPACING)
            .into(),
    ]);

    let create_form = Column::with_children(create_items)
        .spacing(ELEMENT_SPACING)
        .padding(FORM_PADDING)
        .max_width(FORM_MAX_WIDTH);

    wrap_form(create_form)
}

// ============================================================================
// Edit User View - Stage 1 (Select User)
// ============================================================================

/// Build the Select User form (stage 1 of edit)
fn select_user_view<'a>(
    conn: &'a ServerConnection,
    user_management: &'a UserManagementState,
    username: &'a str,
) -> Element<'a, Message> {
    let edit_title = shaped_text(t("title-user-edit"))
        .size(TITLE_SIZE)
        .width(Fill)
        .align_x(Center);

    let can_edit = !username.trim().is_empty();
    let can_delete = !username.trim().is_empty()
        && (conn.is_admin || conn.permissions.iter().any(|p| p == PERMISSION_USER_DELETE));

    // Helper for on_submit
    let submit_action = if can_edit {
        Message::EditUserPressed
    } else {
        Message::ValidateEditUser
    };

    let username_input = text_input(&t("placeholder-username"), username)
        .on_input(Message::EditUsernameChanged)
        .on_submit(submit_action)
        .id(Id::from(InputId::EditUsername))
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE);

    let edit_button = if can_edit {
        button(shaped_text(t("button-edit")).size(TEXT_SIZE))
            .on_press(Message::EditUserPressed)
            .padding(BUTTON_PADDING)
    } else {
        button(shaped_text(t("button-edit")).size(TEXT_SIZE)).padding(BUTTON_PADDING)
    };

    let delete_button = if can_delete {
        button(shaped_text(t("button-delete")).size(TEXT_SIZE))
            .on_press(Message::DeleteUserPressed(username.to_string()))
            .padding(BUTTON_PADDING)
            .style(btn::danger)
    } else {
        button(shaped_text(t("button-delete")).size(TEXT_SIZE))
            .padding(BUTTON_PADDING)
            .style(btn::danger)
    };

    let cancel_button = button(shaped_text(t("button-cancel")).size(TEXT_SIZE))
        .on_press(Message::CancelEditUser)
        .padding(BUTTON_PADDING)
        .style(btn::secondary);

    let mut edit_items: Vec<Element<'a, Message>> = vec![edit_title.into()];

    // Show error if present
    if let Some(error) = &user_management.edit_error {
        edit_items.push(
            shaped_text_wrapped(error)
                .size(TEXT_SIZE)
                .width(Fill)
                .align_x(Center)
                .style(error_text_style)
                .into(),
        );
        edit_items.push(Space::new().height(SPACER_SIZE_SMALL).into());
    } else {
        edit_items.push(Space::new().height(SPACER_SIZE_MEDIUM).into());
    }

    edit_items.extend([
        username_input.into(),
        Space::new().height(SPACER_SIZE_MEDIUM).into(),
        row![
            Space::new().width(Fill),
            cancel_button,
            delete_button,
            edit_button
        ]
        .spacing(ELEMENT_SPACING)
        .into(),
    ]);

    let edit_form = Column::with_children(edit_items)
        .spacing(ELEMENT_SPACING)
        .padding(FORM_PADDING)
        .max_width(FORM_MAX_WIDTH);

    wrap_form(edit_form)
}

// ============================================================================
// Edit User View - Stage 2 (Update User)
// ============================================================================

/// Build the Update User form (stage 2 of edit)
fn update_user_view<'a>(
    conn: &'a ServerConnection,
    user_management: &'a UserManagementState,
    edit_state: &'a UserEditState,
) -> Element<'a, Message> {
    // Extract fields from EditingUser variant
    let (new_username, new_password, is_admin, enabled, permissions) = match edit_state {
        UserEditState::EditingUser {
            new_username,
            new_password,
            is_admin,
            enabled,
            permissions,
            ..
        } => (
            new_username.as_str(),
            new_password.as_str(),
            *is_admin,
            *enabled,
            permissions.as_slice(),
        ),
        _ => return empty_panel(), // Should never happen
    };

    let update_title = shaped_text(t("title-update-user"))
        .size(TITLE_SIZE)
        .width(Fill)
        .align_x(Center);

    let can_update = !new_username.trim().is_empty();

    // Helper for on_submit
    let submit_action = if can_update {
        Message::UpdateUserPressed
    } else {
        Message::ValidateEditUser
    };

    let username_input = text_input(&t("placeholder-username"), new_username)
        .on_input(Message::EditNewUsernameChanged)
        .on_submit(submit_action.clone())
        .id(Id::from(InputId::EditNewUsername))
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE);

    let password_input = text_input(&t("placeholder-password-keep-current"), new_password)
        .on_input(Message::EditNewPasswordChanged)
        .on_submit(submit_action)
        .id(Id::from(InputId::EditNewPassword))
        .secure(true)
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE);

    let admin_checkbox = if conn.is_admin {
        checkbox(is_admin)
            .label(t("label-admin"))
            .on_toggle(Message::EditIsAdminToggled)
            .size(TEXT_SIZE)
            .text_shaping(text::Shaping::Advanced)
    } else {
        checkbox(is_admin)
            .label(t("label-admin"))
            .size(TEXT_SIZE)
            .text_shaping(text::Shaping::Advanced)
    };

    let enabled_checkbox = if conn.is_admin {
        checkbox(enabled)
            .label(t("label-enabled"))
            .on_toggle(Message::EditEnabledToggled)
            .size(TEXT_SIZE)
            .text_shaping(text::Shaping::Advanced)
    } else {
        checkbox(enabled)
            .label(t("label-enabled"))
            .size(TEXT_SIZE)
            .text_shaping(text::Shaping::Advanced)
    };

    let permissions_title = shaped_text(t("label-permissions")).size(TEXT_SIZE);
    let permissions_row =
        build_permission_columns(permissions, conn, Message::EditPermissionToggled);

    let update_button = if can_update {
        button(shaped_text(t("button-update")).size(TEXT_SIZE))
            .on_press(Message::UpdateUserPressed)
            .padding(BUTTON_PADDING)
    } else {
        button(shaped_text(t("button-update")).size(TEXT_SIZE)).padding(BUTTON_PADDING)
    };

    let cancel_button = button(shaped_text(t("button-cancel")).size(TEXT_SIZE))
        .on_press(Message::CancelEditUser)
        .padding(BUTTON_PADDING)
        .style(btn::secondary);

    let mut update_items: Vec<Element<'a, Message>> = vec![update_title.into()];

    // Show error if present
    if let Some(error) = &user_management.edit_error {
        update_items.push(
            shaped_text_wrapped(error)
                .size(TEXT_SIZE)
                .width(Fill)
                .align_x(Center)
                .style(error_text_style)
                .into(),
        );
        update_items.push(Space::new().height(SPACER_SIZE_SMALL).into());
    } else {
        update_items.push(Space::new().height(SPACER_SIZE_MEDIUM).into());
    }

    update_items.extend([
        username_input.into(),
        password_input.into(),
        admin_checkbox.into(),
        enabled_checkbox.into(),
        Space::new().height(SPACER_SIZE_SMALL).into(),
        permissions_title.into(),
        permissions_row,
        Space::new().height(SPACER_SIZE_MEDIUM).into(),
        row![Space::new().width(Fill), cancel_button, update_button]
            .spacing(ELEMENT_SPACING)
            .into(),
    ]);

    let update_form = Column::with_children(update_items)
        .spacing(ELEMENT_SPACING)
        .padding(FORM_PADDING)
        .max_width(FORM_MAX_WIDTH);

    wrap_form(update_form)
}

// ============================================================================
// Main View Function
// ============================================================================

/// Displays user creation or edit form
///
/// Shows one of three forms based on state:
/// - User Create: Form to create a new user with username, password, admin flag, and permissions
/// - User Edit (Stage 1): Simple form to select which user to edit
/// - User Edit (Stage 2): Full update form with all user details pre-filled
pub fn users_view<'a>(
    conn: &'a ServerConnection,
    user_management: &'a UserManagementState,
    active_panel: ActivePanel,
) -> Element<'a, Message> {
    // Show Add User form
    if active_panel == ActivePanel::AddUser {
        return add_user_view(conn, user_management);
    }

    // Show Edit User panel
    if active_panel == ActivePanel::EditUser {
        return match &user_management.edit_state {
            UserEditState::None => {
                // Should never happen, but handle gracefully
                empty_panel()
            }
            UserEditState::SelectingUser { username } => {
                select_user_view(conn, user_management, username)
            }
            edit_state @ UserEditState::EditingUser { .. } => {
                update_user_view(conn, user_management, edit_state)
            }
        };
    }

    // Fallback (should never reach here)
    empty_panel()
}
