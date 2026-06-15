//! Tracker management views (Trackers tab inside the Server Info panel).
//!
//! Mirrors the User / Group management view structure: a sortable table
//! with `lazy()` caching and per-row context menu, full-panel Add and
//! Edit subviews, a Remove confirmation modal, and an Accept Fingerprint
//! modal. The Accept modal closely mirrors `views/fingerprint.rs`.

use std::hash::{Hash, Hasher};

use iced::widget::button as btn;
use iced::widget::text::Wrapping;
use iced::widget::{
    Column, Id, Space, button, checkbox, column, container, lazy, row, scrollable, table, text,
    text_input, tooltip,
};
use iced::{Center, Element, Fill, Length, Theme};
use iced_aw::{ContextMenu, NumberInput};
use nexus_common::names::fold_name;
use nexus_common::protocol::TrackerInfo;
use nexus_common::{DEFAULT_TRACKER_PORT, ERROR_KIND_TRACKER_FINGERPRINT_INTERCEPTED};

use super::constants::{
    PERMISSION_TRACKER_ADD, PERMISSION_TRACKER_EDIT, PERMISSION_TRACKER_REMOVE,
};
use super::fingerprint::format_fingerprint_multiline;
use super::helpers::{sort_icon_or_placeholder, tab_toolbar_icon_button};
use super::layout::{scrollable_modal, scrollable_panel};
use crate::i18n::t;
use crate::icon;
use crate::style::{
    BUTTON_PADDING, CONTENT_MAX_WIDTH, CONTENT_PADDING, CONTEXT_MENU_ITEM_PADDING,
    CONTEXT_MENU_MIN_WIDTH, CONTEXT_MENU_PADDING, CONTEXT_MENU_SEPARATOR_HEIGHT,
    CONTEXT_MENU_SEPARATOR_MARGIN, ELEMENT_SPACING, FINGERPRINT_SPACE_AFTER_LABEL,
    FINGERPRINT_SPACE_AFTER_SERVER_INFO, FINGERPRINT_SPACE_AFTER_TITLE,
    FINGERPRINT_SPACE_AFTER_WARNING, FINGERPRINT_SPACE_BEFORE_BUTTONS,
    FINGERPRINT_SPACE_BETWEEN_SECTIONS, INPUT_PADDING, MONOSPACE_FONT, NO_SPACING,
    SCROLLBAR_PADDING, SEPARATOR_HEIGHT, SORT_ICON_LEFT_MARGIN, SORT_ICON_RIGHT_MARGIN,
    SPACER_SIZE_MEDIUM, SPACER_SIZE_SMALL, TEXT_SIZE, TOOLTIP_BACKGROUND_PADDING, TOOLTIP_GAP,
    TOOLTIP_PADDING, TOOLTIP_TEXT_SIZE, context_menu_container_style, error_text_style,
    menu_button_danger_style, menu_button_style, muted_text_style, panel_title, separator_style,
    shaped_text, shaped_text_wrapped, tooltip_container_style, transparent_icon_button_style,
};
use crate::types::{
    InputId, Message, ServerConnection, TrackerManagementMode, TrackerManagementSortColumn,
    TrackerManagementState,
};
use crate::widgets::MenuButton;

fn hash_color<H: Hasher>(color: iced::Color, state: &mut H) {
    color.r.to_bits().hash(state);
    color.g.to_bits().hash(state);
    color.b.to_bits().hash(state);
    color.a.to_bits().hash(state);
}

// ============================================================================
// Status Bullet
// ============================================================================

/// Status classification for the row indicator.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum TrackerStatus {
    /// Connected and healthy.
    Connected,
    /// Stage-1 fingerprint observation pending admin review.
    PendingFingerprint,
    /// Disconnected, last_error set, or Stage-2 mismatch.
    Error,
}

fn classify_tracker_status(t: &TrackerInfo) -> TrackerStatus {
    if t.pending_fingerprint.is_some()
        && t.last_error_kind.as_deref() != Some(ERROR_KIND_TRACKER_FINGERPRINT_INTERCEPTED)
    {
        TrackerStatus::PendingFingerprint
    } else if t.connected && t.last_error.is_none() {
        TrackerStatus::Connected
    } else {
        TrackerStatus::Error
    }
}

fn status_tooltip(status: TrackerStatus, last_error: Option<&str>) -> String {
    match status {
        TrackerStatus::Connected => t("tooltip-tracker-connected"),
        TrackerStatus::PendingFingerprint => t("tooltip-tracker-fingerprint-pending"),
        TrackerStatus::Error => last_error
            .map(|s| s.to_string())
            .unwrap_or_else(|| t("tooltip-tracker-disconnected")),
    }
}

/// Sort priority for the Status column: Connected > PendingFingerprint > Error.
/// Smaller is "earlier" in ascending order.
fn status_sort_key(status: TrackerStatus) -> u8 {
    match status {
        TrackerStatus::Connected => 0,
        TrackerStatus::PendingFingerprint => 1,
        TrackerStatus::Error => 2,
    }
}

