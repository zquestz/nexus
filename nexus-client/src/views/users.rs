//! User management panel view (list, create, edit, delete users)

use super::constants::{PERMISSION_USER_CREATE, PERMISSION_USER_DELETE, PERMISSION_USER_EDIT};
use super::layout::{scrollable_modal, scrollable_panel};
use crate::i18n::{t, translate_permission};
use crate::icon;
use crate::style::{
    BUTTON_PADDING, ELEMENT_SPACING, FORM_MAX_WIDTH, FORM_PADDING, ICON_BUTTON_PADDING,
    INPUT_PADDING, NO_SPACING, SCROLLBAR_PADDING, SERVER_LIST_BUTTON_HEIGHT,
    SERVER_LIST_DISCONNECT_ICON_SIZE, SERVER_LIST_ITEM_SPACING, SERVER_LIST_TEXT_SIZE,
    SIDEBAR_ACTION_ICON_SIZE, SPACER_SIZE_MEDIUM, SPACER_SIZE_SMALL, TEXT_SIZE, TITLE_SIZE,
    TOOLTIP_BACKGROUND_PADDING, TOOLTIP_GAP, TOOLTIP_PADDING, TOOLTIP_TEXT_SIZE,
    alternating_row_style, chat, content_background_style, danger_icon_button_style,
    error_text_style, muted_text_style, shaped_text, shaped_text_wrapped, tooltip_container_style,
    transparent_icon_button_style,
};
use crate::types::InputId;
use crate::types::{Message, ServerConnection, UserManagementMode, UserManagementState};
use iced::widget::button as btn;
use iced::widget::{
    Column, Id, Row, Space, button, checkbox, column, container, row, scrollable, text, text_input,
    tooltip,
};
use iced::{Center, Element, Fill, Theme, alignment};

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
// List View
// ============================================================================

