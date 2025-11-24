//! User management forms (create/edit/delete users)

use super::style::{
    BUTTON_PADDING, ELEMENT_SPACING, FORM_MAX_WIDTH, FORM_PADDING, INPUT_PADDING,
    SPACER_SIZE_LARGE, SPACER_SIZE_MEDIUM, SPACER_SIZE_SMALL, TEXT_SIZE, TITLE_SIZE,
};
use crate::types::{InputId, Message, ServerConnection, UserEditState, UserManagementState};
use iced::widget::{Column, button, checkbox, column, container, row, text, text_input};
use iced::{Center, Element, Fill};

// Permission constant
const PERMISSION_USER_DELETE: &str = "user_delete";

/// Helper function to create an empty fallback panel
fn empty_panel<'a>() -> Element<'a, Message> {
    container(text("")).width(Fill).height(Fill).into()
}

/// Displays user creation or edit form
///
/// Shows one of three forms based on state:
/// - User Create: Form to create a new user with username, password, admin flag, and permissions
/// - User Edit (Stage 1): Simple form to select which user to edit
/// - User Edit (Stage 2): Full update form with all user details pre-filled
pub fn users_view<'a>(
    conn: &'a ServerConnection,
    user_management: &'a UserManagementState,
    show_add_user: bool,
    show_edit_user: bool,
) -> Element<'a, Message> {
    // Show Add User form
    if show_add_user {
        let create_title = text("User Create")
            .size(TITLE_SIZE)
            .width(Fill)
            .align_x(Center);

        let can_create = !user_management.username.trim().is_empty()
            && !user_management.password.trim().is_empty();

        // Helper for on_submit
        let submit_action = if can_create {
            Message::CreateUserPressed
        } else {
            Message::AdminUsernameChanged(String::new())
        };

        let username_input = text_input("Username", &user_management.username)
            .on_input(Message::AdminUsernameChanged)
            .on_submit(submit_action.clone())
            .id(text_input::Id::from(InputId::AdminUsername))
            .padding(INPUT_PADDING)
            .size(TEXT_SIZE);

        let password_input = text_input("Password", &user_management.password)
            .on_input(Message::AdminPasswordChanged)
            .on_submit(submit_action)
            .id(text_input::Id::from(InputId::AdminPassword))
            .secure(true)
            .padding(INPUT_PADDING)
            .size(TEXT_SIZE);

        let admin_checkbox = if conn.is_admin {
            checkbox("Make Admin", user_management.is_admin)
                .on_toggle(Message::AdminIsAdminToggled)
                .size(TEXT_SIZE)
        } else {
            checkbox("Make Admin", user_management.is_admin).size(TEXT_SIZE)
        };

        let permissions_title = text("Permissions:").size(TEXT_SIZE);
        let mut permissions_column = Column::new().spacing(SPACER_SIZE_SMALL);
        for (permission, enabled) in &user_management.permissions {
            let perm_name = permission.clone();
            let checkbox_widget = if conn.is_admin || conn.permissions.contains(permission) {
                // Can toggle permissions they have
                checkbox(permission.as_str(), *enabled)
                    .on_toggle(move |checked| {
                        Message::AdminPermissionToggled(perm_name.clone(), checked)
                    })
                    .size(TEXT_SIZE)
            } else {
                // Cannot toggle permissions they don't have
                checkbox(permission.as_str(), *enabled).size(TEXT_SIZE)
            };
            permissions_column = permissions_column.push(checkbox_widget);
        }

        let create_button = if can_create {
            button(text("Create").size(TEXT_SIZE))
                .on_press(Message::CreateUserPressed)
                .padding(BUTTON_PADDING)
        } else {
            button(text("Create").size(TEXT_SIZE)).padding(BUTTON_PADDING)
        };

        let cancel_button = button(text("Cancel").size(TEXT_SIZE))
            .on_press(Message::ToggleAddUser)
            .padding(BUTTON_PADDING);

        let create_form = column![
            create_title,
            text("").size(SPACER_SIZE_LARGE),
            username_input,
            password_input,
            admin_checkbox,
            text("").size(SPACER_SIZE_SMALL),
            permissions_title,
            permissions_column,
            text("").size(SPACER_SIZE_MEDIUM),
            row![create_button, cancel_button,].spacing(ELEMENT_SPACING),
        ]
        .spacing(ELEMENT_SPACING)
        .padding(FORM_PADDING)
        .max_width(FORM_MAX_WIDTH);

        return container(create_form)
            .width(Fill)
            .height(Fill)
            .center(Fill)
            .into();
    }

    // Show Edit User panel
    if show_edit_user {
        match &user_management.edit_state {
            UserEditState::None => {
                // Should never happen, but handle gracefully
                empty_panel()
            }
            UserEditState::SelectingUser { username } => {
                // Stage 1: Simple form to select which user to edit
                let edit_title = text("User Edit")
                    .size(TITLE_SIZE)
                    .width(Fill)
                    .align_x(Center);

                let can_edit = !username.trim().is_empty();
                let can_delete = !username.trim().is_empty()
                    && (conn.is_admin
                        || conn.permissions.iter().any(|p| p == PERMISSION_USER_DELETE));

                // Helper for on_submit
                let submit_action = if can_edit {
                    Message::EditUserPressed
                } else {
                    Message::EditUsernameChanged(String::new())
                };

                let username_input = text_input("Username", username)
                    .on_input(Message::EditUsernameChanged)
                    .on_submit(submit_action)
                    .id(text_input::Id::from(InputId::EditUsername))
                    .padding(INPUT_PADDING)
                    .size(TEXT_SIZE);

                let edit_button = if can_edit {
                    button(text("Edit").size(TEXT_SIZE))
                        .on_press(Message::EditUserPressed)
                        .padding(BUTTON_PADDING)
                } else {
                    button(text("Edit").size(TEXT_SIZE)).padding(BUTTON_PADDING)
                };

                let delete_button = if can_delete {
                    button(text("Delete").size(TEXT_SIZE))
                        .on_press(Message::DeleteUserPressed(username.to_string()))
                        .padding(BUTTON_PADDING)
                } else {
                    button(text("Delete").size(TEXT_SIZE)).padding(BUTTON_PADDING)
                };

                let cancel_button = button(text("Cancel").size(TEXT_SIZE))
                    .on_press(Message::CancelEditUser)
                    .padding(BUTTON_PADDING);

                let edit_form = column![
                    edit_title,
                    text("").size(SPACER_SIZE_LARGE),
                    username_input,
                    text("").size(SPACER_SIZE_MEDIUM),
                    row![edit_button, delete_button, cancel_button,].spacing(ELEMENT_SPACING),
                ]
                .spacing(ELEMENT_SPACING)
                .padding(FORM_PADDING)
                .max_width(FORM_MAX_WIDTH);

                container(edit_form)
                    .width(Fill)
                    .height(Fill)
                    .center(Fill)
                    .into()
            }
            UserEditState::EditingUser {
                original_username: _,
                new_username,
                new_password,
                is_admin,
                permissions,
            } => {
                // Stage 2: Full edit form with current values
                let update_title = text("Update User")
                    .size(TITLE_SIZE)
                    .width(Fill)
                    .align_x(Center);

                let can_update = !new_username.trim().is_empty();

                // Helper for on_submit
                let submit_action = if can_update {
                    Message::UpdateUserPressed
                } else {
                    Message::EditNewUsernameChanged(String::new())
                };

                let username_input = text_input("Username", new_username)
                    .on_input(Message::EditNewUsernameChanged)
                    .on_submit(submit_action.clone())
                    .id(text_input::Id::from(InputId::EditNewUsername))
                    .padding(INPUT_PADDING)
                    .size(TEXT_SIZE);

                let password_input =
                    text_input("Password (leave empty to keep current)", new_password)
                        .on_input(Message::EditNewPasswordChanged)
                        .on_submit(submit_action)
                        .id(text_input::Id::from(InputId::EditNewPassword))
                        .secure(true)
                        .padding(INPUT_PADDING)
                        .size(TEXT_SIZE);

                let admin_checkbox = if conn.is_admin {
                    checkbox("Make Admin", *is_admin)
                        .on_toggle(Message::EditIsAdminToggled)
                        .size(TEXT_SIZE)
                } else {
                    checkbox("Make Admin", *is_admin).size(TEXT_SIZE)
                };

                let permissions_title = text("Permissions:").size(TEXT_SIZE);
                let mut permissions_column = Column::new().spacing(SPACER_SIZE_SMALL);
                for (permission, enabled) in permissions {
                    let perm_name = permission.clone();
                    let checkbox_widget = if conn.is_admin || conn.permissions.contains(permission)
                    {
                        // Can toggle permissions they have
                        checkbox(permission.as_str(), *enabled)
                            .on_toggle(move |checked| {
                                Message::EditPermissionToggled(perm_name.clone(), checked)
                            })
                            .size(TEXT_SIZE)
                    } else {
                        // Cannot toggle permissions they don't have
                        checkbox(permission.as_str(), *enabled).size(TEXT_SIZE)
                    };
                    permissions_column = permissions_column.push(checkbox_widget);
                }

                let update_button = if can_update {
                    button(text("Update").size(TEXT_SIZE))
                        .on_press(Message::UpdateUserPressed)
                        .padding(BUTTON_PADDING)
                } else {
                    button(text("Update").size(TEXT_SIZE)).padding(BUTTON_PADDING)
                };

                let cancel_button = button(text("Cancel").size(TEXT_SIZE))
                    .on_press(Message::CancelEditUser)
                    .padding(BUTTON_PADDING);

                let update_form = column![
                    update_title,
                    text("").size(SPACER_SIZE_LARGE),
                    username_input,
                    password_input,
                    admin_checkbox,
                    text("").size(SPACER_SIZE_SMALL),
                    permissions_title,
                    permissions_column,
                    text("").size(SPACER_SIZE_MEDIUM),
                    row![update_button, cancel_button,].spacing(ELEMENT_SPACING),
                ]
                .spacing(ELEMENT_SPACING)
                .padding(FORM_PADDING)
                .max_width(FORM_MAX_WIDTH);

                container(update_form)
                    .width(Fill)
                    .height(Fill)
                    .center(Fill)
                    .into()
            }
        }
    } else {
        // Fallback (should never reach here)
        empty_panel()
    }
}
