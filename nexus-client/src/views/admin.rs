//! User management forms (create/delete users)

use crate::types::{InputId, Message, ServerConnection, UserManagementState};
use iced::widget::{button, checkbox, column, container, row, text, text_input, Column};
use iced::{Center, Element, Fill};

/// Displays user creation or deletion form
pub fn admin_view<'a>(
    _conn: &'a ServerConnection,
    user_management: &'a UserManagementState,
    show_add_user: bool,
    show_delete_user: bool,
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

        let admin_checkbox = checkbox("Make Admin", user_management.is_admin)
            .on_toggle(Message::AdminIsAdminToggled)
            .size(14);

        let permissions_title = text("Permissions:").size(14);
        let mut permissions_column = Column::new().spacing(5);
        for (permission, enabled) in &user_management.permissions {
            let perm_name = permission.clone();
            let checkbox_widget = checkbox(permission.as_str(), *enabled)
                .on_toggle(move |checked| {
                    Message::AdminPermissionToggled(perm_name.clone(), checked)
                })
                .size(14);
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

    // Show Delete User panel
    if show_delete_user {
        let delete_title = text("User Delete").size(20).width(Fill).align_x(Center);

        let can_delete = !user_management.delete_username.trim().is_empty();

        let username_input = text_input("Username", &user_management.delete_username)
            .on_input(Message::DeleteUsernameChanged)
            .on_submit(if can_delete {
                Message::DeleteUserPressed(user_management.delete_username.to_string())
            } else {
                Message::DeleteUsernameChanged(String::new())
            })
            .id(text_input::Id::from(InputId::DeleteUsername))
            .padding(8)
            .size(14);

        let delete_button = if can_delete {
            button(text("Delete").size(14))
                .on_press(Message::DeleteUserPressed(
                    user_management.delete_username.to_string(),
                ))
                .padding(10)
        } else {
            button(text("Delete").size(14)).padding(10)
        };

        let cancel_button = button(text("Cancel").size(14))
            .on_press(Message::ToggleDeleteUser)
            .padding(10);

        let delete_form = column![
            delete_title,
            text("").size(15),
            username_input,
            text("").size(10),
            text("Warning: Deletion is permanent!")
                .color([1.0, 0.5, 0.0])
                .size(14),
            text("").size(10),
            row![delete_button, cancel_button,].spacing(10),
        ]
        .spacing(10)
        .padding(20)
        .max_width(400);

        return container(delete_form)
            .width(Fill)
            .height(Fill)
            .center(Fill)
            .into();
    }

    // Fallback (should never reach here since we only call admin_view when at least one is true)
    container(text("")).width(Fill).height(Fill).into()
}