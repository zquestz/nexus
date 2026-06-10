//! Server info panel view (Config + Trackers tabs)
//!
//! Top-level structure:
//!  - Identity block (always visible): server image, name, description,
//!    shareable URI, fingerprint (click-to-copy).
//!  - Panel-level action error banner (between identity and tabs, only
//!    when set; mirrors User Management's panel-level banner).
//!  - Tab bar: `Config` | `Trackers` (Trackers gated on `tracker_list`).
//!  - Tab body:
//!    - Config tab: alphabetical two-column field list, no toolbar.
//!    - Trackers tab: per-tab toolbar (Add + Refresh) above a sortable
//!      table — the toolbar pattern only applies to tabs whose body is
//!      a table, so Config doesn't get one.
//!  - Bottom button row (Config tab only): `[Edit (admin)] [Close]` —
//!    matches the original pre-restructure panel; Trackers handles
//!    dismissal via clicking another panel (User Management pattern).
//!
//! Edit mode (admin-only) replaces the whole panel with the existing
//! single-column edit form.

use iced::widget::button as btn;
use iced::widget::{
    Column, Id, Space, button, column, container, image, pick_list, rich_text, row, span, svg,
    text_input,
};
use iced::{Center, Element, Fill, Length, Theme};
use iced_aw::{NumberInput, TabLabel, Tabs};
use nexus_common::validators::{
    DEFAULT_BANDWIDTH_CHUNK_SIZE, MAX_BANDWIDTH_CHUNK_SIZE, MIN_BANDWIDTH_CHUNK_SIZE,
    PasswordStrength,
};

use super::constants::PERMISSION_TRACKER_LIST;
use super::helpers::t_args;
use super::layout::scrollable_panel;
use super::password_strength::LocalizedPasswordStrength;
use super::trackers::trackers_tab_view;
use crate::i18n::{log_level_translation_key, strength_translation_key, t};
use crate::image::CachedImage;
use crate::style::{
    BUTTON_PADDING, CONTENT_MAX_WIDTH, CONTENT_PADDING, ELEMENT_SPACING, INPUT_PADDING,
    MONOSPACE_FONT, SERVER_IMAGE_PREVIEW_SIZE, SPACER_SIZE_LARGE, SPACER_SIZE_MEDIUM,
    SPACER_SIZE_SMALL, TAB_LABEL_PADDING, TEXT_SIZE, error_text_style, muted_text_style,
    panel_title, shaped_text, shaped_text_wrapped,
};
use crate::types::{
    InputId, Message, ServerConnection, ServerInfoEditState, ServerInfoTab, TrackerManagementState,
    format_bytes_per_sec_as_mbps,
};

/// Render the server info panel.
///
/// `conn` and `tracker_management` are borrowed separately so the caller
/// can pass live references without having to pre-build a `ServerInfoData`
/// that owns those borrows (which would have a hard time satisfying the
/// caller's lifetime).
pub fn server_info_view<'a>(
    conn: &'a ServerConnection,
    tracker_management: &'a TrackerManagementState,
    theme: &Theme,
) -> Element<'a, Message> {
    let edit_state = conn.server_info_edit.as_ref();

    if let Some(edit_state) = edit_state {
        return server_info_edit_view(edit_state);
    }

    // Tracker subview / modal dispatch (replaces the whole panel body,
    // mirroring User Management's full-panel form pattern).
    match &tracker_management.mode {
        crate::types::TrackerManagementMode::Add => {
            return super::trackers::add_tracker_view(tracker_management);
        }
        crate::types::TrackerManagementMode::Edit { .. } => {
            return super::trackers::edit_tracker_view(tracker_management);
        }
        crate::types::TrackerManagementMode::ConfirmRemove { .. } => {
            return super::trackers::remove_tracker_modal(tracker_management);
        }
        crate::types::TrackerManagementMode::AcceptFingerprint { .. } => {
            return super::trackers::accept_fingerprint_modal(tracker_management);
        }
        crate::types::TrackerManagementMode::List => {}
    }

    let data = build_display_data(conn);
    server_info_display_view(data, conn, tracker_management, theme)
}