/// Build the user list view (styled like server_list/bookmarks)
fn list_view<'a>(
    conn: &'a ServerConnection,
    user_management: &'a UserManagementState,
    theme: &Theme,
    current_username: &str,
) -> Element<'a, Message> {
    // Check permissions
    let can_create = conn.is_admin || conn.permissions.iter().any(|p| p == PERMISSION_USER_CREATE);
    let can_edit = conn.is_admin || conn.permissions.iter().any(|p| p == PERMISSION_USER_EDIT);
    let can_delete = conn.is_admin || conn.permissions.iter().any(|p| p == PERMISSION_USER_DELETE);

    // Build scrollable content (user list or status message)
    let scroll_content_inner: Element<'a, Message> = match &user_management.all_users {
        None => {
            // Loading state
            shaped_text(t("user-management-loading"))
                .size(TEXT_SIZE)
                .width(Fill)
                .align_x(Center)
                .style(muted_text_style)
                .into()
        }
        Some(Err(error)) => {
            // Error state
            shaped_text_wrapped(error)
                .size(TEXT_SIZE)
                .width(Fill)
                .align_x(Center)
                .style(error_text_style)
                .into()
        }
        Some(Ok(users)) => {
            if users.is_empty() {
                shaped_text(t("user-management-no-users"))
                    .size(TEXT_SIZE)
                    .width(Fill)
                    .align_x(Center)
                    .style(muted_text_style)
                    .into()
            } else {
                // Build user rows (styled like server_list bookmarks)
                let mut user_rows = Column::new().spacing(SERVER_LIST_ITEM_SPACING);

                for (index, user) in users.iter().enumerate() {
                    let admin_color = chat::admin(theme);
                    let is_self = user.username.to_lowercase() == current_username.to_lowercase();

                    // Username text with admin coloring
                    let username_text = if user.is_admin {
                        shaped_text(&user.username)
                            .size(SERVER_LIST_TEXT_SIZE)
                            .color(admin_color)
                    } else {
                        shaped_text(&user.username).size(SERVER_LIST_TEXT_SIZE)
                    };

                    // Username as a container that fills available space
                    let username_container = container(username_text)
                        .width(Fill)
                        .height(SERVER_LIST_BUTTON_HEIGHT)
                        .padding(INPUT_PADDING)
                        .align_y(alignment::Vertical::Center);

                    // Build row with username and action buttons
                    let mut user_row = Row::new()
                        .spacing(NO_SPACING)
                        .align_y(alignment::Vertical::Center)
                        .push(username_container);

                    // Edit button (icon style like bookmark edit)
                    // Hidden for self (server rejects self-edit anyway)
                    if can_edit && !is_self {
                        let edit_btn = tooltip(
                            transparent_edit_button(
                                icon::edit(),
                                Message::UserManagementEditClicked(user.username.clone()),
                            ),
                            container(shaped_text(t("tooltip-edit")).size(TOOLTIP_TEXT_SIZE))
                                .padding(TOOLTIP_BACKGROUND_PADDING)
                                .style(tooltip_container_style),
                            tooltip::Position::Top,
                        )
                        .gap(TOOLTIP_GAP)
                        .padding(TOOLTIP_PADDING);
                        user_row = user_row.push(edit_btn);
                    }

                    // Delete button (danger style like disconnect)
                    // Hidden for self (server rejects self-delete anyway)
                    if can_delete && !is_self {
                        let delete_btn = tooltip(
                            danger_delete_button(
                                icon::trash(),
                                Message::UserManagementDeleteClicked(user.username.clone()),
                            ),
                            container(shaped_text(t("tooltip-delete")).size(TOOLTIP_TEXT_SIZE))
                                .padding(TOOLTIP_BACKGROUND_PADDING)
                                .style(tooltip_container_style),
                            tooltip::Position::Top,
                        )
                        .gap(TOOLTIP_GAP)
                        .padding(TOOLTIP_PADDING);
                        user_row = user_row.push(delete_btn);
                    }

                    // Alternating row backgrounds
                    let is_even = index % 2 == 0;
                    let row_container = container(user_row)
                        .width(Fill)
                        .style(alternating_row_style(is_even));

                    user_rows = user_rows.push(row_container);
                }

                user_rows.width(Fill).into()
            }
        }
    };

    let scroll_content = scroll_content_inner;

    // Title (constrained to FORM_MAX_WIDTH, centered)
    let title = container(
        shaped_text(t("title-user-management"))
            .size(TITLE_SIZE)
            .width(Fill)
            .align_x(Center),
    )
    .width(FORM_MAX_WIDTH);

    // Error message (shown below title if present, constrained to FORM_MAX_WIDTH, centered)
    let error_element: Option<Element<'a, Message>> =
        user_management.list_error.as_ref().map(|error| {
            container(
                shaped_text_wrapped(error)
                    .size(TEXT_SIZE)
                    .width(Fill)
                    .align_x(Center)
                    .style(error_text_style),
            )
            .width(FORM_MAX_WIDTH)
            .into()
        });

    // Footer buttons
    let close_button = button(shaped_text(t("button-close")).size(TEXT_SIZE))
        .on_press(Message::CancelUserManagement)
        .padding(BUTTON_PADDING);

    // Create user button (icon style like add bookmark)
    let create_btn: Option<Element<'a, Message>> = if can_create {
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

    // Build footer row, constrained to standard width and centered
    let footer_row = if let Some(create_btn) = create_btn {
        row![create_btn, Space::new().width(Fill), close_button].spacing(ELEMENT_SPACING)
    } else {
        row![Space::new().width(Fill), close_button].spacing(ELEMENT_SPACING)
    };
    let footer = container(footer_row).max_width(FORM_MAX_WIDTH - FORM_PADDING * 2.0);

    let padded_footer = row![
        Space::new().width(SCROLLBAR_PADDING),
        footer,
        Space::new().width(SCROLLBAR_PADDING),
    ];

    // Scrollable content with symmetric padding for scrollbar space
    // Inner content matches footer width, spacers provide scrollbar room
    let scroll_inner = container(scroll_content).max_width(FORM_MAX_WIDTH - FORM_PADDING * 2.0);

    let padded_scroll_content = row![
        Space::new().width(SCROLLBAR_PADDING),
        scroll_inner,
        Space::new().width(SCROLLBAR_PADDING),
    ];

    // Build the form with max_width constraint on the whole thing
    let form = column![
        title,
        if let Some(err) = error_element {
            Element::from(err)
        } else {
            Element::from(Space::new().height(SPACER_SIZE_SMALL))
        },
        container(scrollable(padded_scroll_content)).height(Fill),
        Space::new().height(SPACER_SIZE_SMALL),
        padded_footer,
    ]
    .spacing(ELEMENT_SPACING)
    .padding(iced::Padding {
        top: FORM_PADDING,
        right: FORM_PADDING - SCROLLBAR_PADDING,
        bottom: FORM_PADDING,
        left: FORM_PADDING - SCROLLBAR_PADDING,
    })
    .max_width(FORM_MAX_WIDTH + SCROLLBAR_PADDING * 2.0)
    .height(Fill);

    // Center the form horizontally
    let centered_form = container(form).width(Fill).center_x(Fill);

    // Use container with background style
    container(centered_form)
        .width(Fill)
        .height(Fill)
        .style(content_background_style)
        .into()
}

// ============================================================================
// Create View
// ============================================================================