// ============================================================================
// Sorting
// ============================================================================

fn sort_trackers(
    trackers: &mut [TrackerInfo],
    column: TrackerManagementSortColumn,
    ascending: bool,
) {
    trackers.sort_by(|a, b| {
        let cmp = match column {
            TrackerManagementSortColumn::Name => fold_name(&a.name).cmp(&fold_name(&b.name)),
            TrackerManagementSortColumn::Address => {
                a.address.to_lowercase().cmp(&b.address.to_lowercase())
            }
            TrackerManagementSortColumn::Status => {
                let a_key = status_sort_key(classify_tracker_status(a));
                let b_key = status_sort_key(classify_tracker_status(b));
                a_key.cmp(&b_key)
            }
        };
        if ascending { cmp } else { cmp.reverse() }
    });
}

// ============================================================================
// Lazy Tracker Table
// ============================================================================

/// Dependencies for lazy tracker table rendering.
#[derive(Clone)]
struct TrackerTableDeps {
    trackers: Vec<TrackerInfo>,
    sort_column: TrackerManagementSortColumn,
    sort_ascending: bool,
    can_edit: bool,
    can_remove: bool,
    success_color: iced::Color,
    warning_color: iced::Color,
    danger_color: iced::Color,
}

impl Hash for TrackerTableDeps {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.trackers.len().hash(state);
        for t in &self.trackers {
            t.id.hash(state);
            t.name.hash(state);
            t.address.hash(state);
            t.port.hash(state);
            t.connected.hash(state);
            t.last_error.hash(state);
            t.last_error_kind.hash(state);
            t.pending_fingerprint.hash(state);
            // Deliberately excluded: `last_attempted_at`, `last_connected_at`.
            // The table doesn't render timestamps anywhere — including them
            // would invalidate the lazy cache on every status refresh and
            // burn the cache hit rate. If a future change shows timestamps
            // in the row (or in a tooltip), add them here.
        }
        self.sort_column.hash(state);
        self.sort_ascending.hash(state);
        self.can_edit.hash(state);
        self.can_remove.hash(state);
        hash_color(self.success_color, state);
        hash_color(self.warning_color, state);
        hash_color(self.danger_color, state);
    }
}

/// Build the tracker context menu (Accept Fingerprint, Edit, Remove).
fn build_tracker_context_menu(
    tracker_id: i64,
    name: String,
    has_pending_fingerprint: bool,
    can_edit: bool,
    can_remove: bool,
) -> Element<'static, Message> {
    let mut items: Vec<Element<'static, Message>> = vec![];

    if has_pending_fingerprint && can_edit {
        items.push(
            MenuButton::new(shaped_text(t("menu-tracker-accept-fingerprint")).size(TEXT_SIZE))
                .padding(CONTEXT_MENU_ITEM_PADDING)
                .width(Fill)
                .style(menu_button_style)
                .on_press(Message::TrackerManagementShowAcceptFingerprint(tracker_id))
                .into(),
        );
    }

    if can_edit {
        // Separator after Accept Fingerprint
        if has_pending_fingerprint {
            items.push(
                container(Space::new())
                    .width(Fill)
                    .height(CONTEXT_MENU_SEPARATOR_HEIGHT)
                    .style(separator_style)
                    .into(),
            );
        }
        items.push(
            MenuButton::new(shaped_text(t("button-edit")).size(TEXT_SIZE))
                .padding(CONTEXT_MENU_ITEM_PADDING)
                .width(Fill)
                .style(menu_button_style)
                .on_press(Message::TrackerManagementShowEdit(tracker_id))
                .into(),
        );
    }

    if can_remove {
        if can_edit {
            items.push(
                container(Space::new())
                    .width(Fill)
                    .height(CONTEXT_MENU_SEPARATOR_HEIGHT)
                    .style(separator_style)
                    .into(),
            );
        }
        items.push(
            MenuButton::new(shaped_text(t("button-remove")).size(TEXT_SIZE))
                .padding(CONTEXT_MENU_ITEM_PADDING)
                .width(Fill)
                .style(menu_button_danger_style)
                .on_press(Message::TrackerManagementShowRemove(tracker_id, name))
                .into(),
        );
    }

    container(Column::with_children(items).spacing(CONTEXT_MENU_SEPARATOR_MARGIN))
        .width(CONTEXT_MENU_MIN_WIDTH)
        .padding(CONTEXT_MENU_PADDING)
        .style(context_menu_container_style)
        .into()
}

