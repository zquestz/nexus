//! User management forms (create/edit/delete users)

use crate::types::{InputId, Message, ServerConnection, UserEditState, UserManagementState};
use iced::widget::{button, checkbox, column, container, row, text, text_input, Column};
use iced::{Center, Element, Fill};

/// Displays user creation or edit form
pub fn users_view<'a>(
    conn: &'a ServerConnection,
    user_management: &'a UserManagementState,
    show_add_user: bool,
    show_edit_user: bool,
) -> Element<'a, Message> {
    // Show Add User form
    if show_add_user {
        let create_title = text("User Create").size(20).width(Fill).align_x(Center);

        let can_create = !user_management.username.trim().is_empty()
            && !user_management.password.trim().is_empty();

        let username_input = text_input("Username", &user_management.username)
            .on_input(Message::AdminUsernameChanged)
            .on_submit(if can_create {
                Message::CreateUserPressed
            } else {
                Message::AdminUsernameChanged(String::new())
            })
            .id(text_input::Id::from(InputId::AdminUsername))
            .padding(8)
            .size(14);

        let password_input = text_input("Password", &user_management.password)
            .on_input(Message::AdminPasswordChanged)
            .on_submit(if can_create {
                Message::CreateUserPressed
            } else {
                Message::AdminPasswordChanged(String::new())
            })
            .id(text_input::Id::from(InputId::AdminPassword))
            .secure(true)
            .padding(8)
            .size(14);

        let admin_checkbox = if conn.is_admin {
            checkbox("Make Admin", user_management.is_admin)
                .on_toggle(Message::AdminIsAdminToggled)
                .size(14)
        } else {
            checkbox("Make Admin", user_management.is_admin)
                .size(14)
        };

        let permissions_title = text("Permissions:").size(14);
        let mut permissions_column = Column::new().spacing(5);
        for (permission, enabled) in &user_management.permissions {
            let perm_name = permission.clone();
            let checkbox_widget = if conn.is_admin || conn.permissions.contains(permission) {
                // Can toggle permissions they have
                checkbox(permission.as_str(), *enabled)
                    .on_toggle(move |checked| {
                        Message::AdminPermissionToggled(perm_name.clone(), checked)
                    })
                    .size(14)
            } else {
                // Cannot toggle permissions they don't have
                checkbox(permission.as_str(), *enabled)
                    .size(14)
            };
            permissions_column = permissions_column.push(checkbox_widget);
        }

        let create_button = if can_create {
            button(text("Create").size(14))
                .on_press(Message::CreateUserPressed)
                .padding(10)
        } else {
            button(text("Create").size(14)).padding(10)
        };

        let cancel_button = button(text("Cancel").size(14))
            .on_press(Message::ToggleAddUser)
            .padding(10);

        let create_form = column![
            create_title,
            text("").size(15),
            username_input,
            password_input,
            admin_checkbox,
            text("").size(5),
            permissions_title,
            permissions_column,
            text("").size(10),
            row![create_button, cancel_button,].spacing(10),
        ]
        .spacing(10)
        .padding(20)
        .max_width(400);

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
                container(text("")).width(Fill).height(Fill).into()
            }
            UserEditState::SelectingUser { username } => {
                // Stage 1: Simple form to select which user to edit
                let edit_title = text("User Edit").size(20).width(Fill).align_x(Center);

                let can_edit = !username.trim().is_empty();
                let can_delete = !username.trim().is_empty() 
                    && (conn.is_admin || conn.permissions.contains(&"user_delete".to_string()));

                let username_input = text_input("Username", username)
                    .on_input(Message::EditUsernameChanged)
                    .on_submit(if can_edit {
                        Message::EditUserPressed
                    } else {
                        Message::EditUsernameChanged(String::new())
                    })
                    .id(text_input::Id::from(InputId::EditUsername))
                    .padding(8)
                    .size(14);

                let edit_button = if can_edit {
                    button(text("Edit").size(14))
                        .on_press(Message::EditUserPressed)
                        .padding(10)
                } else {
                    button(text("Edit").size(14)).padding(10)
                };

                let delete_button = if can_delete {
                    button(text("Delete").size(14))
                        .on_press(Message::DeleteUserPressed(username.to_string()))
                        .padding(10)
                } else {
                    button(text("Delete").size(14)).padding(10)
                };

                let cancel_button = button(text("Cancel").size(14))
                    .on_press(Message::CancelEditUser)
                    .padding(10);

                let edit_form = column![
                    edit_title,
                    text("").size(15),
                    username_input,
                    text("").size(10),
                    row![edit_button, delete_button, cancel_button,].spacing(10),
                ]
                .spacing(10)
                .padding(20)
                .max_width(400);

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
                let update_title = text("Update User").size(20).width(Fill).align_x(Center);

                let can_update = !new_username.trim().is_empty();

                let username_input = text_input("Username", new_username)
                    .on_input(Message::EditNewUsernameChanged)
                    .on_submit(if can_update {
                        Message::UpdateUserPressed
                    } else {
                        Message::EditNewUsernameChanged(String::new())
                    })
                    .id(text_input::Id::from(InputId::EditNewUsername))
                    .padding(8)
                    .size(14);

                let password_input = text_input("Password (leave empty to keep current)", new_password)
                    .on_input(Message::EditNewPasswordChanged)
                    .on_submit(if can_update {
                        Message::UpdateUserPressed
                    } else {
                        Message::EditNewPasswordChanged(String::new())
                    })
                    .id(text_input::Id::from(InputId::EditNewPassword))
                    .secure(true)
                    .padding(8)
                    .size(14);

                let admin_checkbox = if conn.is_admin {
                    checkbox("Make Admin", *is_admin)
                        .on_toggle(Message::EditIsAdminToggled)
                        .size(14)
                } else {
                    checkbox("Make Admin", *is_admin)
                        .size(14)
                };

                let permissions_title = text("Permissions:").size(14);
                let mut permissions_column = Column::new().spacing(5);
                for (permission, enabled) in permissions {
                    let perm_name = permission.clone();
                    let checkbox_widget = if conn.is_admin || conn.permissions.contains(permission) {
                        // Can toggle permissions they have
                        checkbox(permission.as_str(), *enabled)
                            .on_toggle(move |checked| {
                                Message::EditPermissionToggled(perm_name.clone(), checked)
                            })
                            .size(14)
                    } else {
                        // Cannot toggle permissions they don't have
                        checkbox(permission.as_str(), *enabled)
                            .size(14)
                    };
                    permissions_column = permissions_column.push(checkbox_widget);
                }

                let update_button = if can_update {
                    button(text("Update").size(14))
                        .on_press(Message::UpdateUserPressed)
                        .padding(10)
                } else {
                    button(text("Update").size(14)).padding(10)
                };

                let cancel_button = button(text("Cancel").size(14))
                    .on_press(Message::CancelEditUser)
                    .padding(10);

                let update_form = column![
                    update_title,
                    text("").size(15),
                    username_input,
                    password_input,
                    admin_checkbox,
                    text("").size(5),
                    permissions_title,
                    permissions_column,
                    text("").size(10),
                    row![update_button, cancel_button,].spacing(10),
                ]
                .spacing(10)
                .padding(20)
                .max_width(400);

                container(update_form)
                    .width(Fill)
                    .height(Fill)
                    .center(Fill)
                    .into()
            }
        }
    } else {
        // Fallback (should never reach here since we only call admin_view when at least one is true)
        container(text("")).width(Fill).height(Fill).into()
    }
}
