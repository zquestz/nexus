//! Server info panel view

use crate::i18n::t;
use crate::style::{
    BUTTON_PADDING, ELEMENT_SPACING, FORM_MAX_WIDTH, FORM_PADDING, INPUT_PADDING,
    SPACER_SIZE_MEDIUM, SPACER_SIZE_SMALL, SUBHEADING_SIZE, TEXT_SIZE, TITLE_SIZE,
    content_background_style, error_text_style, shaped_text, shaped_text_wrapped,
    subheading_text_style,
};
use crate::types::{InputId, Message, ServerInfoEditState};
use iced::widget::button as btn;
use iced::widget::{Id, Space, button, column, container, row, text_input};
use iced::{Center, Element, Fill};
use iced_aw::NumberInput;

/// Data needed to render the server info panel
pub struct ServerInfoData<'a> {
    /// Server name (if provided)
    pub name: Option<String>,
    /// Server description (if provided)
    pub description: Option<String>,
    /// Server version (if provided)
    pub version: Option<String>,
    /// Max connections per IP (admin only)
    pub max_connections_per_ip: Option<u32>,
    /// Whether the current user is an admin
    pub is_admin: bool,
    /// Edit state (Some when in edit mode)
    pub edit_state: Option<&'a ServerInfoEditState>,
}

/// Render the server info panel
///
/// Displays server information received during login.
/// Only shows fields that were provided by the server.
/// Admins see an Edit button to modify server configuration.
pub fn server_info_view(data: &ServerInfoData<'_>) -> Element<'static, Message> {
    if let Some(edit_state) = data.edit_state {
        server_info_edit_view(edit_state)
    } else {
        server_info_display_view(data)
    }
}

/// Render the server info display view (read-only)
fn server_info_display_view(data: &ServerInfoData<'_>) -> Element<'static, Message> {
    // Server name as the title (fallback to generic title if no name)
    let title_text = data.name.clone().unwrap_or_else(|| t("title-server-info"));
    let title = shaped_text(title_text)
        .size(TITLE_SIZE)
        .width(Fill)
        .align_x(Center);

    let mut content = column![title].spacing(ELEMENT_SPACING);

    // Server description directly under title (no label)
    if let Some(description) = &data.description
        && !description.is_empty()
    {
        content = content.push(
            shaped_text_wrapped(description.clone())
                .size(TEXT_SIZE)
                .width(Fill)
                .align_x(Center),
        );
    }

    content = content.push(Space::new().height(SPACER_SIZE_MEDIUM));

    // Details subheading
    let details_heading = shaped_text(t("label-details"))
        .size(SUBHEADING_SIZE)
        .style(subheading_text_style);
    content = content.push(details_heading);

    // Server version
    if let Some(version) = &data.version {
        let label = shaped_text(t("label-server-version")).size(TEXT_SIZE);
        let value = shaped_text(version.clone()).size(TEXT_SIZE);
        let info_row = row![label, Space::new().width(ELEMENT_SPACING), value].align_y(Center);
        content = content.push(info_row);
    }

    // Max connections per IP (admin only)
    if let Some(max_conn) = data.max_connections_per_ip {
        let label = shaped_text(t("label-max-connections-per-ip")).size(TEXT_SIZE);
        let value = shaped_text(max_conn.to_string()).size(TEXT_SIZE);
        let info_row = row![label, Space::new().width(ELEMENT_SPACING), value].align_y(Center);
        content = content.push(info_row);
    }

    content = content.push(Space::new().height(SPACER_SIZE_MEDIUM));

    // Buttons: Edit (admin only, secondary) and Close (primary)
    let buttons = if data.is_admin {
        row![
            Space::new().width(Fill),
            button(shaped_text(t("button-edit")).size(TEXT_SIZE))
                .on_press(Message::EditServerInfoPressed)
                .padding(BUTTON_PADDING)
                .style(btn::secondary),
            button(shaped_text(t("button-close")).size(TEXT_SIZE))
                .on_press(Message::CloseServerInfo)
                .padding(BUTTON_PADDING),
        ]
        .spacing(ELEMENT_SPACING)
    } else {
        row![
            Space::new().width(Fill),
            button(shaped_text(t("button-close")).size(TEXT_SIZE))
                .on_press(Message::CloseServerInfo)
                .padding(BUTTON_PADDING),
        ]
        .spacing(ELEMENT_SPACING)
    };

    content = content.push(buttons);

    let form = content.padding(FORM_PADDING).max_width(FORM_MAX_WIDTH);

    container(form)
        .width(Fill)
        .height(Fill)
        .center(Fill)
        .style(content_background_style)
        .into()
}

/// Render the server info edit view (editable form)
fn server_info_edit_view(edit_state: &ServerInfoEditState) -> Element<'static, Message> {
    let title = shaped_text(t("title-edit-server-info"))
        .size(TITLE_SIZE)
        .width(Fill)
        .align_x(Center);

    let mut form_items: Vec<Element<'static, Message>> = vec![title.into()];

    // Show error if present
    if let Some(error) = &edit_state.error {
        form_items.push(
            shaped_text_wrapped(error.clone())
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

    // Server name input
    let name_label = shaped_text(t("label-server-name")).size(TEXT_SIZE);
    let name_input = text_input(&t("placeholder-server-name"), &edit_state.name)
        .on_input(Message::EditServerInfoNameChanged)
        .on_submit(Message::UpdateServerInfoPressed)
        .id(Id::from(InputId::EditServerInfoName))
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE);
    form_items.push(name_label.into());
    form_items.push(name_input.into());

    // Server description input
    let desc_label = shaped_text(t("label-server-description")).size(TEXT_SIZE);
    let desc_input = text_input(
        &t("placeholder-server-description"),
        &edit_state.description,
    )
    .on_input(Message::EditServerInfoDescriptionChanged)
    .on_submit(Message::UpdateServerInfoPressed)
    .id(Id::from(InputId::EditServerInfoDescription))
    .padding(INPUT_PADDING)
    .size(TEXT_SIZE);
    form_items.push(desc_label.into());
    form_items.push(desc_input.into());

    // Max connections per IP input using NumberInput
    let max_conn_label = shaped_text(t("label-max-connections-per-ip")).size(TEXT_SIZE);
    let current_value = edit_state.max_connections_per_ip.unwrap_or(1);
    let max_conn_input: Element<'static, Message> = NumberInput::new(
        &current_value,
        1..=u32::MAX,
        Message::EditServerInfoMaxConnectionsChanged,
    )
    .padding(INPUT_PADDING)
    .into();
    form_items.push(max_conn_label.into());
    form_items.push(max_conn_input);

    form_items.push(Space::new().height(SPACER_SIZE_MEDIUM).into());

    // Buttons: Cancel (secondary) and Save (primary)
    let buttons = row![
        Space::new().width(Fill),
        button(shaped_text(t("button-cancel")).size(TEXT_SIZE))
            .on_press(Message::CancelEditServerInfo)
            .padding(BUTTON_PADDING)
            .style(btn::secondary),
        button(shaped_text(t("button-save")).size(TEXT_SIZE))
            .on_press(Message::UpdateServerInfoPressed)
            .padding(BUTTON_PADDING),
    ]
    .spacing(ELEMENT_SPACING);

    form_items.push(buttons.into());

    let form = iced::widget::Column::with_children(form_items)
        .spacing(ELEMENT_SPACING)
        .padding(FORM_PADDING)
        .max_width(FORM_MAX_WIDTH);

    container(form)
        .width(Fill)
        .height(Fill)
        .center(Fill)
        .style(content_background_style)
        .into()
}