/// Build the create user form
fn create_view<'a>(
    conn: &'a ServerConnection,
    user_management: &'a UserManagementState,
) -> Element<'a, Message> {
    let title = shaped_text(t("title-user-create"))
        .size(TITLE_SIZE)
        .width(Fill)
        .align_x(Center);

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

    let admin_checkbox = if conn.is_admin {
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

    let permissions_title = shaped_text(t("label-permissions")).size(TEXT_SIZE);
    let permissions_row = build_permission_columns(
        &user_management.permissions,
        conn,
        Message::UserManagementPermissionToggled,
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
        enabled_checkbox.into(),
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
        .padding(FORM_PADDING)
        .max_width(FORM_MAX_WIDTH);

    scrollable_panel(form)
}

// ============================================================================
// Edit View
// ============================================================================

/// Build the edit user form
#[allow(clippy::too_many_arguments)]
fn edit_view<'a>(
    conn: &'a ServerConnection,
    user_management: &'a UserManagementState,
    original_username: &'a str,
    new_username: &'a str,
    new_password: &'a str,
    is_admin: bool,
    enabled: bool,
    permissions: &'a [(String, bool)],
) -> Element<'a, Message> {
    let title = shaped_text(t("title-update-user"))
        .size(TITLE_SIZE)
        .width(Fill)
        .align_x(Center);

    let subtitle = shaped_text_wrapped(original_username)
        .size(TEXT_SIZE)
        .width(Fill)
        .align_x(Center)
        .style(muted_text_style);

    let can_update = !new_username.trim().is_empty();

    // Helper for on_submit
    let submit_action = if can_update {
        Message::UserManagementUpdatePressed
    } else {
        Message::ValidateUserManagementEdit
    };

    let username_input = text_input(&t("placeholder-username"), new_username)
        .on_input(Message::UserManagementEditUsernameChanged)
        .on_submit(submit_action.clone())
        .id(Id::from(InputId::EditNewUsername))
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE);

    let password_input = text_input(&t("placeholder-password-keep-current"), new_password)
        .on_input(Message::UserManagementEditPasswordChanged)
        .on_submit(submit_action)
        .id(Id::from(InputId::EditNewPassword))
        .secure(true)
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE);

    let admin_checkbox = if conn.is_admin {
        checkbox(is_admin)
            .label(t("label-admin"))
            .on_toggle(Message::UserManagementEditIsAdminToggled)
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
            .on_toggle(Message::UserManagementEditEnabledToggled)
            .size(TEXT_SIZE)
            .text_shaping(text::Shaping::Advanced)
    } else {
        checkbox(enabled)
            .label(t("label-enabled"))
            .size(TEXT_SIZE)
            .text_shaping(text::Shaping::Advanced)
    };

    let permissions_title = shaped_text(t("label-permissions")).size(TEXT_SIZE);
    let permissions_row = build_permission_columns(
        permissions,
        conn,
        Message::UserManagementEditPermissionToggled,
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
    if let Some(error) = &user_management.edit_error {
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
        enabled_checkbox.into(),
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
        .padding(FORM_PADDING)
        .max_width(FORM_MAX_WIDTH);

    scrollable_panel(form)
}

// ============================================================================
// Delete Confirmation Modal
// ============================================================================

/// Build the delete confirmation modal
fn confirm_delete_modal<'a>(username: &'a str) -> Element<'a, Message> {
    let title = shaped_text(t("title-confirm-delete"))
        .size(TITLE_SIZE)
        .width(Fill)
        .align_x(Center);

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

    let form = column![
        title,
        Space::new().height(SPACER_SIZE_MEDIUM),
        message,
        Space::new().height(SPACER_SIZE_MEDIUM),
        row![Space::new().width(Fill), cancel_button, confirm_button].spacing(ELEMENT_SPACING),
    ]
    .spacing(ELEMENT_SPACING)
    .padding(FORM_PADDING)
    .max_width(FORM_MAX_WIDTH);

    scrollable_modal(form)
}

// ============================================================================
// Helper for t_args
// ============================================================================

fn t_args(key: &str, args: &[(&str, &str)]) -> String {
    crate::i18n::t_args(key, args)
}

// ============================================================================
// Main View Function
// ============================================================================

/// Displays the user management panel
///
/// Shows one of four views based on mode:
/// - List: Shows all users with edit/delete buttons
/// - Create: Form to create a new user
/// - Edit: Form to edit an existing user
/// - ConfirmDelete: Modal to confirm user deletion
pub fn users_view<'a>(
    conn: &'a ServerConnection,
    user_management: &'a UserManagementState,
    theme: &Theme,
) -> Element<'a, Message> {
    match &user_management.mode {
        UserManagementMode::List => list_view(conn, user_management, theme, &conn.username),
        UserManagementMode::Create => create_view(conn, user_management),
        UserManagementMode::Edit {
            original_username,
            new_username,
            new_password,
            is_admin,
            enabled,
            permissions,
        } => edit_view(
            conn,
            user_management,
            original_username,
            new_username,
            new_password,
            *is_admin,
            *enabled,
            permissions,
        ),
        UserManagementMode::ConfirmDelete { username } => confirm_delete_modal(username),
    }
}