fn lazy_tracker_table(deps: TrackerTableDeps) -> Element<'static, Message> {
    let can_edit = deps.can_edit;
    let can_remove = deps.can_remove;

    lazy(deps, move |deps| {
        // Status header (sortable, no label text — the column is just bullets).
        let status_sort_icon = sort_icon_or_placeholder(
            deps.sort_column == TrackerManagementSortColumn::Status,
            deps.sort_ascending,
        );
        let status_header_content: Element<'static, Message> = row![
            shaped_text(t("col-status"))
                .size(TEXT_SIZE)
                .wrapping(Wrapping::Word)
                .style(muted_text_style),
            Space::new().width(Fill),
            Space::new().width(SORT_ICON_LEFT_MARGIN),
            status_sort_icon,
            Space::new().width(SORT_ICON_RIGHT_MARGIN),
        ]
        .align_y(Center)
        .into();
        let status_header: Element<'static, Message> = button(status_header_content)
            .padding(NO_SPACING)
            .width(Length::Shrink)
            .style(transparent_icon_button_style)
            .on_press(Message::TrackerManagementSortChanged(
                TrackerManagementSortColumn::Status,
            ))
            .into();

        let success_color = deps.success_color;
        let warning_color = deps.warning_color;
        let danger_color = deps.danger_color;

        // Status column cell — bullet with tooltip.
        let status_column = table::column(status_header, move |tracker: TrackerInfo| {
            let status = classify_tracker_status(&tracker);
            let color = match status {
                TrackerStatus::Connected => success_color,
                TrackerStatus::PendingFingerprint => warning_color,
                TrackerStatus::Error => danger_color,
            };
            let tooltip_text = status_tooltip(status, tracker.last_error.as_deref());

            let bullet = shaped_text("●").size(TEXT_SIZE).color(color);

            tooltip(
                bullet,
                container(shaped_text(tooltip_text).size(TOOLTIP_TEXT_SIZE))
                    .padding(TOOLTIP_BACKGROUND_PADDING)
                    .style(tooltip_container_style),
                tooltip::Position::Top,
            )
            .gap(TOOLTIP_GAP)
            .padding(TOOLTIP_PADDING)
        })
        .width(Length::Shrink);

        // Name header
        let name_sort_icon = sort_icon_or_placeholder(
            deps.sort_column == TrackerManagementSortColumn::Name,
            deps.sort_ascending,
        );
        let name_header_content: Element<'static, Message> = row![
            shaped_text(t("col-name"))
                .size(TEXT_SIZE)
                .wrapping(Wrapping::Word)
                .style(muted_text_style),
            Space::new().width(Fill),
            Space::new().width(SORT_ICON_LEFT_MARGIN),
            name_sort_icon,
            Space::new().width(SORT_ICON_RIGHT_MARGIN),
        ]
        .align_y(Center)
        .into();
        let name_header: Element<'static, Message> = button(name_header_content)
            .padding(NO_SPACING)
            .width(Length::Shrink)
            .style(transparent_icon_button_style)
            .on_press(Message::TrackerManagementSortChanged(
                TrackerManagementSortColumn::Name,
            ))
            .into();

        // Name cell with context menu
        let name_column = table::column(name_header, move |tracker: TrackerInfo| {
            let tracker_id = tracker.id;
            let name_for_menu = tracker.name.clone();
            let has_pending = tracker.pending_fingerprint.is_some()
                && tracker.last_error_kind.as_deref()
                    != Some(ERROR_KIND_TRACKER_FINGERPRINT_INTERCEPTED);

            let text_widget = shaped_text(tracker.name)
                .size(TEXT_SIZE)
                .wrapping(Wrapping::WordOrGlyph);

            let content: Element<'static, Message> = if can_edit {
                button(text_widget)
                    .padding(NO_SPACING)
                    .style(transparent_icon_button_style)
                    .on_press(Message::TrackerManagementShowEdit(tracker_id))
                    .into()
            } else {
                text_widget.into()
            };

            if can_edit || can_remove {
                ContextMenu::new(content, move || {
                    build_tracker_context_menu(
                        tracker_id,
                        name_for_menu.clone(),
                        has_pending,
                        can_edit,
                        can_remove,
                    )
                })
                .into()
            } else {
                content
            }
        })
        .width(Length::FillPortion(1));

        // Address header
        let address_sort_icon = sort_icon_or_placeholder(
            deps.sort_column == TrackerManagementSortColumn::Address,
            deps.sort_ascending,
        );
        let address_header_content: Element<'static, Message> = row![
            shaped_text(t("col-address"))
                .size(TEXT_SIZE)
                .wrapping(Wrapping::Word)
                .style(muted_text_style),
            Space::new().width(Fill),
            Space::new().width(SORT_ICON_LEFT_MARGIN),
            address_sort_icon,
            Space::new().width(SORT_ICON_RIGHT_MARGIN),
            // Trailing gap so the rightmost column doesn't abut the scrollbar.
            Space::new().width(SCROLLBAR_PADDING),
        ]
        .align_y(Center)
        .into();
        let address_header: Element<'static, Message> = button(address_header_content)
            .padding(NO_SPACING)
            .width(Length::Shrink)
            .style(transparent_icon_button_style)
            .on_press(Message::TrackerManagementSortChanged(
                TrackerManagementSortColumn::Address,
            ))
            .into();

        // Address column. Trailing Space matches the header so the rightmost
        // column doesn't abut the scrollbar.
        let address_column = table::column(address_header, |tracker: TrackerInfo| {
            // Show the port suffix only when it differs from the well-known
            // default — keeps the common case clean while making non-default
            // ports visible without opening Edit on every row.
            let display = if tracker.port == DEFAULT_TRACKER_PORT {
                tracker.address
            } else {
                crate::uri::format_endpoint(&tracker.address, tracker.port)
            };
            row![
                shaped_text(display)
                    .size(TEXT_SIZE)
                    .wrapping(Wrapping::WordOrGlyph)
                    .style(muted_text_style),
                Space::new().width(SCROLLBAR_PADDING),
            ]
            .align_y(Center)
        })
        .width(Length::FillPortion(1));

        let columns = [status_column, name_column, address_column];

        table(columns, deps.trackers.clone())
            .width(Fill)
            .padding_x(SPACER_SIZE_SMALL)
            .padding_y(SPACER_SIZE_SMALL)
            .separator_x(NO_SPACING)
            .separator_y(SEPARATOR_HEIGHT)
    })
    .into()
}