/// Build the snapshot of display-only fields used by the Config tab.
fn build_display_data(conn: &ServerConnection) -> DisplayFields {
    DisplayFields {
        name: conn.server_name.clone(),
        description: conn.server_description.clone(),
        version: conn.server_version.clone(),
        max_connections_per_ip: conn.max_connections_per_ip,
        max_transfers_per_ip: conn.max_transfers_per_ip,
        file_reindex_interval: conn.file_reindex_interval,
        persistent_channels: conn.persistent_channels.clone(),
        auto_join_channels: conn.auto_join_channels.clone(),
        min_password_strength: Some(conn.min_password_strength),
        log_level: conn.log_level.clone(),
        chat_burst_limit: conn.chat_burst_limit,
        chat_rate_limit: conn.chat_rate_limit,
        fingerprint: Some(conn.connection_info.certificate_fingerprint.clone()),
        connection_address: conn.connection_info.address.clone(),
        connection_port: conn.connection_info.port,
        public_address: conn.public_address.clone(),
        max_outbound_rate: conn.max_outbound_rate,
        scheduler_chunk_size: conn.scheduler_chunk_size,
    }
}

/// Owned snapshot of the display-only fields. The cached server image is
/// passed as a borrow alongside this struct because `CachedImage` doesn't
/// derive `Clone`.
struct DisplayFields {
    name: Option<String>,
    description: Option<String>,
    version: Option<String>,
    max_connections_per_ip: Option<u32>,
    max_transfers_per_ip: Option<u32>,
    file_reindex_interval: Option<u32>,
    persistent_channels: Option<String>,
    auto_join_channels: Option<String>,
    min_password_strength: Option<PasswordStrength>,
    log_level: Option<String>,
    chat_burst_limit: Option<u32>,
    chat_rate_limit: Option<u32>,
    fingerprint: Option<String>,
    connection_address: String,
    connection_port: u16,
    public_address: Option<String>,
    max_outbound_rate: Option<u64>,
    scheduler_chunk_size: Option<u32>,
}

