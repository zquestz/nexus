//! Tracker discovery (Trackers) panel view.
//!
//! Mode dispatch happens in [`tracker_browser_view`]:
//!   - `List` → list view (title row + toolbar + search + body)
//!   - `Add` → [`add_tracker_view`]
//!   - `Edit` → [`edit_tracker_view`]
//!   - `ConfirmRemove` → [`remove_tracker_modal`]
//!   - `AcceptFingerprint` → falls through to the list view; the
//!     dedicated dialog lands with the network layer.
//!
//! In list mode the toolbar's `[+]` Add button is always enabled,
//! Edit and Remove are conditionally enabled when a tracker is
//! selected, and Refresh is disabled until the network layer lands.
//! Sort, dropdown selection, and search input are fully wired.

use std::hash::{Hash, Hasher};

use iced::alignment::Vertical;
use iced::widget::button as btn;
use iced::widget::text::Wrapping;
use iced::widget::{
    Column, Id, Space, button, column, container, lazy, pick_list, row, scrollable, table,
    text_input, tooltip,
};
use iced::{Center, Element, Fill, Length};
use iced_aw::NumberInput;
use nexus_common::tracker_protocol::ServerEntry;
use uuid::Uuid;

use super::helpers::{sort_icon_or_placeholder, t_args};
use super::layout::scrollable_panel;
use crate::i18n::t;
use crate::icon;
use crate::style::{
    BUTTON_PADDING, CONTENT_MAX_WIDTH, CONTENT_PADDING, ELEMENT_SPACING, HEADING_BUTTON_PADDING,
    ICON_SIZE, INPUT_PADDING, MONOSPACE_FONT, NO_SPACING, SCROLLBAR_PADDING, SEPARATOR_HEIGHT,
    SORT_ICON_LEFT_MARGIN, SORT_ICON_RIGHT_MARGIN, SPACER_SIZE_MEDIUM, SPACER_SIZE_SMALL,
    TEXT_SIZE, TITLE_SIZE, TOOLBAR_BUTTON_PADDING, TOOLTIP_BACKGROUND_PADDING, TOOLTIP_GAP,
    TOOLTIP_PADDING, TOOLTIP_TEXT_SIZE, content_background_style, disabled_icon_button_style,
    error_text_style, muted_text_style, panel_title, shaped_text, shaped_text_wrapped,
    tooltip_container_style, transparent_icon_button_style,
};
use crate::types::{
    ClientTracker, InputId, Message, TrackerBrowserMode, TrackerBrowserSortColumn,
    TrackerBrowserState, TrackerCacheEntry,
};

/// Approximate rendered height of an iced 0.14 `pick_list` with
/// `text_size = TEXT_SIZE` and default padding. The toolbar row's
/// icon buttons are wrapped in containers of this exact height with
/// `align_y(Vertical::Center)` so they align with the pulldown menu
/// without forcing the row to grow Fill-height.
const TOOLBAR_ROW_HEIGHT: f32 = 30.0;

// =============================================================================
// Dropdown option
// =============================================================================

/// `pick_list` option for the tracker dropdown. Holds the stable id and
/// the display name; renders the name via `Display`. `Eq + Hash` are
/// required by `pick_list` to match the current selection against the
/// option list.
#[derive(Clone, Debug, PartialEq, Eq)]
struct TrackerOption {
    id: Uuid,
    name: String,
}

impl Hash for TrackerOption {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl std::fmt::Display for TrackerOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)
    }
}

fn build_options(trackers: &[ClientTracker]) -> Vec<TrackerOption> {
    let mut options: Vec<TrackerOption> = trackers
        .iter()
        .map(|t| TrackerOption {
            id: t.id,
            name: t.name.clone(),
        })
        .collect();
    options.sort_by_key(|o| o.name.to_lowercase());
    options
}

// =============================================================================
// Lazy server table
// =============================================================================

/// Dependencies the lazy-cached table hashes against. Step 2 always
/// passes an empty `entries` vec because no fetch path exists yet,
/// but the dependency shape is the one step 5+ will reuse.
#[derive(Clone)]
struct ServerTableDeps {
    entries: Vec<ServerEntry>,
    sort_column: TrackerBrowserSortColumn,
    sort_ascending: bool,
}