// ============================================================================
// Trackers Tab Body
// ============================================================================

/// Toolbar for the Trackers tab (Add + Refresh icons).
fn trackers_tab_toolbar<'a>(
    conn: &'a ServerConnection,
    tracker_management: &'a TrackerManagementState,
) -> Element<'a, Message> {
    let add_btn = tab_toolbar_icon_button(
        icon::plus_circled(),
        "tooltip-tracker-add",
        Message::TrackerManagementShowAdd,
        conn.has_permission(PERMISSION_TRACKER_ADD),
    );
    let refresh_btn = tab_toolbar_icon_button(
        icon::refresh(),
        "tooltip-refresh",
        Message::TrackerManagementRefresh,
        tracker_management.all_trackers.is_some(),
    );

    row![add_btn, refresh_btn].spacing(SPACER_SIZE_SMALL).into()
}

/// Render the Trackers tab body — toolbar + sortable table.
pub fn trackers_tab_view<'a>(
    conn: &'a ServerConnection,
    tracker_management: &'a TrackerManagementState,
    theme: &Theme,
) -> Element<'a, Message> {
    let toolbar = trackers_tab_toolbar(conn, tracker_management);

    let body: Element<'a, Message> = match &tracker_management.all_trackers {
        None => container(
            shaped_text(t("tracker-management-loading"))
                .size(TEXT_SIZE)
                .style(muted_text_style),
        )
        .width(Fill)
        .center_x(Fill)
        .padding(SPACER_SIZE_SMALL)
        .into(),
        Some(Err(error)) => container(
            shaped_text_wrapped(error.clone())
                .size(TEXT_SIZE)
                .style(error_text_style),
        )
        .width(Fill)
        .center_x(Fill)
        .padding(SPACER_SIZE_SMALL)
        .into(),
        Some(Ok(trackers)) => {
            if trackers.is_empty() {
                container(
                    shaped_text(t("tracker-management-no-trackers"))
                        .size(TEXT_SIZE)
                        .style(muted_text_style),
                )
                .width(Fill)
                .height(Fill)
                .center_x(Fill)
                .center_y(Fill)
                .padding(SPACER_SIZE_SMALL)
                .into()
            } else {
                let mut sorted = trackers.clone();
                sort_trackers(
                    &mut sorted,
                    tracker_management.sort_column,
                    tracker_management.sort_ascending,
                );

                let palette = theme.extended_palette();
                let deps = TrackerTableDeps {
                    trackers: sorted,
                    sort_column: tracker_management.sort_column,
                    sort_ascending: tracker_management.sort_ascending,
                    can_edit: conn.has_permission(PERMISSION_TRACKER_EDIT),
                    can_remove: conn.has_permission(PERMISSION_TRACKER_REMOVE),
                    success_color: palette.success.base.color,
                    warning_color: palette.warning.base.color,
                    danger_color: palette.danger.base.color,
                };

                scrollable(lazy_tracker_table(deps)).height(Fill).into()
            }
        }
    };

    column![toolbar, body]
        .spacing(SPACER_SIZE_SMALL)
        .height(Fill)
        .into()
}

// ============================================================================
// Add Tracker Subview
// ============================================================================

