//! Server info panel view

use iced::widget::button as btn;
use iced::widget::{Id, Space, button, container, image, row, svg, text_input};
use iced::{Center, Element, Fill, Length};
use iced_aw::{NumberInput, TabLabel, Tabs};

use super::layout::scrollable_panel;
use crate::i18n::{t, t_args};
use crate::image::CachedImage;
use crate::style::{
    BUTTON_PADDING, CONTENT_MAX_WIDTH, CONTENT_PADDING, ELEMENT_SPACING, INPUT_PADDING,
    SERVER_IMAGE_PREVIEW_SIZE, SPACER_SIZE_LARGE, SPACER_SIZE_MEDIUM, SPACER_SIZE_SMALL,
    TAB_LABEL_PADDING, TEXT_SIZE, error_text_style, muted_text_style, panel_title, shaped_text,
    shaped_text_wrapped,
};
use crate::types::{InputId, Message, ServerInfoEditState, ServerInfoTab};

/// Data needed to render the server info panel
pub struct ServerInfoData<'a> {
    /// Server name (if provided)
    pub name: Option<String>,
    /// Server description (if provided)
    pub description: Option<String>,
    /// Server version (if provided)
    pub version: Option<String>,
    /// Max connections per IP (all users)
    pub max_connections_per_ip: Option<u32>,
    /// Max transfers per IP (all users)
    pub max_transfers_per_ip: Option<u32>,
    /// File reindex interval in minutes (admins + file_reindex permission, 0 = disabled)
    pub file_reindex_interval: Option<u32>,
    /// Persistent channels (space-separated, admin only)
    pub persistent_channels: Option<String>,
    /// Auto-join channels (space-separated, users with chat_join permission + admins)
    pub auto_join_channels: Option<String>,
    /// Cached server image for display (None if no image set)
    pub cached_server_image: Option<&'a CachedImage>,
    /// Whether the current user is an admin
    pub is_admin: bool,
    /// Active tab in display mode (shown based on available data)
    pub active_tab: ServerInfoTab,
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
    let mut items: Vec<Element<'static, Message>> = Vec::new();

    // Server image at the top (if set)
    if let Some(cached_image) = data.cached_server_image {
        let image_element: Element<'static, Message> = match cached_image {
            CachedImage::Raster(handle) => image(handle.clone())
                .width(Length::Fill)
                .content_fit(iced::ContentFit::ScaleDown)
                .into(),
            CachedImage::Svg(handle) => svg(handle.clone())
                .width(Length::Fill)
                .content_fit(iced::ContentFit::ScaleDown)
                .into(),
        };
        items.push(image_element);
    }

    // Server name as the title (fallback to generic title if no name)
    let title_text = data.name.clone().unwrap_or_else(|| t("title-server-info"));
    items.push(panel_title(title_text).into());

    // Server description directly under title (no label)
    if let Some(description) = data.description.as_ref().filter(|d| !d.is_empty()) {
        items.push(
            shaped_text_wrapped(description.clone())
                .size(TEXT_SIZE)
                .width(Fill)
                .align_x(Center)
                .into(),
        );
    }

    // Server version (centered like name and description)
    if let Some(version) = &data.version {
        items.push(
            container(
                shaped_text(t_args("label-version-value", &[("version", version)])).size(TEXT_SIZE),
            )
            .width(Fill)
            .align_x(Center)
            .into(),
        );
    }

    // Determine which tabs to show based on available data
    let has_limits = data.max_connections_per_ip.is_some() || data.max_transfers_per_ip.is_some();
    let has_files = data.file_reindex_interval.is_some();
    let has_channels = data.persistent_channels.is_some() || data.auto_join_channels.is_some();

    // Show settings tabs if user has any settings data
    if has_limits || has_files || has_channels {
        items.push(Space::new().height(SPACER_SIZE_MEDIUM).into());

        // Build tab content for each available tab
        let limits_content: Element<'static, Message> = {
            let mut content_items: Vec<Element<'static, Message>> = Vec::new();
            // Space between tab bar and first content
            content_items.push(Space::new().height(SPACER_SIZE_MEDIUM).into());
            if let Some(max_conn) = data.max_connections_per_ip {
                content_items.push(
                    row![
                        shaped_text(t("label-connections-short")).size(TEXT_SIZE),
                        Space::new().width(ELEMENT_SPACING),
                        shaped_text(max_conn.to_string()).size(TEXT_SIZE),
                    ]
                    .align_y(Center)
                    .into(),
                );
            }
            if let Some(max_xfer) = data.max_transfers_per_ip {
                content_items.push(
                    row![
                        shaped_text(t("label-transfers-short")).size(TEXT_SIZE),
                        Space::new().width(ELEMENT_SPACING),
                        shaped_text(max_xfer.to_string()).size(TEXT_SIZE),
                    ]
                    .align_y(Center)
                    .into(),
                );
            }
            iced::widget::Column::with_children(content_items)
                .spacing(ELEMENT_SPACING)
                .into()
        };

        let files_content: Element<'static, Message> = {
            let mut content_items: Vec<Element<'static, Message>> = Vec::new();
            // Space between tab bar and first content
            content_items.push(Space::new().height(SPACER_SIZE_MEDIUM).into());
            if let Some(interval) = data.file_reindex_interval {
                let value = if interval == 0 {
                    t("label-disabled")
                } else {
                    t_args(
                        "label-file-reindex-interval-value",
                        &[("minutes", &interval.to_string())],
                    )
                };
                content_items.push(
                    row![
                        shaped_text(t("label-reindex-short")).size(TEXT_SIZE),
                        Space::new().width(ELEMENT_SPACING),
                        shaped_text(value).size(TEXT_SIZE),
                    ]
                    .align_y(Center)
                    .into(),
                );
            }
            iced::widget::Column::with_children(content_items)
                .spacing(ELEMENT_SPACING)
                .into()
        };

        let channels_content: Element<'static, Message> = {
            let mut content_items: Vec<Element<'static, Message>> = Vec::new();
            // Space between tab bar and first content
            content_items.push(Space::new().height(SPACER_SIZE_MEDIUM).into());
            if let Some(channels) = &data.persistent_channels {
                let value = if channels.is_empty() {
                    t("label-none")
                } else {
                    channels.clone()
                };
                content_items.push(
                    row![
                        shaped_text(t("label-persistent-short")).size(TEXT_SIZE),
                        Space::new().width(ELEMENT_SPACING),
                        shaped_text(value).size(TEXT_SIZE),
                    ]
                    .align_y(Center)
                    .into(),
                );
            }
            if let Some(channels) = &data.auto_join_channels {
                let value = if channels.is_empty() {
                    t("label-none")
                } else {
                    channels.clone()
                };
                content_items.push(
                    row![
                        shaped_text(t("label-auto-join-short")).size(TEXT_SIZE),
                        Space::new().width(ELEMENT_SPACING),
                        shaped_text(value).size(TEXT_SIZE),
                    ]
                    .align_y(Center)
                    .into(),
                );
            }
            iced::widget::Column::with_children(content_items)
                .spacing(ELEMENT_SPACING)
                .into()
        };

        // Build tabs widget (only including tabs for available data)
        let mut tabs: Tabs<'static, Message, ServerInfoTab> =
            Tabs::new(Message::ServerInfoTabChanged);

        if has_limits {
            tabs = tabs.push(
                ServerInfoTab::Limits,
                TabLabel::Text(t("tab-limits")),
                limits_content,
            );
        }

        if has_files {
            tabs = tabs.push(
                ServerInfoTab::Files,
                TabLabel::Text(t("tab-files")),
                files_content,
            );
        }

        if has_channels {
            tabs = tabs.push(
                ServerInfoTab::Channels,
                TabLabel::Text(t("tab-channels")),
                channels_content,
            );
        }

        let tabs = tabs
            .set_active_tab(&data.active_tab)
            .tab_bar_position(iced_aw::TabBarPosition::Top)
            .text_size(TEXT_SIZE)
            .tab_label_padding(TAB_LABEL_PADDING);

        items.push(tabs.into());
    }

    items.push(Space::new().height(SPACER_SIZE_MEDIUM).into());

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

    items.push(buttons.into());

    let content = iced::widget::Column::with_children(items)
        .spacing(ELEMENT_SPACING)
        .padding(CONTENT_PADDING)
        .max_width(CONTENT_MAX_WIDTH);

    scrollable_panel(content)
}