impl Hash for ServerTableDeps {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.entries.len().hash(state);
        for e in &self.entries {
            e.name.hash(state);
            e.description.hash(state);
            e.user_count.hash(state);
        }
        self.sort_column.hash(state);
        self.sort_ascending.hash(state);
    }
}

fn lazy_server_table(deps: ServerTableDeps) -> Element<'static, Message> {
    lazy(deps, |deps| {
        // Name header (sortable)
        let name_sort_icon = sort_icon_or_placeholder(
            deps.sort_column == TrackerBrowserSortColumn::Name,
            deps.sort_ascending,
        );
        let name_header_content: Element<'static, Message> = row![
            shaped_text(t("col-name"))
                .size(TEXT_SIZE)
                .wrapping(Wrapping::Word)
                .style(muted_text_style),
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
            .on_press(Message::TrackerBrowserSortChanged(
                TrackerBrowserSortColumn::Name,
            ))
            .into();

        let name_column = table::column(name_header, |entry: ServerEntry| {
            shaped_text(entry.name)
                .size(TEXT_SIZE)
                .wrapping(Wrapping::WordOrGlyph)
        })
        .width(Length::FillPortion(1));

        // Description header (sortable)
        let desc_sort_icon = sort_icon_or_placeholder(
            deps.sort_column == TrackerBrowserSortColumn::Description,
            deps.sort_ascending,
        );
        let desc_header_content: Element<'static, Message> = row![
            shaped_text(t("col-description"))
                .size(TEXT_SIZE)
                .wrapping(Wrapping::Word)
                .style(muted_text_style),
            Space::new().width(SORT_ICON_LEFT_MARGIN),
            desc_sort_icon,
            Space::new().width(SORT_ICON_RIGHT_MARGIN),
        ]
        .align_y(Center)
        .into();
        let desc_header: Element<'static, Message> = button(desc_header_content)
            .padding(NO_SPACING)
            .width(Length::Shrink)
            .style(transparent_icon_button_style)
            .on_press(Message::TrackerBrowserSortChanged(
                TrackerBrowserSortColumn::Description,
            ))
            .into();

        let desc_column = table::column(desc_header, |entry: ServerEntry| {
            shaped_text(entry.description.unwrap_or_default())
                .size(TEXT_SIZE)
                .wrapping(Wrapping::WordOrGlyph)
                .style(muted_text_style)
        })
        .width(Length::FillPortion(1));

        // Users header (sortable, rightmost — trailing scrollbar padding)
        let users_sort_icon = sort_icon_or_placeholder(
            deps.sort_column == TrackerBrowserSortColumn::Users,
            deps.sort_ascending,
        );
        let users_header_content: Element<'static, Message> = row![
            shaped_text(t("col-users"))
                .size(TEXT_SIZE)
                .wrapping(Wrapping::Word)
                .style(muted_text_style),
            Space::new().width(SORT_ICON_LEFT_MARGIN),
            users_sort_icon,
            Space::new().width(SORT_ICON_RIGHT_MARGIN),
            Space::new().width(SCROLLBAR_PADDING),
        ]
        .align_y(Center)
        .into();
        let users_header: Element<'static, Message> = button(users_header_content)
            .padding(NO_SPACING)
            .width(Length::Shrink)
            .style(transparent_icon_button_style)
            .on_press(Message::TrackerBrowserSortChanged(
                TrackerBrowserSortColumn::Users,
            ))
            .into();

        let users_column = table::column(users_header, |entry: ServerEntry| {
            row![
                shaped_text(entry.user_count.to_string()).size(TEXT_SIZE),
                Space::new().width(SCROLLBAR_PADDING),
            ]
            .align_y(Center)
        })
        .width(Length::Shrink);

        let columns = [name_column, desc_column, users_column];

        table(columns, deps.entries.clone())
            .width(Fill)
            .padding_x(SPACER_SIZE_SMALL)
            .padding_y(SPACER_SIZE_SMALL)
            .separator_x(NO_SPACING)
            .separator_y(SEPARATOR_HEIGHT)
    })
    .into()
}

// =============================================================================
// Toolbar status text
// =============================================================================