/// Render the Add Tracker subview (full-panel form).
pub fn add_tracker_view(state: &TrackerManagementState) -> Element<'_, Message> {
    let title = panel_title(t("title-tracker-add"));

    let can_submit = !state.add_name.trim().is_empty()
        && !state.add_address.trim().is_empty()
        && !state.is_submitting;

    let submit_action = if can_submit {
        Message::AddTrackerSubmit
    } else {
        Message::ValidateAddTracker
    };

    let mut items: Vec<Element<'_, Message>> = vec![title.into()];

    if let Some(error) = &state.form_error {
        items.push(
            shaped_text_wrapped(error.clone())
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

    // Name
    let name_input = text_input(&t("placeholder-tracker-name"), &state.add_name)
        .on_input(Message::AddTrackerNameChanged)
        .on_submit(submit_action.clone())
        .id(Id::from(InputId::AddTrackerName))
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE)
        .width(Fill);
    items.push(
        row![
            shaped_text(t("label-name")).size(TEXT_SIZE),
            Space::new().width(ELEMENT_SPACING),
            name_input
        ]
        .align_y(Center)
        .into(),
    );

    // Address
    let address_input = text_input(&t("placeholder-tracker-address"), &state.add_address)
        .on_input(Message::AddTrackerAddressChanged)
        .on_submit(submit_action.clone())
        .id(Id::from(InputId::AddTrackerAddress))
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE)
        .width(Fill);
    items.push(
        row![
            shaped_text(t("label-address")).size(TEXT_SIZE),
            Space::new().width(ELEMENT_SPACING),
            address_input
        ]
        .align_y(Center)
        .into(),
    );

    // Port
    let port_input: Element<'_, Message> = NumberInput::new(
        &state.add_port,
        1..=u16::MAX,
        Message::AddTrackerPortChanged,
    )
    .id(Id::from(InputId::AddTrackerPort))
    .padding(INPUT_PADDING)
    .into();
    items.push(
        row![
            shaped_text(t("label-port")).size(TEXT_SIZE),
            Space::new().width(ELEMENT_SPACING),
            port_input
        ]
        .align_y(Center)
        .into(),
    );

    // Password (optional)
    let password_input = text_input(&t("placeholder-tracker-password"), &state.add_password)
        .on_input(Message::AddTrackerPasswordChanged)
        .on_submit(submit_action.clone())
        .id(Id::from(InputId::AddTrackerPassword))
        .secure(true)
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE)
        .width(Fill);
    items.push(
        row![
            shaped_text(t("label-tracker-password")).size(TEXT_SIZE),
            Space::new().width(ELEMENT_SPACING),
            password_input
        ]
        .align_y(Center)
        .into(),
    );

    // Fingerprint (optional). Last text input — consumes
    // `submit_action`. Conventionally last across both tracker forms.
    let fingerprint_input = text_input(
        &t("placeholder-tracker-fingerprint"),
        &state.add_fingerprint,
    )
    .on_input(Message::AddTrackerFingerprintChanged)
    .on_submit(submit_action)
    .id(Id::from(InputId::AddTrackerFingerprint))
    .padding(INPUT_PADDING)
    .size(TEXT_SIZE)
    .font(MONOSPACE_FONT)
    .width(Fill);
    items.push(
        row![
            shaped_text(t("label-fingerprint")).size(TEXT_SIZE),
            Space::new().width(ELEMENT_SPACING),
            fingerprint_input
        ]
        .align_y(Center)
        .into(),
    );

    // Enabled checkbox
    let enabled_checkbox = checkbox(state.add_enabled)
        .label(t("label-enabled"))
        .on_toggle(Message::AddTrackerEnabledToggled)
        .size(TEXT_SIZE)
        .text_shaping(text::Shaping::Advanced);
    items.push(enabled_checkbox.into());

    items.push(Space::new().height(SPACER_SIZE_MEDIUM).into());

    let cancel_button = button(shaped_text(t("button-cancel")).size(TEXT_SIZE))
        .on_press(Message::CancelTrackerManagement)
        .padding(BUTTON_PADDING)
        .style(btn::secondary);

    let submit_button = if can_submit {
        button(shaped_text(t("button-save")).size(TEXT_SIZE))
            .on_press(Message::AddTrackerSubmit)
            .padding(BUTTON_PADDING)
    } else {
        button(shaped_text(t("button-save")).size(TEXT_SIZE)).padding(BUTTON_PADDING)
    };

    items.push(
        row![Space::new().width(Fill), cancel_button, submit_button]
            .spacing(ELEMENT_SPACING)
            .into(),
    );

    let form = Column::with_children(items)
        .spacing(ELEMENT_SPACING)
        .padding(CONTENT_PADDING)
        .max_width(CONTENT_MAX_WIDTH);

    scrollable_panel(form)
}

// ============================================================================
// Edit Tracker Subview
// ============================================================================