/// Render the server info display view (Config + Trackers tabs).
fn server_info_display_view<'a>(
    data: DisplayFields,
    conn: &'a ServerConnection,
    tracker_management: &'a TrackerManagementState,
    theme: &Theme,
) -> Element<'a, Message> {
    let mut items: Vec<Element<'a, Message>> = Vec::new();

    // Server image at the top (if set)
    if let Some(cached_image) = conn.cached_server_image.as_ref() {
        let image_element: Element<'a, Message> = match cached_image {
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

    // Shareable nexus:// URI — styled like a chat link; click copies to clipboard.
    let share_uri = crate::uri::build_share_uri(
        data.public_address.as_deref(),
        &data.connection_address,
        data.connection_port,
    );
    let share_link = rich_text![
        span(share_uri.clone())
            .color(theme.palette().primary)
            .link(share_uri)
    ]
    .on_link_click(Message::CopyServerUri)
    .size(TEXT_SIZE)
    .align_x(Center);
    items.push(
        row![
            Space::new().width(Fill),
            share_link,
            Space::new().width(Fill)
        ]
        .into(),
    );

    // Fingerprint moved to identity block: label + hex on a single
    // centered, glyph-wrapped rich_text. Glyph wrapping (rather than
    // WordOrGlyph) lets the colon-separated hex break at any character
    // when it hits the right edge — colons aren't word boundaries so
    // anything word-aware would treat the whole hex as one
    // unbreakable token. The hex value is a link so clicking copies
    // it to the clipboard.
    if let Some(fingerprint) = &data.fingerprint {
        let fp_copy_value = fingerprint.clone();
        let label_part = format!("{} ", t("label-fingerprint"));
        let fingerprint_link = rich_text![
            span(label_part),
            span(fingerprint.clone())
                .color(theme.palette().primary)
                .font(MONOSPACE_FONT)
                .link(fp_copy_value),
        ]
        .on_link_click(Message::CopyServerFingerprint)
        .size(TEXT_SIZE)
        .width(Fill)
        .align_x(Center)
        .wrapping(iced::widget::text::Wrapping::Glyph);
        items.push(fingerprint_link.into());
    }

    // Panel-level action error banner (between identity and tabs).
    if let Some(err) = &tracker_management.list_error {
        items.push(
            shaped_text_wrapped(err.clone())
                .size(TEXT_SIZE)
                .width(Fill)
                .align_x(Center)
                .style(error_text_style)
                .into(),
        );
    }

    items.push(Space::new().height(SPACER_SIZE_MEDIUM).into());

    // Config tab content: just the body (no per-tab toolbar). The
    // Edit / Close affordances live in a bottom button row below the
    // Tabs widget — toolbar-style icons looked out of place above a
    // pure label/value list, and the original pre-restructure panel
    // had bottom buttons too. Trackers tab keeps its toolbar because
    // its body is a sortable table where the toolbar pattern fits.
    let config_body = config_tab_body(&data);
    let config_content: Element<'a, Message> =
        column![Space::new().height(SPACER_SIZE_MEDIUM), config_body,]
            .height(Fill)
            .into();

    // Build the Trackers tab content
    let has_tracker_list_perm = conn.has_permission(PERMISSION_TRACKER_LIST);

    let mut tabs: Tabs<'a, Message, ServerInfoTab> = Tabs::new(Message::ServerInfoTabChanged).push(
        ServerInfoTab::Config,
        TabLabel::Text(t("tab-config")),
        config_content,
    );

    // Trackers is an operator/admin subsection inside the otherwise
    // user-facing Server Info panel. Hide the tab without tracker_list instead
    // of showing a disabled/empty tab to most users.
    if has_tracker_list_perm {
        let trackers_tab = trackers_tab_view(conn, tracker_management, theme);
        let trackers_content: Element<'a, Message> =
            column![Space::new().height(SPACER_SIZE_MEDIUM), trackers_tab,]
                .height(Fill)
                .into();
        tabs = tabs.push(
            ServerInfoTab::Trackers,
            TabLabel::Text(t("tab-trackers")),
            trackers_content,
        );
    }

    // If user lacks tracker_list and currently has Trackers selected
    // (e.g., the permission was just revoked), fall back to Config so
    // the tab bar doesn't render with an inactive selected state.
    let active_tab = if conn.server_info_tab == ServerInfoTab::Trackers && !has_tracker_list_perm {
        ServerInfoTab::Config
    } else {
        conn.server_info_tab
    };

    let tabs = tabs
        .set_active_tab(&active_tab)
        .tab_bar_position(iced_aw::TabBarPosition::Top)
        .text_size(TEXT_SIZE)
        .tab_label_padding(TAB_LABEL_PADDING);

    items.push(container(tabs).height(Fill).into());

    // Bottom button row — only on the Config tab (Trackers handles
    // dismissal via clicking another panel, like User Management does;
    // its toolbar covers the in-tab actions). Edit is admin-gated.
    //
    // The leading spacer matches the breathing room the original
    // pre-restructure panel had between the field list and the
    // buttons; without it the buttons crowd the last config row.
    if active_tab == ServerInfoTab::Config {
        items.push(Space::new().height(SPACER_SIZE_MEDIUM).into());
        let mut buttons = row![Space::new().width(Fill)].spacing(ELEMENT_SPACING);
        if conn.is_admin {
            buttons = buttons.push(
                button(shaped_text(t("button-edit")).size(TEXT_SIZE))
                    .on_press(Message::EditServerInfoPressed)
                    .padding(BUTTON_PADDING)
                    .style(btn::secondary),
            );
        }
        buttons = buttons.push(
            button(shaped_text(t("button-close")).size(TEXT_SIZE))
                .on_press(Message::CloseServerInfo)
                .padding(BUTTON_PADDING),
        );
        items.push(buttons.into());
    }

    // Outer wrap: symmetric `CONTENT_PADDING` + `CONTENT_MAX_WIDTH`,
    // whole panel scrolls as one unit. The 20px right padding
    // provides a stable gutter wide enough for the scrollbar without
    // overlapping content. Matches `views/settings/mod.rs` — both
    // are header-less panels where the entire content scrolls.
    let content = Column::with_children(items)
        .spacing(ELEMENT_SPACING)
        .padding(CONTENT_PADDING)
        .max_width(CONTENT_MAX_WIDTH)
        .height(Fill);

    // Wrap in scrollable so the identity block + tabs scroll together
    // when the window is shorter than the content (mirrors the original
    // pre-tab-restructure layout's scrollable_panel behavior).
    scrollable_panel(content)
}