/// Render the right-aligned status text reflecting the cache state for
/// the selected tracker. Empty when no tracker is selected; muted while
/// loading; danger-styled on error; neutral on success.
fn toolbar_status<'a>(cache: Option<&'a TrackerCacheEntry>) -> Element<'a, Message> {
    let Some(entry) = cache else {
        return Space::new().into();
    };
    if entry.is_fetching && entry.entries.is_none() {
        return shaped_text(t("tracker-browser-status-loading"))
            .size(TEXT_SIZE)
            .style(muted_text_style)
            .into();
    }
    if let Some(error) = &entry.error {
        let msg = t_args("tracker-browser-status-error", &[("message", error)]);
        return shaped_text_wrapped(msg)
            .size(TEXT_SIZE)
            .style(error_text_style)
            .into();
    }
    if let Some(entries) = &entry.entries {
        let count = entries.len().to_string();
        let label = t_args("tracker-browser-status-servers", &[("count", &count)]);
        return shaped_text(label).size(TEXT_SIZE).into();
    }
    Space::new().into()
}

// =============================================================================
// List-area body
// =============================================================================

/// Decide what fills the list-area below the search row, per the
/// cache-state matrix in the spec:
///
/// | `entries`  | `is_fetching` | renders                              |
/// |------------|---------------|--------------------------------------|
/// | `Some(_)`  | any           | the table (last successful fetch)    |
/// | `None`     | `true`        | loading indicator                    |
/// | `None`     | `false`       | empty state (initial / never fetched)|
fn list_body<'a>(
    state: &'a TrackerBrowserState,
    selected_cache: Option<&'a TrackerCacheEntry>,
) -> Element<'a, Message> {
    if state.selected_tracker.is_none() || selected_cache.is_none() {
        // No tracker selected, or no fetch has ever occurred for the
        // selected tracker — render an empty area. The network layer
        // populates the cache on selection change once it lands.
        return Space::new().into();
    }

    // safe — guard above
    let cache = selected_cache.expect("guarded above");

    if let Some(entries) = &cache.entries {
        if entries.is_empty() {
            return container(
                shaped_text(t("empty-tracker-no-servers"))
                    .size(TEXT_SIZE)
                    .style(muted_text_style),
            )
            .width(Fill)
            .center_x(Fill)
            .padding(SPACER_SIZE_SMALL)
            .into();
        }
        let deps = ServerTableDeps {
            entries: entries.clone(),
            sort_column: state.sort_column,
            sort_ascending: state.sort_ascending,
        };
        return scrollable(lazy_server_table(deps)).height(Fill).into();
    }

    if cache.is_fetching {
        return container(
            shaped_text(t("tracker-browser-status-loading"))
                .size(TEXT_SIZE)
                .style(muted_text_style),
        )
        .width(Fill)
        .center_x(Fill)
        .padding(SPACER_SIZE_SMALL)
        .into();
    }

    // None + !is_fetching — initial / never fetched. Step 2 has no
    // fetch path so this is the dominant case. Render an empty space;
    // the toolbar status text already communicates "nothing yet".
    Space::new().into()
}

// =============================================================================
// Top-level view
// =============================================================================

/// Render the tracker discovery panel. Dispatches by mode.
/// `AcceptFingerprint` falls through to the list view in step 3 —
/// step 6 wires its dedicated dialog.
pub fn tracker_browser_view<'a>(
    state: &'a TrackerBrowserState,
    trackers: &'a [ClientTracker],
) -> Element<'a, Message> {
    match &state.mode {
        TrackerBrowserMode::List | TrackerBrowserMode::AcceptFingerprint { .. } => {
            list_view(state, trackers)
        }
        TrackerBrowserMode::Add => add_tracker_view(state),
        TrackerBrowserMode::Edit { .. } => edit_tracker_view(state),
        TrackerBrowserMode::ConfirmRemove { .. } => remove_tracker_modal(state),
    }
}