/// Render the Edit Tracker subview (full-panel form).
pub fn edit_tracker_view(state: &TrackerManagementState) -> Element<'_, Message> {
    let TrackerManagementMode::Edit {
        original_name,
        original_address: _,
        original_port: _,
        original_fingerprint: _,
        original_password: _,
        original_enabled: _,
        last_error,
        name,
        address,
        port,
        fingerprint,
        password,
        enabled,
        ..
    } = &state.mode
    else {
        // Caller guards against this; return empty as a safety net.
        return Space::new().into();
    };

    let title = panel_title(t("title-tracker-edit"));
    let subtitle = shaped_text_wrapped(original_name.clone())
        .size(TEXT_SIZE)
        .width(Fill)
        .align_x(Center)
        .style(muted_text_style);

    let can_submit = state.mode.has_effective_tracker_update_changes()
        && !name.trim().is_empty()
        && !address.trim().is_empty()
        && !state.is_submitting;

    let submit_action = if can_submit {
        Message::EditTrackerSubmit
    } else {
        Message::ValidateEditTracker
    };

    let mut items: Vec<Element<'_, Message>> = vec![title.into(), subtitle.into()];

    // Form-level error has precedence over last_error context.
    if let Some(error) = &state.form_error {
        items.push(
            shaped_text_wrapped(error.clone())
                .size(TEXT_SIZE)
                .width(Fill)
                .align_x(Center)
                .style(error_text_style)
                .into(),
        );
        items.push(Space::new().height(SPACER_SIZE_SMALL).into());
    } else if let Some(err) = last_error {
        // Show last operational error as informational context. Uses
        // muted styling (not the danger color) because this banner is
        // historical — it describes what failed on the previous
        // registration cycle, not anything the admin entered just now.
        // The form-error path above keeps the danger color for actual
        // validation/rejection errors that block saving.
        items.push(
            shaped_text_wrapped(err.clone())
                .size(TEXT_SIZE)
                .width(Fill)
                .align_x(Center)
                .style(muted_text_style)
                .into(),
        );
        items.push(Space::new().height(SPACER_SIZE_SMALL).into());
    } else {
        items.push(Space::new().height(SPACER_SIZE_MEDIUM).into());
    }

    // Name
    let name_input = text_input(&t("placeholder-tracker-name"), name)
        .on_input(Message::EditTrackerNameChanged)
        .on_submit(submit_action.clone())
        .id(Id::from(InputId::EditTrackerName))
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE)
        .width(Fill);
    items.push(
        row![
            shaped_text(t("label-name")).size(TEXT_SIZE),
            Space::new().width(ELEMENT_SPACING),
            name_input
        ]
        .align_y(Center)
        .into(),
    );

    // Address
    let address_input = text_input(&t("placeholder-tracker-address"), address)
        .on_input(Message::EditTrackerAddressChanged)
        .on_submit(submit_action.clone())
        .id(Id::from(InputId::EditTrackerAddress))
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE)
        .width(Fill);
    items.push(
        row![
            shaped_text(t("label-address")).size(TEXT_SIZE),
            Space::new().width(ELEMENT_SPACING),
            address_input
        ]
        .align_y(Center)
        .into(),
    );

    // Port
    let port_input: Element<'_, Message> =
        NumberInput::new(port, 1..=u16::MAX, Message::EditTrackerPortChanged)
            .id(Id::from(InputId::EditTrackerPort))
            .padding(INPUT_PADDING)
            .into();
    items.push(
        row![
            shaped_text(t("label-port")).size(TEXT_SIZE),
            Space::new().width(ELEMENT_SPACING),
            port_input
        ]
        .align_y(Center)
        .into(),
    );

    // Password (optional)
    let password_input = text_input(&t("placeholder-tracker-password"), password)
        .on_input(Message::EditTrackerPasswordChanged)
        .on_submit(submit_action.clone())
        .id(Id::from(InputId::EditTrackerPassword))
        .secure(true)
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE)
        .width(Fill);
    items.push(
        row![
            shaped_text(t("label-tracker-password")).size(TEXT_SIZE),
            Space::new().width(ELEMENT_SPACING),
            password_input
        ]
        .align_y(Center)
        .into(),
    );

    // Fingerprint (optional). Last text input — consumes
    // `submit_action`. Conventionally last across both tracker forms.
    let fingerprint_input = text_input(&t("placeholder-tracker-fingerprint"), fingerprint)
        .on_input(Message::EditTrackerFingerprintChanged)
        .on_submit(submit_action)
        .id(Id::from(InputId::EditTrackerFingerprint))
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE)
        .font(MONOSPACE_FONT)
        .width(Fill);
    items.push(
        row![
            shaped_text(t("label-fingerprint")).size(TEXT_SIZE),
            Space::new().width(ELEMENT_SPACING),
            fingerprint_input
        ]
        .align_y(Center)
        .into(),
    );

    // Enabled checkbox
    let enabled_checkbox = checkbox(*enabled)
        .label(t("label-enabled"))
        .on_toggle(Message::EditTrackerEnabledToggled)
        .size(TEXT_SIZE)
        .text_shaping(text::Shaping::Advanced);
    items.push(enabled_checkbox.into());

    items.push(Space::new().height(SPACER_SIZE_MEDIUM).into());

    let cancel_button = button(shaped_text(t("button-cancel")).size(TEXT_SIZE))
        .on_press(Message::CancelTrackerManagement)
        .padding(BUTTON_PADDING)
        .style(btn::secondary);

    let submit_button = if can_submit {
        button(shaped_text(t("button-save")).size(TEXT_SIZE))
            .on_press(Message::EditTrackerSubmit)
            .padding(BUTTON_PADDING)
    } else {
        button(shaped_text(t("button-save")).size(TEXT_SIZE)).padding(BUTTON_PADDING)
    };

    items.push(
        row![Space::new().width(Fill), cancel_button, submit_button]
            .spacing(ELEMENT_SPACING)
            .into(),
    );

    let form = Column::with_children(items)
        .spacing(ELEMENT_SPACING)
        .padding(CONTENT_PADDING)
        .max_width(CONTENT_MAX_WIDTH);

    scrollable_panel(form)
}

