//! Server info panel view

use crate::i18n::t;
use crate::style::{
    BUTTON_PADDING, ELEMENT_SPACING, FORM_MAX_WIDTH, FORM_PADDING, SPACER_SIZE_MEDIUM, TEXT_SIZE,
    TITLE_SIZE, content_background_style, shaped_text,
};
use crate::types::Message;
use iced::widget::{Space, button, column, container, row};
use iced::{Center, Element, Fill};

/// Data needed to render the server info panel
pub struct ServerInfoData {
    /// Server name (if provided)
    pub name: Option<String>,
    /// Server description (if provided)
    pub description: Option<String>,
    /// Server version (if provided)
    pub version: Option<String>,
    /// Max connections per IP (admin only)
    pub max_connections_per_ip: Option<u32>,
}

/// Render the server info panel
///
/// Displays server information received during login.
/// Only shows fields that were provided by the server.
pub fn server_info_view(data: &ServerInfoData) -> Element<'static, Message> {
    let title = shaped_text(t("title-server-info"))
        .size(TITLE_SIZE)
        .width(Fill)
        .align_x(Center);

    let mut content =
        column![title, Space::new().height(SPACER_SIZE_MEDIUM),].spacing(ELEMENT_SPACING);

    // Server name
    if let Some(name) = &data.name {
        let label = shaped_text(t("label-server-name")).size(TEXT_SIZE);
        let value = shaped_text(name.clone()).size(TEXT_SIZE);
        let info_row = row![label, Space::new().width(ELEMENT_SPACING), value].align_y(Center);
        content = content.push(info_row);
    }

    // Server description (only if non-empty)
    if let Some(description) = &data.description
        && !description.is_empty()
    {
        let label = shaped_text(t("label-server-description")).size(TEXT_SIZE);
        let value = shaped_text(description.clone()).size(TEXT_SIZE);
        let info_row = row![label, Space::new().width(ELEMENT_SPACING), value].align_y(Center);
        content = content.push(info_row);
    }

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

    // Close button (primary style since it's the default action)
    let buttons = row![
        Space::new().width(Fill),
        button(shaped_text(t("button-close")).size(TEXT_SIZE))
            .on_press(Message::CloseServerInfo)
            .padding(BUTTON_PADDING),
    ]
    .spacing(ELEMENT_SPACING);

    content = content.push(Space::new().height(SPACER_SIZE_MEDIUM));
    content = content.push(buttons);

    let form = content.padding(FORM_PADDING).max_width(FORM_MAX_WIDTH);

    container(form)
        .width(Fill)
        .height(Fill)
        .center(Fill)
        .style(content_background_style)
        .into()
}