/// Render the server info edit view (editable form)
fn server_info_edit_view(edit_state: &ServerInfoEditState) -> Element<'static, Message> {
    let title = panel_title(t("title-edit-server-info"));

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

    // Server name input with inline label
    let name_label = shaped_text(t("label-name")).size(TEXT_SIZE);
    let name_input = text_input(&t("placeholder-server-name"), &edit_state.name)
        .on_input(Message::EditServerInfoNameChanged)
        .on_submit(Message::UpdateServerInfoPressed)
        .id(Id::from(InputId::EditServerInfoName))
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE)
        .width(Fill);
    form_items.push(
        row![name_label, Space::new().width(ELEMENT_SPACING), name_input]
            .align_y(Center)
            .into(),
    );

    // Server description input with inline label
    let desc_label = shaped_text(t("label-description")).size(TEXT_SIZE);
    let desc_input = text_input(
        &t("placeholder-server-description"),
        &edit_state.description,
    )
    .on_input(Message::EditServerInfoDescriptionChanged)
    .on_submit(Message::UpdateServerInfoPressed)
    .id(Id::from(InputId::EditServerInfoDescription))
    .padding(INPUT_PADDING)
    .size(TEXT_SIZE)
    .width(Fill);
    form_items.push(
        row![desc_label, Space::new().width(ELEMENT_SPACING), desc_input]
            .align_y(Center)
            .into(),
    );

    // Image row with inline label
    let image_label = shaped_text(t("label-image")).size(TEXT_SIZE);

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
        form_items.push(
            row![
                image_label,
                Space::new().width(ELEMENT_SPACING),
                image_preview,
                Space::new().width(ELEMENT_SPACING),
                pick_image_button,
                clear_image_button
            ]
            .spacing(ELEMENT_SPACING)
            .align_y(Center)
            .into(),
        );
    } else {
        form_items.push(
            row![
                image_label,
                Space::new().width(ELEMENT_SPACING),
                pick_image_button,
                clear_image_button
            ]
            .spacing(ELEMENT_SPACING)
            .align_y(Center)
            .into(),
        );
    }

    form_items.push(Space::new().height(SPACER_SIZE_MEDIUM).into());

    // Limits subheading
    form_items.push(
        shaped_text(t("tab-limits"))
            .size(TEXT_SIZE)
            .style(muted_text_style)
            .into(),
    );

    // Connections and Transfers side-by-side
    let conn_label = shaped_text(t("label-connections-short")).size(TEXT_SIZE);
    let max_conn_value = edit_state.max_connections_per_ip.unwrap_or(1);
    let max_conn_input: Element<'static, Message> = NumberInput::new(
        &max_conn_value,
        0..=u32::MAX,
        Message::EditServerInfoMaxConnectionsChanged,
    )
    .id(Id::from(InputId::EditServerInfoMaxConnections))
    .padding(INPUT_PADDING)
    .into();

    let xfer_label = shaped_text(t("label-transfers-short")).size(TEXT_SIZE);
    let max_xfer_value = edit_state.max_transfers_per_ip.unwrap_or(1);
    let max_xfer_input: Element<'static, Message> = NumberInput::new(
        &max_xfer_value,
        0..=u32::MAX,
        Message::EditServerInfoMaxTransfersChanged,
    )
    .id(Id::from(InputId::EditServerInfoMaxTransfers))
    .padding(INPUT_PADDING)
    .into();

    form_items.push(
        row![
            conn_label,
            Space::new().width(ELEMENT_SPACING),
            max_conn_input,
            Space::new().width(SPACER_SIZE_LARGE),
            xfer_label,
            Space::new().width(ELEMENT_SPACING),
            max_xfer_input,
        ]
        .align_y(Center)
        .into(),
    );

    form_items.push(Space::new().height(SPACER_SIZE_MEDIUM).into());

    // Files subheading
    form_items.push(
        shaped_text(t("tab-files"))
            .size(TEXT_SIZE)
            .style(muted_text_style)
            .into(),
    );

    // Reindex with "minutes" suffix inline
    let reindex_label = shaped_text(t("label-reindex-short")).size(TEXT_SIZE);
    let reindex_value = edit_state.file_reindex_interval.unwrap_or(5);
    let reindex_input: Element<'static, Message> = NumberInput::new(
        &reindex_value,
        0..=255u32,
        Message::EditServerInfoFileReindexIntervalChanged,
    )
    .id(Id::from(InputId::EditServerInfoFileReindexInterval))
    .padding(INPUT_PADDING)
    .into();
    let minutes_suffix = shaped_text(t("label-minutes")).size(TEXT_SIZE);

    form_items.push(
        row![
            reindex_label,
            Space::new().width(ELEMENT_SPACING),
            reindex_input,
            Space::new().width(ELEMENT_SPACING),
            minutes_suffix,
        ]
        .align_y(Center)
        .into(),
    );

    form_items.push(Space::new().height(SPACER_SIZE_MEDIUM).into());

    // Channels subheading
    form_items.push(
        shaped_text(t("tab-channels"))
            .size(TEXT_SIZE)
            .style(muted_text_style)
            .into(),
    );

    // Persistent channels input with inline label
    let persistent_label = shaped_text(t("label-persistent-short")).size(TEXT_SIZE);
    let persistent_input = text_input(
        &t("placeholder-persistent-channels"),
        &edit_state.persistent_channels,
    )
    .on_input(Message::EditServerInfoPersistentChannelsChanged)
    .on_submit(Message::UpdateServerInfoPressed)
    .padding(INPUT_PADDING)
    .size(TEXT_SIZE)
    .width(Fill);
    form_items.push(
        row![
            persistent_label,
            Space::new().width(ELEMENT_SPACING),
            persistent_input
        ]
        .align_y(Center)
        .into(),
    );

    // Auto-join channels input with inline label
    let auto_join_label = shaped_text(t("label-auto-join-short")).size(TEXT_SIZE);
    let auto_join_input = text_input(
        &t("placeholder-auto-join-channels"),
        &edit_state.auto_join_channels,
    )
    .on_input(Message::EditServerInfoAutoJoinChannelsChanged)
    .on_submit(Message::UpdateServerInfoPressed)
    .padding(INPUT_PADDING)
    .size(TEXT_SIZE)
    .width(Fill);
    form_items.push(
        row![
            auto_join_label,
            Space::new().width(ELEMENT_SPACING),
            auto_join_input
        ]
        .align_y(Center)
        .into(),
    );

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
        .padding(CONTENT_PADDING)
        .max_width(CONTENT_MAX_WIDTH);

    scrollable_panel(form)
}