// ============================================================================
// Remove Tracker Confirm Modal
// ============================================================================

/// Render the Remove Tracker confirmation modal.
pub fn remove_tracker_modal(state: &TrackerManagementState) -> Element<'_, Message> {
    let TrackerManagementMode::ConfirmRemove { name, .. } = &state.mode else {
        return Space::new().into();
    };

    let title = panel_title(t("dialog-remove-tracker-title"));

    let body_text = super::helpers::t_args("dialog-remove-tracker-body", &[("name", name)]);
    let body = shaped_text_wrapped(body_text)
        .size(TEXT_SIZE)
        .width(Fill)
        .align_x(Center);

    let confirm_button = if !state.is_remove_submitting {
        button(shaped_text(t("button-remove")).size(TEXT_SIZE))
            .on_press(Message::RemoveTrackerConfirm)
            .padding(BUTTON_PADDING)
            .style(btn::danger)
    } else {
        button(shaped_text(t("button-remove")).size(TEXT_SIZE))
            .padding(BUTTON_PADDING)
            .style(btn::danger)
    };

    let cancel_button = button(shaped_text(t("button-cancel")).size(TEXT_SIZE))
        .on_press(Message::RemoveTrackerCancel)
        .padding(BUTTON_PADDING)
        .style(btn::secondary);

    let mut items: Vec<Element<'_, Message>> = vec![title.into()];

    if let Some(err) = &state.remove_error {
        items.push(
            shaped_text_wrapped(err.clone())
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
        body.into(),
        Space::new().height(SPACER_SIZE_MEDIUM).into(),
        row![Space::new().width(Fill), cancel_button, confirm_button]
            .spacing(ELEMENT_SPACING)
            .into(),
    ]);

    let form = Column::with_children(items)
        .spacing(ELEMENT_SPACING)
        .padding(CONTENT_PADDING)
        .max_width(CONTENT_MAX_WIDTH);

    scrollable_panel(form)
}

// ============================================================================
// Accept Fingerprint Modal
// ============================================================================

/// Render the Accept Fingerprint dialog (Stage-1 fingerprint promotion).
pub fn accept_fingerprint_modal(state: &TrackerManagementState) -> Element<'_, Message> {
    let TrackerManagementMode::AcceptFingerprint {
        name,
        address,
        port,
        expected,
        received,
        ..
    } = &state.mode
    else {
        return Space::new().into();
    };

    let title = panel_title(t("title-fingerprint-accept"));

    let server_line_text = format!("{} - {}", name, crate::uri::format_endpoint(address, *port));
    let server_line = shaped_text(server_line_text).size(TEXT_SIZE);

    let warning = shaped_text_wrapped(t("tracker-fingerprint-warning")).size(TEXT_SIZE);

    let expected_label = shaped_text(t("label-expected-fingerprint")).size(TEXT_SIZE);
    let expected_value: Element<'_, Message> = if let Some(fp) = expected.as_ref() {
        shaped_text(format_fingerprint_multiline(fp))
            .size(TEXT_SIZE)
            .font(MONOSPACE_FONT)
            .into()
    } else {
        shaped_text(t("label-fingerprint-unpinned"))
            .size(TEXT_SIZE)
            .style(muted_text_style)
            .into()
    };

    let received_label = shaped_text(t("label-received-fingerprint")).size(TEXT_SIZE);
    let received_value = shaped_text(format_fingerprint_multiline(received))
        .size(TEXT_SIZE)
        .font(MONOSPACE_FONT);

    let cancel_button = button(
        shaped_text(t("button-cancel"))
            .size(TEXT_SIZE)
            .width(Length::Fill)
            .center(),
    )
    .on_press(Message::AcceptFingerprintCancel)
    .padding(BUTTON_PADDING)
    .style(btn::secondary);

    let accept_button = if !state.is_accept_fingerprint_submitting {
        button(
            shaped_text(t("button-accept-fingerprint"))
                .size(TEXT_SIZE)
                .width(Length::Fill)
                .center(),
        )
        .on_press(Message::AcceptFingerprintConfirm)
        .padding(BUTTON_PADDING)
        .style(btn::danger)
    } else {
        button(
            shaped_text(t("button-accept-fingerprint"))
                .size(TEXT_SIZE)
                .width(Length::Fill)
                .center(),
        )
        .padding(BUTTON_PADDING)
        .style(btn::danger)
    };

    let button_row = row![
        Space::new().width(Length::Fill),
        cancel_button,
        accept_button
    ]
    .spacing(ELEMENT_SPACING);

    let mut items: Vec<Element<'_, Message>> = vec![title.into()];

    // When an error banner is present, follow it with the small inter-row
    // spacer used by the Add / Edit forms — `FINGERPRINT_SPACE_AFTER_TITLE`
    // is meant for the title-to-server-line gap, not for after-error
    // padding. When there's no error, fall through to the full title gap.
    if let Some(err) = &state.accept_fingerprint_error {
        items.push(
            shaped_text_wrapped(err.clone())
                .size(TEXT_SIZE)
                .width(Fill)
                .align_x(Center)
                .style(error_text_style)
                .into(),
        );
        items.push(Space::new().height(SPACER_SIZE_SMALL).into());
    } else {
        items.push(Space::new().height(FINGERPRINT_SPACE_AFTER_TITLE).into());
    }

    items.extend([
        server_line.into(),
        Space::new()
            .height(FINGERPRINT_SPACE_AFTER_SERVER_INFO)
            .into(),
        warning.into(),
        Space::new().height(FINGERPRINT_SPACE_AFTER_WARNING).into(),
        expected_label.into(),
        Space::new().height(FINGERPRINT_SPACE_AFTER_LABEL).into(),
        expected_value,
        Space::new()
            .height(FINGERPRINT_SPACE_BETWEEN_SECTIONS)
            .into(),
        received_label.into(),
        Space::new().height(FINGERPRINT_SPACE_AFTER_LABEL).into(),
        received_value.into(),
        Space::new().height(FINGERPRINT_SPACE_BEFORE_BUTTONS).into(),
        button_row.into(),
    ]);

    let dialog = Column::with_children(items)
        .spacing(ELEMENT_SPACING)
        .padding(CONTENT_PADDING)
        .max_width(CONTENT_MAX_WIDTH);

    scrollable_modal(dialog)
}