fn list_view<'a>(
    state: &'a TrackerBrowserState,
    trackers: &'a [ClientTracker],
) -> Element<'a, Message> {
    let has_trackers = !trackers.is_empty();
    let options = build_options(trackers);
    let selected_option = state
        .selected_tracker
        .and_then(|id| options.iter().find(|o| o.id == id).cloned());
    let selected_cache = state.selected_tracker.and_then(|id| state.cache.get(&id));

    // -- Title row: news-style centered title with a balanced [+]
    // button on the right. The invisible spacer on the left equals the
    // button's rendered width so the title sits geometrically centered
    // even with the action icon next to it.
    let add_icon = container(icon::plus_circled().size(ICON_SIZE))
        .width(ICON_SIZE)
        .height(ICON_SIZE)
        .align_x(iced::alignment::Horizontal::Center)
        .align_y(Vertical::Center);
    let add_button_inner = button(add_icon)
        .on_press(Message::TrackerBrowserShowAdd)
        .padding(HEADING_BUTTON_PADDING)
        .style(transparent_icon_button_style);
    let add_button: Element<'a, Message> = tooltip(
        add_button_inner,
        container(shaped_text(t("tooltip-tracker-add")).size(TOOLTIP_TEXT_SIZE))
            .padding(TOOLTIP_BACKGROUND_PADDING)
            .style(tooltip_container_style),
        tooltip::Position::Top,
    )
    .gap(TOOLTIP_GAP)
    .padding(TOOLTIP_PADDING)
    .into();

    // Title row with [+] add button on the right. No leading/trailing
    // `SCROLLBAR_PADDING` gutters — the body (lazy table widget)
    // reserves its scrollbar slot in the rightmost column, so the
    // title doesn't need to inset to match. Button lands at
    // `form_right - 20`, matching news/transfers buttons.
    let button_width = ICON_SIZE + HEADING_BUTTON_PADDING.left + HEADING_BUTTON_PADDING.right;
    let title_row: Element<'a, Message> = row![
        // Balance the [+] on the right
        Space::new().width(button_width),
        shaped_text(t("title-trackers"))
            .size(TITLE_SIZE)
            .width(Fill)
            .align_x(Center),
        add_button,
    ]
    .align_y(Center)
    .into();

    // -- Toolbar row: pulldown + Edit/Remove/Refresh icons + status text.
    // Both the pulldown and each icon button are wrapped in
    // `TOOLBAR_ROW_HEIGHT`-tall containers with
    // `align_y(Vertical::Center)`, so they line up regardless of
    // intrinsic widget heights. Always render the pick_list, even
    // when `options` is empty — empty shows blank, nothing expands.
    // The "No trackers configured." copy lives in the list-area body.
    let row_h = TOOLBAR_ROW_HEIGHT;
    let dropdown_inner = pick_list(options, selected_option, |opt: TrackerOption| {
        Message::TrackerBrowserSelectTracker(opt.id)
    })
    .text_size(TEXT_SIZE);
    let dropdown: Element<'a, Message> = container(dropdown_inner)
        .height(row_h)
        .align_y(Vertical::Center)
        .into();

    // Edit and Remove are conditionally enabled when a tracker is
    // selected; Refresh stays disabled until the network layer
    // lands. Built inline in the files-toolbar style:
    // `button(icon.size(ICON_SIZE)).padding(TOOLBAR_BUTTON_PADDING)`,
    // so each click target sizes to the icon's natural width and the
    // visual gap between buttons matches the files toolbar exactly.
    // Each is wrapped in a `TOOLBAR_ROW_HEIGHT`-tall container with
    // `align_y(Vertical::Center)` to align with the pulldown.
    // Tooltip only when enabled (matches files-toolbar convention —
    // disabled buttons don't surface a tooltip).
    let has_selection = state.selected_tracker.is_some();
    let inline_btn = |icon_widget: iced::widget::Text<'a>,
                      tooltip_key: &'static str,
                      on_press: Option<Message>|
     -> Element<'a, Message> {
        let inner: Element<'a, Message> = if let Some(msg) = on_press {
            tooltip(
                button(icon_widget.size(ICON_SIZE))
                    .on_press(msg)
                    .padding(TOOLBAR_BUTTON_PADDING)
                    .style(transparent_icon_button_style),
                container(shaped_text(t(tooltip_key)).size(TOOLTIP_TEXT_SIZE))
                    .padding(TOOLTIP_BACKGROUND_PADDING)
                    .style(tooltip_container_style),
                tooltip::Position::Bottom,
            )
            .gap(TOOLTIP_GAP)
            .padding(TOOLTIP_PADDING)
            .into()
        } else {
            button(icon_widget.size(ICON_SIZE))
                .padding(TOOLBAR_BUTTON_PADDING)
                .style(disabled_icon_button_style)
                .into()
        };
        container(inner)
            .height(row_h)
            .align_y(Vertical::Center)
            .into()
    };
    let edit_button = inline_btn(
        icon::edit(),
        "tooltip-tracker-edit",
        has_selection.then_some(Message::TrackerBrowserShowEdit),
    );
    let remove_button = inline_btn(
        icon::trash(),
        "tooltip-tracker-remove",
        has_selection.then_some(Message::TrackerBrowserShowRemove),
    );
    // Refresh stays disabled until step 5 wires the network layer.
    let refresh_button = inline_btn(icon::refresh(), "tooltip-refresh", None);

    let toolbar_row = row![
        dropdown,
        edit_button,
        remove_button,
        refresh_button,
        Space::new().width(Fill),
        toolbar_status(selected_cache),
    ]
    .spacing(SPACER_SIZE_SMALL)
    .align_y(Center);

    // -- Search row: live filter (no submit step).
    let search_input = text_input(&t("placeholder-tracker-search"), &state.search_input)
        .on_input(Message::TrackerBrowserSearchInputChanged)
        .id(Id::from(InputId::TrackerBrowserSearch))
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE)
        .width(Fill);

    // -- List body: dispatch on cache state, OR render the
    // "no trackers configured" empty-state when zero rows exist.
    // Empty/loading text is top-aligned (mirrors users / groups
    // tab pattern) — no vertical centering.
    let body: Element<'a, Message> = if has_trackers {
        list_body(state, selected_cache)
    } else {
        container(
            shaped_text(t("empty-no-trackers-configured"))
                .size(TEXT_SIZE)
                .style(muted_text_style),
        )
        .width(Fill)
        .center_x(Fill)
        .padding(SPACER_SIZE_SMALL)
        .into()
    };

    // Outer layout: symmetric `CONTENT_PADDING` + `CONTENT_MAX_WIDTH`,
    // standard `scrollable_panel` wrap. The list widget reserves its
    // own scrollbar gutter in the rightmost column, so the outer
    // container doesn't need to leave room for one.
    //
    // The body is wrapped in `container(...).height(Fill)` so it
    // absorbs the column's slack rather than spreading it across
    // the rows above.
    let form = column![
        title_row,
        toolbar_row,
        search_input,
        container(body).width(Fill).height(Fill),
    ]
    .spacing(SPACER_SIZE_SMALL)
    .align_x(Center)
    .padding(CONTENT_PADDING)
    .max_width(CONTENT_MAX_WIDTH)
    .height(Fill);

    let centered_form = container(form).width(Fill).center_x(Fill);

    container(centered_form)
        .width(Fill)
        .height(Fill)
        .style(content_background_style)
        .into()
}