/// Build the Config tab body — three permission-gated sections.
///
/// Sections (`General`, `Chat`, `Files`) mirror the structure of the
/// edit view, so admins see the same grouping in display and edit
/// modes. Each section renders its heading and rows only when at
/// least one of its fields is visible to the requesting user; an
/// entirely-hidden section disappears from the layout.
///
/// Within a section, rows are alphabetical.
fn config_tab_body(data: &DisplayFields) -> Element<'static, Message> {
    let mut general_rows: Vec<Element<'static, Message>> = Vec::new();
    let mut bandwidth_rows: Vec<Element<'static, Message>> = Vec::new();
    let mut chat_rows: Vec<Element<'static, Message>> = Vec::new();
    let mut files_rows: Vec<Element<'static, Message>> = Vec::new();

    // -- General (alphabetical) --
    if let Some(level) = &data.log_level {
        general_rows.push(config_row(
            t("label-log-level"),
            t(log_level_translation_key(level)),
        ));
    }
    if let Some(max_conn) = data.max_connections_per_ip {
        general_rows.push(config_row(
            t("label-max-connections-per-ip"),
            max_conn.to_string(),
        ));
    }
    if let Some(max_xfer) = data.max_transfers_per_ip {
        general_rows.push(config_row(
            t("label-max-transfers-per-ip"),
            max_xfer.to_string(),
        ));
    }
    if let Some(strength) = data.min_password_strength {
        general_rows.push(config_row(
            t("label-min-password-strength"),
            t(strength_translation_key(strength)),
        ));
    }
    if let Some(version) = &data.version {
        general_rows.push(config_row(t("label-version-short"), version.clone()));
    }

    // -- Bandwidth (alphabetical within the section) --
    // `max_outbound_rate` is visible to all (like chat_rate_limit);
    // `scheduler_chunk_size` is admin-only and the server already sends
    // `None` for non-admins, so the if-let naturally hides it.
    if let Some(bytes_per_sec) = data.max_outbound_rate {
        let value = if bytes_per_sec == 0 {
            t("label-unlimited")
        } else {
            format_bytes_per_sec_as_mbps(bytes_per_sec)
        };
        bandwidth_rows.push(config_row(t("label-max-outbound-rate"), value));
    }
    if let Some(chunk) = data.scheduler_chunk_size {
        bandwidth_rows.push(config_row(
            t("label-scheduler-chunk-size"),
            chunk.to_string(),
        ));
    }

    // -- Chat (alphabetical) --
    if let Some(channels) = &data.auto_join_channels {
        let value = if channels.is_empty() {
            t("label-none")
        } else {
            channels.clone()
        };
        chat_rows.push(config_row(t("label-auto-join-channels"), value));
    }
    if let Some(burst) = data.chat_burst_limit {
        chat_rows.push(config_row(t("label-chat-burst-limit"), burst.to_string()));
    }
    if let Some(rate) = data.chat_rate_limit {
        chat_rows.push(config_row(t("label-chat-rate-limit"), rate.to_string()));
    }
    if let Some(channels) = &data.persistent_channels {
        let value = if channels.is_empty() {
            t("label-none")
        } else {
            channels.clone()
        };
        chat_rows.push(config_row(t("label-persistent-channels"), value));
    }

    // -- Files --
    if let Some(interval) = data.file_reindex_interval {
        let value = if interval == 0 {
            t("label-disabled")
        } else {
            t_args(
                "label-file-reindex-interval-value",
                &[("minutes", &interval.to_string())],
            )
        };
        files_rows.push(config_row(t("label-file-reindex-interval"), value));
    }

    // Compose. Sections that contributed no rows are skipped entirely
    // (heading included). Every emitted section gets a leading
    // SPACER_SIZE_MEDIUM — the first section's spacer compounds with
    // the outer wrapper's spacer to give the topmost heading the
    // breathing room it needs from the tab bar's hard bottom edge,
    // while between sections a single MEDIUM gap is enough.
    let sections: [(&str, Vec<Element<'static, Message>>); 4] = [
        ("label-general-section", general_rows),
        ("label-bandwidth-section", bandwidth_rows),
        ("label-chat-section", chat_rows),
        ("label-files-section", files_rows),
    ];
    let mut items: Vec<Element<'static, Message>> = Vec::new();
    for (heading_key, rows) in sections {
        if rows.is_empty() {
            continue;
        }
        items.push(Space::new().height(SPACER_SIZE_MEDIUM).into());
        items.push(
            shaped_text(t(heading_key))
                .size(TEXT_SIZE)
                .style(muted_text_style)
                .into(),
        );
        items.extend(rows);
    }

    Column::with_children(items)
        .spacing(ELEMENT_SPACING)
        .width(Fill)
        .into()
}