#[cfg(test)]
mod tests {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    use super::*;

    fn sample_tracker() -> TrackerInfo {
        TrackerInfo {
            id: 1,
            address: "tracker.example".to_string(),
            port: DEFAULT_TRACKER_PORT,
            fingerprint: Some("AA:BB".to_string()),
            password: None,
            name: "Primary".to_string(),
            enabled: true,
            created_at: 100,
            updated_at: 100,
            connected: true,
            last_connected_at: Some(100),
            last_attempted_at: Some(100),
            last_error: None,
            last_error_kind: None,
            pending_fingerprint: None,
            refresh_interval: Some(300),
        }
    }

    fn sample_deps() -> TrackerTableDeps {
        TrackerTableDeps {
            trackers: vec![sample_tracker()],
            sort_column: TrackerManagementSortColumn::Name,
            sort_ascending: true,
            can_edit: true,
            can_remove: true,
            success_color: iced::Color::from_rgb(0.0, 1.0, 0.0),
            warning_color: iced::Color::from_rgb(1.0, 1.0, 0.0),
            danger_color: iced::Color::from_rgb(1.0, 0.0, 0.0),
        }
    }

    fn deps_hash(deps: &TrackerTableDeps) -> u64 {
        let mut hasher = DefaultHasher::new();
        deps.hash(&mut hasher);
        hasher.finish()
    }

    fn assert_hash_changes(mutate: impl FnOnce(&mut TrackerTableDeps)) {
        let mut deps = sample_deps();
        let base = deps_hash(&deps);
        mutate(&mut deps);
        assert_ne!(deps_hash(&deps), base);
    }

    #[test]
    fn tracker_table_deps_hash_changes_on_rendered_and_action_fields() {
        assert_hash_changes(|d| d.trackers[0].id = 2);
        assert_hash_changes(|d| d.trackers[0].name = "Backup".to_string());
        assert_hash_changes(|d| d.trackers[0].address = "other.example".to_string());
        assert_hash_changes(|d| d.trackers[0].port = 7511);
        assert_hash_changes(|d| d.trackers[0].connected = false);
        assert_hash_changes(|d| d.trackers[0].last_error = Some("failed".to_string()));
        assert_hash_changes(|d| d.trackers[0].last_error_kind = Some("unauthorized".to_string()));
        assert_hash_changes(|d| d.trackers[0].pending_fingerprint = Some("CC:DD".to_string()));
        assert_hash_changes(|d| d.sort_column = TrackerManagementSortColumn::Status);
        assert_hash_changes(|d| d.sort_ascending = false);
        assert_hash_changes(|d| d.can_edit = false);
        assert_hash_changes(|d| d.can_remove = false);
        assert_hash_changes(|d| d.success_color = iced::Color::from_rgb(0.0, 0.5, 0.0));
        assert_hash_changes(|d| d.warning_color = iced::Color::from_rgb(0.5, 0.5, 0.0));
        assert_hash_changes(|d| d.danger_color = iced::Color::from_rgb(0.5, 0.0, 0.0));
    }

    #[test]
    fn tracker_table_deps_hash_ignores_non_row_fields() {
        let mut deps = sample_deps();
        let base = deps_hash(&deps);

        deps.trackers[0].fingerprint = Some("CC:DD".to_string());
        deps.trackers[0].password = Some("secret".to_string());
        deps.trackers[0].enabled = false;
        deps.trackers[0].created_at = 200;
        deps.trackers[0].updated_at = 300;
        deps.trackers[0].last_connected_at = Some(400);
        deps.trackers[0].last_attempted_at = Some(500);
        deps.trackers[0].refresh_interval = Some(600);

        assert_eq!(deps_hash(&deps), base);
    }
}