// =============================================================================
// Add subview
// =============================================================================

/// Render the Add Tracker subview (full-panel form). Mirrors the
/// shape of the BBS-admin `views/trackers.rs::add_tracker_view`,
/// but bound to discovery-side messages and i18n keys.
fn add_tracker_view<'a>(state: &'a TrackerBrowserState) -> Element<'a, Message> {
    let title = panel_title(t("title-tracker-add"));

    let can_submit = !state.add_name.trim().is_empty()
        && !state.add_address.trim().is_empty()
        && !state.is_submitting;

    let submit_action = if can_submit {
        Message::TrackerBrowserAddSubmit
    } else {
        Message::TrackerBrowserValidateAdd
    };

    let mut items: Vec<Element<'a, Message>> = vec![title.into()];

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
        .on_input(Message::TrackerBrowserAddNameChanged)
        .on_submit(submit_action.clone())
        .id(Id::from(InputId::TrackerBrowserAddName))
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE)
        .width(Fill);
    items.push(
        row![
            shaped_text(t("label-name")).size(TEXT_SIZE),
            Space::new().width(ELEMENT_SPACING),
            name_input,
        ]
        .align_y(Center)
        .into(),
    );

    // Address
    let address_input = text_input(&t("placeholder-tracker-address"), &state.add_address)
        .on_input(Message::TrackerBrowserAddAddressChanged)
        .on_submit(submit_action.clone())
        .id(Id::from(InputId::TrackerBrowserAddAddress))
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE)
        .width(Fill);
    items.push(
        row![
            shaped_text(t("label-address")).size(TEXT_SIZE),
            Space::new().width(ELEMENT_SPACING),
            address_input,
        ]
        .align_y(Center)
        .into(),
    );

    // Port
    let port_input: Element<'a, Message> = NumberInput::new(
        &state.add_port,
        1..=u16::MAX,
        Message::TrackerBrowserAddPortChanged,
    )
    .id(Id::from(InputId::TrackerBrowserAddPort))
    .padding(INPUT_PADDING)
    .into();
    items.push(
        row![
            shaped_text(t("label-port")).size(TEXT_SIZE),
            Space::new().width(ELEMENT_SPACING),
            port_input,
        ]
        .align_y(Center)
        .into(),
    );

    // Password (optional)
    let password_input = text_input(&t("placeholder-tracker-password"), &state.add_password)
        .on_input(Message::TrackerBrowserAddPasswordChanged)
        .on_submit(submit_action.clone())
        .id(Id::from(InputId::TrackerBrowserAddPassword))
        .secure(true)
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE)
        .width(Fill);
    items.push(
        row![
            shaped_text(t("label-tracker-password")).size(TEXT_SIZE),
            Space::new().width(ELEMENT_SPACING),
            password_input,
        ]
        .align_y(Center)
        .into(),
    );

    // Fingerprint (optional)
    let fingerprint_input = text_input(
        &t("placeholder-tracker-fingerprint"),
        &state.add_fingerprint,
    )
    .on_input(Message::TrackerBrowserAddFingerprintChanged)
    .on_submit(submit_action)
    .id(Id::from(InputId::TrackerBrowserAddFingerprint))
    .padding(INPUT_PADDING)
    .size(TEXT_SIZE)
    .font(MONOSPACE_FONT)
    .width(Fill);
    items.push(
        row![
            shaped_text(t("label-fingerprint")).size(TEXT_SIZE),
            Space::new().width(ELEMENT_SPACING),
            fingerprint_input,
        ]
        .align_y(Center)
        .into(),
    );

    items.push(Space::new().height(SPACER_SIZE_MEDIUM).into());

    let cancel_button = button(shaped_text(t("button-cancel")).size(TEXT_SIZE))
        .on_press(Message::TrackerBrowserCancelMode)
        .padding(BUTTON_PADDING)
        .style(btn::secondary);

    let submit_button = if can_submit {
        button(shaped_text(t("button-save")).size(TEXT_SIZE))
            .on_press(Message::TrackerBrowserAddSubmit)
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

// =============================================================================
// Edit subview
// =============================================================================

/// Render the Edit Tracker subview. Pre-populated from the `Mode::Edit`
/// payload; all fields editable.
fn edit_tracker_view<'a>(state: &'a TrackerBrowserState) -> Element<'a, Message> {
    let TrackerBrowserMode::Edit {
        original_name,
        name,
        address,
        port,
        password,
        fingerprint,
        ..
    } = &state.mode
    else {
        // Caller guards against this; render an empty area as a
        // safety net rather than panicking.
        return Space::new().into();
    };

    let title = panel_title(t("title-tracker-edit"));
    let subtitle = shaped_text_wrapped(original_name.clone())
        .size(TEXT_SIZE)
        .width(Fill)
        .align_x(Center)
        .style(muted_text_style);

    let can_submit = !name.trim().is_empty() && !address.trim().is_empty() && !state.is_submitting;

    let submit_action = if can_submit {
        Message::TrackerBrowserEditSubmit
    } else {
        Message::TrackerBrowserValidateEdit
    };

    let mut items: Vec<Element<'a, Message>> = vec![title.into(), subtitle.into()];

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

    let name_input = text_input(&t("placeholder-tracker-name"), name)
        .on_input(Message::TrackerBrowserEditNameChanged)
        .on_submit(submit_action.clone())
        .id(Id::from(InputId::TrackerBrowserEditName))
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE)
        .width(Fill);
    items.push(
        row![
            shaped_text(t("label-name")).size(TEXT_SIZE),
            Space::new().width(ELEMENT_SPACING),
            name_input,
        ]
        .align_y(Center)
        .into(),
    );

    let address_input = text_input(&t("placeholder-tracker-address"), address)
        .on_input(Message::TrackerBrowserEditAddressChanged)
        .on_submit(submit_action.clone())
        .id(Id::from(InputId::TrackerBrowserEditAddress))
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE)
        .width(Fill);
    items.push(
        row![
            shaped_text(t("label-address")).size(TEXT_SIZE),
            Space::new().width(ELEMENT_SPACING),
            address_input,
        ]
        .align_y(Center)
        .into(),
    );

    let port_input: Element<'a, Message> =
        NumberInput::new(port, 1..=u16::MAX, Message::TrackerBrowserEditPortChanged)
            .id(Id::from(InputId::TrackerBrowserEditPort))
            .padding(INPUT_PADDING)
            .into();
    items.push(
        row![
            shaped_text(t("label-port")).size(TEXT_SIZE),
            Space::new().width(ELEMENT_SPACING),
            port_input,
        ]
        .align_y(Center)
        .into(),
    );

    let password_input = text_input(&t("placeholder-tracker-password"), password)
        .on_input(Message::TrackerBrowserEditPasswordChanged)
        .on_submit(submit_action.clone())
        .id(Id::from(InputId::TrackerBrowserEditPassword))
        .secure(true)
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE)
        .width(Fill);
    items.push(
        row![
            shaped_text(t("label-tracker-password")).size(TEXT_SIZE),
            Space::new().width(ELEMENT_SPACING),
            password_input,
        ]
        .align_y(Center)
        .into(),
    );

    let fingerprint_input = text_input(&t("placeholder-tracker-fingerprint"), fingerprint)
        .on_input(Message::TrackerBrowserEditFingerprintChanged)
        .on_submit(submit_action)
        .id(Id::from(InputId::TrackerBrowserEditFingerprint))
        .padding(INPUT_PADDING)
        .size(TEXT_SIZE)
        .font(MONOSPACE_FONT)
        .width(Fill);
    items.push(
        row![
            shaped_text(t("label-fingerprint")).size(TEXT_SIZE),
            Space::new().width(ELEMENT_SPACING),
            fingerprint_input,
        ]
        .align_y(Center)
        .into(),
    );

    items.push(Space::new().height(SPACER_SIZE_MEDIUM).into());

    let cancel_button = button(shaped_text(t("button-cancel")).size(TEXT_SIZE))
        .on_press(Message::TrackerBrowserCancelMode)
        .padding(BUTTON_PADDING)
        .style(btn::secondary);

    let submit_button = if can_submit {
        button(shaped_text(t("button-save")).size(TEXT_SIZE))
            .on_press(Message::TrackerBrowserEditSubmit)
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

// =============================================================================
// ConfirmRemove modal
// =============================================================================

fn remove_tracker_modal<'a>(state: &'a TrackerBrowserState) -> Element<'a, Message> {
    let TrackerBrowserMode::ConfirmRemove { name, .. } = &state.mode else {
        return Space::new().into();
    };

    let title = panel_title(t("dialog-remove-tracker-title"));
    let body_text = t_args("dialog-remove-client-tracker-body", &[("name", name)]);
    let body = shaped_text_wrapped(body_text)
        .size(TEXT_SIZE)
        .width(Fill)
        .align_x(Center);

    let cancel_button = button(shaped_text(t("button-cancel")).size(TEXT_SIZE))
        .on_press(Message::TrackerBrowserCancelMode)
        .padding(BUTTON_PADDING)
        .style(btn::secondary);

    let confirm_button = if !state.is_remove_submitting {
        button(shaped_text(t("button-remove")).size(TEXT_SIZE))
            .on_press(Message::TrackerBrowserRemoveConfirm)
            .padding(BUTTON_PADDING)
            .style(btn::danger)
    } else {
        button(shaped_text(t("button-remove")).size(TEXT_SIZE))
            .padding(BUTTON_PADDING)
            .style(btn::danger)
    };

    let mut items: Vec<Element<'a, Message>> = vec![title.into()];

    if let Some(error) = &state.remove_error {
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

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ClientTracker;

    fn make_tracker(name: &str) -> ClientTracker {
        ClientTracker {
            name: name.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn test_build_options_alphabetical_case_insensitive() {
        let trackers = vec![
            make_tracker("zeta"),
            make_tracker("Alpha"),
            make_tracker("beta"),
        ];
        let opts = build_options(&trackers);
        assert_eq!(opts.len(), 3);
        assert_eq!(opts[0].name, "Alpha");
        assert_eq!(opts[1].name, "beta");
        assert_eq!(opts[2].name, "zeta");
    }

    #[test]
    fn test_tracker_option_eq_by_id_and_name() {
        let id = Uuid::new_v4();
        let a = TrackerOption {
            id,
            name: "A".to_string(),
        };
        let b = TrackerOption {
            id,
            name: "A".to_string(),
        };
        assert_eq!(a, b);
    }
}