/// Build a single Config-tab row with label + value.
///
/// `width(Fill)` on the row + `width(Fill)` on the wrapped value makes
/// the value take the remainder of the row's allotted width and wrap
/// at word boundaries when the column is too narrow to fit on one
/// line. Without these, the row sizes to its intrinsic content and
/// overflows the column when the window is narrow.
fn config_row<'a>(label: String, value: String) -> Element<'a, Message> {
    row![
        shaped_text(label).size(TEXT_SIZE),
        Space::new().width(ELEMENT_SPACING),
        shaped_text_wrapped(value).size(TEXT_SIZE).width(Fill),
    ]
    .width(Fill)
    .into()
}

/// Render the server info edit view (editable form)
fn server_info_edit_view(edit_state: &ServerInfoEditState) -> Element<'static, Message> {
    let title = panel_title(t("title-server-info-edit"));
    let can_save = edit_state.has_changes_from_original() && !edit_state.is_submitting;

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
        .id(Id::from(InputId::EditServerInfoName))
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE)
        .width(Fill);
    let name_input = if can_save {
        name_input.on_submit(Message::UpdateServerInfoPressed)
    } else {
        name_input
    };
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
    .id(Id::from(InputId::EditServerInfoDescription))
    .padding(INPUT_PADDING)
    .size(TEXT_SIZE)
    .width(Fill);
    let desc_input = if can_save {
        desc_input.on_submit(Message::UpdateServerInfoPressed)
    } else {
        desc_input
    };
    form_items.push(
        row![desc_label, Space::new().width(ELEMENT_SPACING), desc_input]
            .align_y(Center)
            .into(),
    );

    // Public address input with inline label
    let public_address_label = shaped_text(t("label-public-address")).size(TEXT_SIZE);
    let public_address_input =
        text_input(&t("placeholder-public-address"), &edit_state.public_address)
            .on_input(Message::EditServerInfoPublicAddressChanged)
            .id(Id::from(InputId::EditServerInfoPublicAddress))
            .padding(INPUT_PADDING)
            .size(TEXT_SIZE)
            .width(Fill);
    let public_address_input = if can_save {
        public_address_input.on_submit(Message::UpdateServerInfoPressed)
    } else {
        public_address_input
    };
    form_items.push(
        row![
            public_address_label,
            Space::new().width(ELEMENT_SPACING),
            public_address_input
        ]
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

    // General subheading
    form_items.push(
        shaped_text(t("label-general-section"))
            .size(TEXT_SIZE)
            .style(muted_text_style)
            .into(),
    );

    // Connections and Transfers side-by-side
    let conn_label = shaped_text(t("label-max-connections-per-ip")).size(TEXT_SIZE);
    let max_conn_value = edit_state.max_connections_per_ip.unwrap_or(1);
    let max_conn_input: Element<'static, Message> = NumberInput::new(
        &max_conn_value,
        0..=u32::MAX,
        Message::EditServerInfoMaxConnectionsChanged,
    )
    .id(Id::from(InputId::EditServerInfoMaxConnections))
    .padding(INPUT_PADDING)
    .into();

    let xfer_label = shaped_text(t("label-max-transfers-per-ip")).size(TEXT_SIZE);
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

    // Min password strength picker
    let strength_label = shaped_text(t("label-min-password-strength")).size(TEXT_SIZE);
    let strength_picker: Element<'static, Message> = pick_list(
        LocalizedPasswordStrength::all(),
        Some(LocalizedPasswordStrength::from(
            edit_state.min_password_strength,
        )),
        |ls| Message::EditServerInfoMinPasswordStrengthChanged(ls.into()),
    )
    .text_size(TEXT_SIZE)
    .into();

    form_items.push(
        row![
            strength_label,
            Space::new().width(ELEMENT_SPACING),
            strength_picker
        ]
        .align_y(Center)
        .into(),
    );

    form_items.push(Space::new().height(SPACER_SIZE_MEDIUM).into());

    // Bandwidth subheading + section
    form_items.push(
        shaped_text(t("label-bandwidth-section"))
            .size(TEXT_SIZE)
            .style(muted_text_style)
            .into(),
    );

    // Max outbound rate (Mbps, NumberInput<f64>; stepper of 1.0, users can
    // type fractional values directly. 0.0 means unlimited.)
    let outbound_label = shaped_text(t("label-max-outbound-rate")).size(TEXT_SIZE);
    let outbound_input: Element<'static, Message> = NumberInput::new(
        &edit_state.max_outbound_rate_mbps,
        0.0..=f64::MAX,
        Message::EditServerInfoMaxOutboundRateChanged,
    )
    .id(Id::from(InputId::EditServerInfoMaxOutboundRate))
    .padding(INPUT_PADDING)
    .into();
    form_items.push(
        row![
            outbound_label,
            Space::new().width(ELEMENT_SPACING),
            outbound_input
        ]
        .align_y(Center)
        .into(),
    );

    // Scheduler chunk size (bytes, NumberInput; bounds from validators)
    let chunk_label = shaped_text(t("label-scheduler-chunk-size")).size(TEXT_SIZE);
    let chunk_value = edit_state
        .scheduler_chunk_size
        .unwrap_or(DEFAULT_BANDWIDTH_CHUNK_SIZE);
    let chunk_input: Element<'static, Message> = NumberInput::new(
        &chunk_value,
        MIN_BANDWIDTH_CHUNK_SIZE..=MAX_BANDWIDTH_CHUNK_SIZE,
        |v| Message::EditServerInfoSchedulerChunkSizeChanged(Some(v)),
    )
    .id(Id::from(InputId::EditServerInfoSchedulerChunkSize))
    .padding(INPUT_PADDING)
    .into();
    form_items.push(
        row![
            chunk_label,
            Space::new().width(ELEMENT_SPACING),
            chunk_input
        ]
        .align_y(Center)
        .into(),
    );

    form_items.push(Space::new().height(SPACER_SIZE_MEDIUM).into());

    // Chat subheading
    form_items.push(
        shaped_text(t("label-chat-section"))
            .size(TEXT_SIZE)
            .style(muted_text_style)
            .into(),
    );

    // Alphabetical: Auto-join Channels, Chat Burst Limit, Chat Rate Limit, Persistent Channels

    // Auto-join channels input with inline label
    let auto_join_label = shaped_text(t("label-auto-join-channels")).size(TEXT_SIZE);
    let auto_join_input = text_input(
        &t("placeholder-auto-join-channels"),
        &edit_state.auto_join_channels,
    )
    .on_input(Message::EditServerInfoAutoJoinChannelsChanged)
    .id(Id::from(InputId::EditServerInfoAutoJoinChannels))
    .padding(INPUT_PADDING)
    .size(TEXT_SIZE)
    .width(Fill);
    let auto_join_input = if can_save {
        auto_join_input.on_submit(Message::UpdateServerInfoPressed)
    } else {
        auto_join_input
    };
    form_items.push(
        row![
            auto_join_label,
            Space::new().width(ELEMENT_SPACING),
            auto_join_input
        ]
        .align_y(Center)
        .into(),
    );

    // Chat burst limit
    let burst_label = shaped_text(t("label-chat-burst-limit")).size(TEXT_SIZE);
    let burst_input: Element<'static, Message> = NumberInput::new(
        &edit_state.chat_burst_limit,
        0..=u32::MAX,
        Message::EditServerInfoChatBurstLimitChanged,
    )
    .padding(INPUT_PADDING)
    .into();
    form_items.push(
        row![
            burst_label,
            Space::new().width(ELEMENT_SPACING),
            burst_input
        ]
        .align_y(Center)
        .into(),
    );

    // Chat rate limit
    let rate_label = shaped_text(t("label-chat-rate-limit")).size(TEXT_SIZE);
    let rate_input: Element<'static, Message> = NumberInput::new(
        &edit_state.chat_rate_limit,
        0..=u32::MAX,
        Message::EditServerInfoChatRateLimitChanged,
    )
    .padding(INPUT_PADDING)
    .into();
    form_items.push(
        row![rate_label, Space::new().width(ELEMENT_SPACING), rate_input]
            .align_y(Center)
            .into(),
    );

    // Persistent channels input with inline label
    let persistent_label = shaped_text(t("label-persistent-channels")).size(TEXT_SIZE);
    let persistent_input = text_input(
        &t("placeholder-persistent-channels"),
        &edit_state.persistent_channels,
    )
    .on_input(Message::EditServerInfoPersistentChannelsChanged)
    .id(Id::from(InputId::EditServerInfoPersistentChannels))
    .padding(INPUT_PADDING)
    .size(TEXT_SIZE)
    .width(Fill);
    let persistent_input = if can_save {
        persistent_input.on_submit(Message::UpdateServerInfoPressed)
    } else {
        persistent_input
    };
    form_items.push(
        row![
            persistent_label,
            Space::new().width(ELEMENT_SPACING),
            persistent_input
        ]
        .align_y(Center)
        .into(),
    );

    form_items.push(Space::new().height(SPACER_SIZE_MEDIUM).into());

    // Files subheading
    form_items.push(
        shaped_text(t("label-files-section"))
            .size(TEXT_SIZE)
            .style(muted_text_style)
            .into(),
    );

    // Reindex with "minutes" suffix inline
    let reindex_label = shaped_text(t("label-file-reindex-interval")).size(TEXT_SIZE);
    let reindex_value = edit_state.file_reindex_interval.unwrap_or(5);
    let reindex_input: Element<'static, Message> = NumberInput::new(
        &reindex_value,
        0..=255u32,
        Message::EditServerInfoFileReindexIntervalChanged,
    )
    .id(Id::from(InputId::EditServerInfoFileReindexInterval))
    .padding(INPUT_PADDING)
    .into();
    let minutes_suffix = shaped_text(t_args(
        "label-minutes",
        &[("count", &reindex_value.to_string())],
    ))
    .size(TEXT_SIZE);

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

    // Buttons: Cancel (secondary) and Save (primary)
    let buttons = row![
        Space::new().width(Fill),
        button(shaped_text(t("button-cancel")).size(TEXT_SIZE))
            .on_press(Message::CancelEditServerInfo)
            .padding(BUTTON_PADDING)
            .style(btn::secondary),
        button(shaped_text(t("button-save")).size(TEXT_SIZE))
            .on_press_maybe(can_save.then_some(Message::UpdateServerInfoPressed))
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
