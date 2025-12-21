//! Server info panel view

use super::layout::scrollable_panel;
use crate::i18n::t;
use crate::image::CachedImage;
use crate::style::{
    BUTTON_PADDING, ELEMENT_SPACING, FORM_MAX_WIDTH, FORM_PADDING, INPUT_PADDING,
    SERVER_IMAGE_PREVIEW_SIZE, SPACER_SIZE_MEDIUM, SPACER_SIZE_SMALL, SUBHEADING_SIZE, TEXT_SIZE,
    TITLE_SIZE, error_text_style, shaped_text, shaped_text_wrapped, subheading_text_style,
};
use crate::types::{InputId, Message, ServerInfoEditState};
use iced::widget::button as btn;
use iced::widget::{Id, Space, button, image, row, svg, text_input};
use iced::{Center, Element, Fill, Length};
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
    /// Cached server image for display (None if no image set)
    pub cached_server_image: Option<&'a CachedImage>,
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
    // Server image at the top (if set)
    let image_element: Option<Element<'static, Message>> =
        data.cached_server_image
            .map(|cached_image| match cached_image {
                CachedImage::Raster(handle) => image(handle.clone())
                    .width(Length::Fill)
                    .content_fit(iced::ContentFit::ScaleDown)
                    .into(),
                CachedImage::Svg(handle) => svg(handle.clone())
                    .width(Length::Fill)
                    .content_fit(iced::ContentFit::ScaleDown)
                    .into(),
            });

    // Server name as the title (fallback to generic title if no name)
    let title_text = data.name.clone().unwrap_or_else(|| t("title-server-info"));
    let title = shaped_text(title_text)
        .size(TITLE_SIZE)
        .width(Fill)
        .align_x(Center);

    // Server description directly under title (no label)
    let description_element: Option<Element<'static, Message>> = data
        .description
        .as_ref()
        .filter(|d| !d.is_empty())
        .map(|description| {
            shaped_text_wrapped(description.clone())
                .size(TEXT_SIZE)
                .width(Fill)
                .align_x(Center)
                .into()
        });

    // Details subheading
    let details_heading = shaped_text(t("label-details"))
        .size(SUBHEADING_SIZE)
        .style(subheading_text_style);

    // Server version
    let version_row: Option<Element<'static, Message>> = data.version.as_ref().map(|version| {
        let label = shaped_text(t("label-server-version")).size(TEXT_SIZE);
        let value = shaped_text(version.clone()).size(TEXT_SIZE);
        row![label, Space::new().width(ELEMENT_SPACING), value]
            .align_y(Center)
            .into()
    });

    // Max connections per IP (admin only)
    let max_conn_row: Option<Element<'static, Message>> =
        data.max_connections_per_ip.map(|max_conn| {
            let label = shaped_text(t("label-max-connections-per-ip")).size(TEXT_SIZE);
            let value = shaped_text(max_conn.to_string()).size(TEXT_SIZE);
            row![label, Space::new().width(ELEMENT_SPACING), value]
                .align_y(Center)
                .into()
        });

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

    // Build content column
    let mut items: Vec<Element<'static, Message>> = Vec::new();

    if let Some(img) = image_element {
        items.push(img);
    }
    items.push(title.into());
    if let Some(desc) = description_element {
        items.push(desc);
    }
    items.push(Space::new().height(SPACER_SIZE_MEDIUM).into());
    items.push(details_heading.into());
    if let Some(ver) = version_row {
        items.push(ver);
    }
    if let Some(conn) = max_conn_row {
        items.push(conn);
    }
    items.push(Space::new().height(SPACER_SIZE_MEDIUM).into());
    items.push(buttons.into());

    let content = iced::widget::Column::with_children(items)
        .spacing(ELEMENT_SPACING)
        .padding(FORM_PADDING)
        .max_width(FORM_MAX_WIDTH);

    scrollable_panel(content)
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

    // General subheading
    let general_heading = shaped_text(t("label-general"))
        .size(SUBHEADING_SIZE)
        .style(subheading_text_style);
    form_items.push(general_heading.into());

    // Server name input
    let name_input = text_input(&t("placeholder-server-name"), &edit_state.name)
        .on_input(Message::EditServerInfoNameChanged)
        .on_submit(Message::UpdateServerInfoPressed)
        .id(Id::from(InputId::EditServerInfoName))
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE);
    form_items.push(name_input.into());

    // Server description input
    let desc_input = text_input(
        &t("placeholder-server-description"),
        &edit_state.description,
    )
    .on_input(Message::EditServerInfoDescriptionChanged)
    .on_submit(Message::UpdateServerInfoPressed)
    .id(Id::from(InputId::EditServerInfoDescription))
    .padding(INPUT_PADDING)
    .size(TEXT_SIZE);
    form_items.push(desc_input.into());

    form_items.push(Space::new().height(SPACER_SIZE_SMALL).into());

    // Image subheading
    let image_heading = shaped_text(t("label-image"))
        .size(SUBHEADING_SIZE)
        .style(subheading_text_style);
    form_items.push(image_heading.into());

    // Image buttons
    let pick_image_button = button(shaped_text(t("button-choose-image")).size(TEXT_SIZE))
        .on_press(Message::PickServerImagePressed)
        .padding(BUTTON_PADDING)
        .style(btn::secondary);

    let clear_image_button = if !edit_state.image.is_empty() {
        button(shaped_text(t("button-clear-image")).size(TEXT_SIZE))
            .on_press(Message::ClearServerImagePressed)
            .padding(BUTTON_PADDING)
            .style(btn::secondary)
    } else {
        button(shaped_text(t("button-clear-image")).size(TEXT_SIZE))
            .padding(BUTTON_PADDING)
            .style(btn::secondary)
    };

    let image_buttons = row![pick_image_button, clear_image_button].spacing(ELEMENT_SPACING);

    // Image row: preview (if exists) + buttons
    if let Some(cached) = &edit_state.cached_image {
        let image_preview: Element<'static, Message> = match cached {
            CachedImage::Raster(handle) => image(handle.clone())
                .width(SERVER_IMAGE_PREVIEW_SIZE)
                .height(SERVER_IMAGE_PREVIEW_SIZE)
                .content_fit(iced::ContentFit::ScaleDown)
                .into(),
            CachedImage::Svg(handle) => svg(handle.clone())
                .width(SERVER_IMAGE_PREVIEW_SIZE)
                .height(SERVER_IMAGE_PREVIEW_SIZE)
                .content_fit(iced::ContentFit::ScaleDown)
                .into(),
        };
        let image_row = row![image_preview, image_buttons]
            .spacing(ELEMENT_SPACING)
            .align_y(Center);
        form_items.push(image_row.into());
    } else {
        form_items.push(image_buttons.into());
    }

    form_items.push(Space::new().height(SPACER_SIZE_SMALL).into());

    // Limits subheading
    let limits_heading = shaped_text(t("label-limits"))
        .size(SUBHEADING_SIZE)
        .style(subheading_text_style);
    form_items.push(limits_heading.into());

    // Max connections per IP input using NumberInput
    let max_conn_label = shaped_text(t("label-max-connections-per-ip")).size(TEXT_SIZE);
    let current_value = edit_state.max_connections_per_ip.unwrap_or(1);
    let max_conn_input: Element<'static, Message> = NumberInput::new(
        &current_value,
        1..=u32::MAX,
        Message::EditServerInfoMaxConnectionsChanged,
    )
    .id(Id::from(InputId::EditServerInfoMaxConnections))
    .padding(INPUT_PADDING)
    .into();
    let max_conn_row = row![max_conn_label, max_conn_input]
        .spacing(ELEMENT_SPACING)
        .align_y(Center);
    form_items.push(max_conn_row.into());

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

    scrollable_panel(form)
}
