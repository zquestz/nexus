//! Tracker discovery (Trackers) panel view.
//!
//! Mode dispatch happens in [`tracker_browser_view`]:
//!   - `List` → list view (title row + toolbar + search + body)
//!   - `Add` → [`add_tracker_view`]
//!   - `Edit` → [`edit_tracker_view`]
//!   - `ConfirmRemove` → [`remove_tracker_modal`]
//!   - `AcceptFingerprint` → fingerprint acceptance dialog
//!
//! In list mode the toolbar's `[+]` Add button is always enabled,
//! Edit and Remove are conditionally enabled when a tracker is
//! selected, and Refresh is enabled when a selected tracker is not already fetching.
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
use iced_aw::{ContextMenu, NumberInput};
use nexus_common::names::fold_name;
use nexus_common::tracker_protocol::ServerEntry;
use uuid::Uuid;

use super::fingerprint::format_fingerprint_multiline;
use super::helpers::{sort_icon_or_placeholder, t_args};
use super::layout::{scrollable_modal, scrollable_panel};
use crate::i18n::t;
use crate::icon;
use crate::style::{
    BUTTON_PADDING, CONTENT_MAX_WIDTH, CONTENT_PADDING, CONTEXT_MENU_ITEM_PADDING,
    CONTEXT_MENU_MIN_WIDTH, CONTEXT_MENU_PADDING, CONTEXT_MENU_SEPARATOR_MARGIN, ELEMENT_SPACING,
    FINGERPRINT_SPACE_AFTER_LABEL, FINGERPRINT_SPACE_AFTER_SERVER_INFO,
    FINGERPRINT_SPACE_AFTER_TITLE, FINGERPRINT_SPACE_AFTER_WARNING,
    FINGERPRINT_SPACE_BEFORE_BUTTONS, FINGERPRINT_SPACE_BETWEEN_SECTIONS, HEADING_BUTTON_PADDING,
    ICON_SIZE, INPUT_PADDING, MONOSPACE_FONT, NO_SPACING, SCROLLBAR_PADDING, SEPARATOR_HEIGHT,
    SORT_ICON_LEFT_MARGIN, SORT_ICON_RIGHT_MARGIN, SPACER_SIZE_MEDIUM, SPACER_SIZE_SMALL,
    TEXT_SIZE, TITLE_SIZE, TOOLBAR_BUTTON_PADDING, TOOLTIP_BACKGROUND_PADDING, TOOLTIP_GAP,
    TOOLTIP_PADDING, TOOLTIP_TEXT_SIZE, content_background_style, context_menu_container_style,
    disabled_icon_button_style, error_text_style, menu_button_style, muted_text_style, panel_title,
    shaped_text, shaped_text_wrapped, success_text_style, tooltip_container_style,
    transparent_icon_button_style,
};
use crate::types::{
    ClientTracker, InputId, Message, TrackerBrowserMode, TrackerBrowserSortColumn,
    TrackerBrowserState, TrackerCacheEntry,
};
use crate::widgets::MenuButton;

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
    options.sort_by_cached_key(|o| fold_name(&o.name));
    options
}

// =============================================================================
// Lazy server table
// =============================================================================

/// Dependencies the lazy-cached table hashes against. The row actions
/// capture endpoint and fingerprint fields, so the hash must cover the
/// complete entry data, not only visibly rendered columns.
#[derive(Clone)]
struct ServerTableDeps {
    entries: Vec<ServerEntry>,
    sort_column: TrackerBrowserSortColumn,
    sort_ascending: bool,
}

impl Hash for ServerTableDeps {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Hash every entry whole: the row's click and context-menu closures
        // capture all of `ServerEntry` (address, port, fingerprint, …), so an
        // endpoint or cert change with an unchanged name/user_count must still
        // invalidate the lazy cache.
        self.entries.hash(state);
        self.sort_column.hash(state);
        self.sort_ascending.hash(state);
    }
}

fn sort_entries(entries: &mut [ServerEntry], column: TrackerBrowserSortColumn, ascending: bool) {
    entries.sort_by(|a, b| {
        let cmp = match column {
            TrackerBrowserSortColumn::Name => fold_name(&a.name).cmp(&fold_name(&b.name)),
            TrackerBrowserSortColumn::Description => a
                .description
                .as_deref()
                .unwrap_or("")
                .to_lowercase()
                .cmp(&b.description.as_deref().unwrap_or("").to_lowercase()),
            TrackerBrowserSortColumn::Users => a.user_count.cmp(&b.user_count),
            TrackerBrowserSortColumn::Public => a.allows_guest.cmp(&b.allows_guest),
        };
        if ascending { cmp } else { cmp.reverse() }
    });
}

/// Filter entries by query (case-insensitive substring match against
/// name and description) and sort by the active column.
///
/// Empty / whitespace-only query → all entries, sorted.
/// Non-empty query → two-pass relevance: name-substring matches first,
/// then description-only matches; within each group the active column
/// sort applies. Entries matching neither name nor description are
/// dropped.
fn filter_and_sort_entries(
    entries: &[ServerEntry],
    query: &str,
    column: TrackerBrowserSortColumn,
    ascending: bool,
) -> Vec<ServerEntry> {
    if query.trim().is_empty() {
        let mut all: Vec<ServerEntry> = entries.to_vec();
        sort_entries(&mut all, column, ascending);
        return all;
    }
    let needle = query.to_lowercase();
    let mut name_matches: Vec<ServerEntry> = Vec::new();
    let mut desc_only: Vec<ServerEntry> = Vec::new();
    for entry in entries {
        if entry.name.to_lowercase().contains(&needle) {
            name_matches.push(entry.clone());
            continue;
        }
        if entry
            .description
            .as_deref()
            .is_some_and(|d| d.to_lowercase().contains(&needle))
        {
            desc_only.push(entry.clone());
        }
    }
    sort_entries(&mut name_matches, column, ascending);
    sort_entries(&mut desc_only, column, ascending);
    name_matches.extend(desc_only);
    name_matches
}

/// Build the row context menu for a tracker discovery entry. Three
/// items, no separators (similar-weight actions, no destructive item
/// to set apart):
///   - Connect — same as left-clicking Name
///   - Bookmark — direct-create with blank credentials
///   - Copy URI — clipboard `nexus://address:port`
fn build_row_context_menu(entry: ServerEntry) -> Element<'static, Message> {
    let connect_msg = Message::OpenConnectFromTracker {
        name: entry.name.clone(),
        address: entry.address.clone(),
        port: entry.port,
        fingerprint: entry.fingerprint.clone(),
    };
    let bookmark_msg = Message::TrackerBrowserBookmarkRow {
        name: entry.name,
        address: entry.address.clone(),
        port: entry.port,
        fingerprint: entry.fingerprint,
    };
    let copy_uri_msg = Message::TrackerBrowserCopyRowUri {
        address: entry.address,
        port: entry.port,
    };

    let items: Vec<Element<'static, Message>> = vec![
        MenuButton::new(shaped_text(t("menu-tracker-connect")).size(TEXT_SIZE))
            .padding(CONTEXT_MENU_ITEM_PADDING)
            .width(Fill)
            .style(menu_button_style)
            .on_press(connect_msg)
            .into(),
        MenuButton::new(shaped_text(t("menu-tracker-bookmark")).size(TEXT_SIZE))
            .padding(CONTEXT_MENU_ITEM_PADDING)
            .width(Fill)
            .style(menu_button_style)
            .on_press(bookmark_msg)
            .into(),
        MenuButton::new(shaped_text(t("menu-tracker-copy-uri")).size(TEXT_SIZE))
            .padding(CONTEXT_MENU_ITEM_PADDING)
            .width(Fill)
            .style(menu_button_style)
            .on_press(copy_uri_msg)
            .into(),
    ];

    container(iced::widget::Column::with_children(items).spacing(CONTEXT_MENU_SEPARATOR_MARGIN))
        .width(CONTEXT_MENU_MIN_WIDTH)
        .padding(CONTEXT_MENU_PADDING)
        .style(context_menu_container_style)
        .into()
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

        // Name cell is clickable: launches the Connect form pre-filled
        // with this row's four fields. Style mirrors files-listing
        // clickable filenames — transparent button background, hover
        // colour from the theme, no underline. The cell is also
        // wrapped in a `ContextMenu` exposing Connect / Bookmark /
        // Copy URI on right-click.
        let name_column = table::column(
            name_header,
            |entry: ServerEntry| -> Element<'static, Message> {
                let label = shaped_text(entry.name.clone())
                    .size(TEXT_SIZE)
                    .wrapping(Wrapping::WordOrGlyph);
                let connect_message = Message::OpenConnectFromTracker {
                    name: entry.name.clone(),
                    address: entry.address.clone(),
                    port: entry.port,
                    fingerprint: entry.fingerprint.clone(),
                };
                let cell: Element<'static, Message> = button(label)
                    .padding(NO_SPACING)
                    .style(transparent_icon_button_style)
                    .on_press(connect_message)
                    .into();
                let menu_entry = entry.clone();
                ContextMenu::new(cell, move || build_row_context_menu(menu_entry.clone())).into()
            },
        )
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
        .width(Length::FillPortion(2));

        // Users header (sortable). Scrollbar padding lives on the
        // rightmost column (Public, below) per the table-consistency
        // convention.
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
            shaped_text(entry.user_count.to_string()).size(TEXT_SIZE)
        })
        .width(Length::Shrink);

        // Public header (sortable, rightmost — trailing scrollbar padding).
        // Indicates whether the server allows guest login (`allows_guest`).
        // Cell renders `icon::ok()` (✓ in success colour) for true,
        // `icon::close()` (✕ in default colour) for false.
        let public_sort_icon = sort_icon_or_placeholder(
            deps.sort_column == TrackerBrowserSortColumn::Public,
            deps.sort_ascending,
        );
        let public_header_content: Element<'static, Message> = row![
            shaped_text(t("col-public"))
                .size(TEXT_SIZE)
                .wrapping(Wrapping::Word)
                .style(muted_text_style),
            Space::new().width(SORT_ICON_LEFT_MARGIN),
            public_sort_icon,
            Space::new().width(SORT_ICON_RIGHT_MARGIN),
            Space::new().width(SCROLLBAR_PADDING),
        ]
        .align_y(Center)
        .into();
        let public_header: Element<'static, Message> = button(public_header_content)
            .padding(NO_SPACING)
            .width(Length::Shrink)
            .style(transparent_icon_button_style)
            .on_press(Message::TrackerBrowserSortChanged(
                TrackerBrowserSortColumn::Public,
            ))
            .into();

        let public_column = table::column(public_header, |entry: ServerEntry| {
            let (glyph, tip_key): (iced::widget::Text<'static>, &'static str) =
                if entry.allows_guest {
                    (
                        icon::ok().size(TEXT_SIZE).style(success_text_style),
                        "tooltip-server-public-yes",
                    )
                } else {
                    (icon::close().size(TEXT_SIZE), "tooltip-server-public-no")
                };
            let tipped: Element<'static, Message> = tooltip(
                glyph,
                container(shaped_text(t(tip_key)).size(TOOLTIP_TEXT_SIZE))
                    .padding(TOOLTIP_BACKGROUND_PADDING)
                    .style(tooltip_container_style),
                tooltip::Position::Top,
            )
            .gap(TOOLTIP_GAP)
            .padding(TOOLTIP_PADDING)
            .into();
            row![tipped, Space::new().width(SCROLLBAR_PADDING)].align_y(Center)
        })
        .width(Length::Shrink);

        let columns = [name_column, desc_column, users_column, public_column];

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
/// loading; neutral on success (server count — reflects the *visible*
/// count, i.e. post-filter, so it matches what the user sees in the
/// table). Errors are rendered in a dedicated row under the panel
/// title — see `error_row` below.
fn toolbar_status<'a>(
    cache: Option<&'a TrackerCacheEntry>,
    filtered_count: Option<usize>,
) -> Element<'a, Message> {
    let Some(entry) = cache else {
        return Space::new().into();
    };
    if entry.is_fetching && entry.entries.is_none() {
        return shaped_text(t("tracker-browser-status-loading"))
            .size(TEXT_SIZE)
            .style(muted_text_style)
            .into();
    }
    if let Some(count) = filtered_count {
        let count_str = count.to_string();
        let label = t_args("tracker-browser-status-servers", &[("count", &count_str)]);
        return shaped_text(label).size(TEXT_SIZE).into();
    }
    Space::new().into()
}

// =============================================================================
// List-area body
// =============================================================================

enum TrackerListBody {
    NoSelectionOrCache,
    Entries {
        filtered: Vec<ServerEntry>,
        search_active: bool,
    },
    Loading,
    Empty,
}

impl TrackerListBody {
    fn filtered_count(&self) -> Option<usize> {
        match self {
            Self::Entries { filtered, .. } => Some(filtered.len()),
            Self::NoSelectionOrCache | Self::Loading | Self::Empty => None,
        }
    }
}

fn build_list_body_state(
    state: &TrackerBrowserState,
    selected_cache: Option<&TrackerCacheEntry>,
) -> TrackerListBody {
    let Some(cache) = selected_cache.filter(|_| state.selected_tracker.is_some()) else {
        // No tracker selected, or no fetch has ever occurred for the selected
        // tracker — render an empty area. The network layer populates the
        // cache on selection change once it lands.
        return TrackerListBody::NoSelectionOrCache;
    };

    if let Some(entries) = cache.entries.as_ref() {
        return TrackerListBody::Entries {
            filtered: filter_and_sort_entries(
                entries,
                &state.search_input,
                state.sort_column,
                state.sort_ascending,
            ),
            search_active: !state.search_input.trim().is_empty(),
        };
    }

    if cache.is_fetching {
        TrackerListBody::Loading
    } else {
        TrackerListBody::Empty
    }
}

/// Decide what fills the list-area below the search row, per the
/// cache-state matrix in the spec:
///
/// | `entries`  | `is_fetching` | renders                              |
/// |------------|---------------|--------------------------------------|
/// | `Some(_)`  | any           | the table (last successful fetch)    |
/// | `None`     | `true`        | loading indicator                    |
/// | `None`     | `false`       | empty state (initial / never fetched)|
fn list_body<'a>(
    body: TrackerListBody,
    sort_column: TrackerBrowserSortColumn,
    sort_ascending: bool,
) -> Element<'a, Message> {
    match body {
        TrackerListBody::NoSelectionOrCache => Space::new().into(),
        TrackerListBody::Entries {
            filtered,
            search_active,
        } => {
            if filtered.is_empty() {
                // Two distinct empty messages:
                //   - listing has zero entries -> "this tracker has no
                //     registered servers"
                //   - listing has entries but search filtered all of them
                //     out -> "no matches"
                let key = if search_active {
                    "empty-tracker-no-matches"
                } else {
                    "empty-tracker-no-servers"
                };
                container(shaped_text(t(key)).size(TEXT_SIZE).style(muted_text_style))
                    .width(Fill)
                    .center_x(Fill)
                    .padding(SPACER_SIZE_SMALL)
                    .into()
            } else {
                let deps = ServerTableDeps {
                    entries: filtered,
                    sort_column,
                    sort_ascending,
                };
                scrollable(lazy_server_table(deps)).height(Fill).into()
            }
        }
        TrackerListBody::Loading => container(
            shaped_text(t("tracker-browser-status-loading"))
                .size(TEXT_SIZE)
                .style(muted_text_style),
        )
        .width(Fill)
        .center_x(Fill)
        .padding(SPACER_SIZE_SMALL)
        .into(),
        TrackerListBody::Empty => {
            // None + !is_fetching — initial / never fetched. Render an empty
            // space; the toolbar status text already communicates "nothing yet".
            Space::new().into()
        }
    }
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
        TrackerBrowserMode::List => list_view(state, trackers),
        TrackerBrowserMode::Add => add_tracker_view(state),
        TrackerBrowserMode::Edit { .. } => edit_tracker_view(state),
        TrackerBrowserMode::ConfirmRemove { .. } => remove_tracker_modal(state),
        TrackerBrowserMode::AcceptFingerprint { .. } => accept_fingerprint_modal(state),
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

    // Compute the cache-derived body state once and reuse its count for the
    // toolbar, so "Servers: N" matches what the user actually sees.
    let body_state = build_list_body_state(state, selected_cache);
    let filtered_count = body_state.filtered_count();

    // -- Title row: news-style centered title with a balanced [+]
    // button on the right. The invisible spacer on the left equals the
    // button's rendered width so the title sits geometrically centered
    // even with the action icon next to it.
    let add_icon = container(icon::plus().size(ICON_SIZE))
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
    // Refresh enabled when a tracker is selected and no query is in
    // flight for it. The handler also stale-drops re-entered presses
    // as a defense-in-depth, but the disabled visual is the primary
    // UX signal.
    let is_fetching = selected_cache.is_some_and(|c| c.is_fetching);
    let refresh_button = inline_btn(
        icon::refresh(),
        "tooltip-refresh",
        (has_selection && !is_fetching).then_some(Message::TrackerBrowserRefresh),
    );

    let toolbar_row = row![
        dropdown,
        edit_button,
        remove_button,
        refresh_button,
        Space::new().width(Fill),
        toolbar_status(selected_cache, filtered_count),
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
        list_body(body_state, state.sort_column, state.sort_ascending)
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
    //
    // The error banner (when present) is conditionally pushed
    // between title and toolbar, mirroring the files-view pattern:
    // wrapped in a container with a `SPACER_SIZE_MEDIUM` bottom
    // padding so there's clear breathing room before the toolbar
    // row. Same row is omitted entirely when there's no error so
    // there's no leftover spacer.
    let mut form_column = column![title_row]
        .spacing(SPACER_SIZE_SMALL)
        .align_x(Center);
    if let Some(error) = selected_cache.and_then(|c| c.error.as_ref()) {
        form_column = form_column.push(
            shaped_text_wrapped(error.as_str())
                .size(TEXT_SIZE)
                .width(Fill)
                .align_x(Center)
                .style(error_text_style),
        );
        form_column = form_column.push(Space::new().height(SPACER_SIZE_MEDIUM));
    }
    let form = form_column
        .push(toolbar_row)
        .push(search_input)
        .push(container(body).width(Fill).height(Fill))
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
// AcceptFingerprint modal
// =============================================================================

/// Stage-1 TOFU mismatch dialog. Mirrors `views/trackers.rs::
/// accept_fingerprint_modal` (the BBS-admin tracker panel) but bound
/// to discovery-side messages (`TrackerBrowserAcceptFingerprint*`)
/// so the two flows don't collide. Identity line + warning + expected
/// vs received fingerprints in monospace + Cancel/Accept buttons.
fn accept_fingerprint_modal<'a>(state: &'a TrackerBrowserState) -> Element<'a, Message> {
    let TrackerBrowserMode::AcceptFingerprint {
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
    let expected_value: Element<'a, Message> = if let Some(fp) = expected.as_ref() {
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
    .on_press(Message::TrackerBrowserAcceptFingerprintCancel)
    .padding(BUTTON_PADDING)
    .style(btn::secondary);

    let accept_button = if !state.is_accept_fingerprint_submitting {
        button(
            shaped_text(t("button-accept-fingerprint"))
                .size(TEXT_SIZE)
                .width(Length::Fill)
                .center(),
        )
        .on_press(Message::TrackerBrowserAcceptFingerprintConfirm)
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

    let mut items: Vec<Element<'a, Message>> = vec![title.into()];

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

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::hash_map::DefaultHasher;

    use crate::types::ClientTracker;

    fn make_tracker(name: &str) -> ClientTracker {
        ClientTracker {
            name: name.to_string(),
            ..Default::default()
        }
    }

    fn sample_entry() -> ServerEntry {
        ServerEntry {
            name: "Nexus".to_string(),
            description: Some("A board".to_string()),
            address: "bbs.example".to_string(),
            port: 7500,
            websocket_port: Some(7502),
            version: "0.9.3".to_string(),
            fingerprint: "AA:BB".to_string(),
            user_count: 3,
            allows_guest: true,
        }
    }

    fn deps_hash_with(
        entries: Vec<ServerEntry>,
        sort_column: TrackerBrowserSortColumn,
        sort_ascending: bool,
    ) -> u64 {
        let deps = ServerTableDeps {
            entries,
            sort_column,
            sort_ascending,
        };
        let mut hasher = DefaultHasher::new();
        deps.hash(&mut hasher);
        hasher.finish()
    }

    fn deps_hash(entries: Vec<ServerEntry>) -> u64 {
        deps_hash_with(entries, TrackerBrowserSortColumn::Name, true)
    }

    #[test]
    fn server_table_deps_hash_stable_when_unchanged() {
        assert_eq!(
            deps_hash(vec![sample_entry()]),
            deps_hash(vec![sample_entry()])
        );
    }

    #[test]
    fn server_table_deps_hash_changes_on_action_affecting_fields() {
        let base = deps_hash(vec![sample_entry()]);
        for mutate in [
            (|e: &mut ServerEntry| e.name = "Other".to_string()) as fn(&mut ServerEntry),
            |e| e.description = Some("Changed".to_string()),
            (|e: &mut ServerEntry| e.fingerprint = "CC:DD".to_string()) as fn(&mut ServerEntry),
            |e| e.address = "other.example".to_string(),
            |e| e.port = 7600,
            |e| e.websocket_port = Some(8502),
            |e| e.version = "0.9.4".to_string(),
            |e| e.user_count = 7,
            |e| e.allows_guest = false,
        ] {
            let mut entry = sample_entry();
            mutate(&mut entry);
            assert_ne!(deps_hash(vec![entry]), base);
        }
    }

    #[test]
    fn server_table_deps_hash_changes_on_sort_state() {
        let base = deps_hash(vec![sample_entry()]);
        assert_ne!(
            deps_hash_with(vec![sample_entry()], TrackerBrowserSortColumn::Users, true,),
            base
        );
        assert_ne!(
            deps_hash_with(vec![sample_entry()], TrackerBrowserSortColumn::Name, false),
            base
        );
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

    // =========================================================================
    // sort_entries / filter_and_sort_entries
    // =========================================================================

    fn make_entry(name: &str, description: Option<&str>, user_count: u32) -> ServerEntry {
        ServerEntry {
            name: name.to_string(),
            description: description.map(str::to_string),
            address: "example.com".to_string(),
            port: 7500,
            websocket_port: None,
            version: "0.8.0".to_string(),
            fingerprint: String::new(),
            user_count,
            allows_guest: false,
        }
    }

    #[test]
    fn sort_entries_by_name_case_insensitive_ascending() {
        let mut entries = vec![
            make_entry("zeta", None, 0),
            make_entry("Alpha", None, 0),
            make_entry("beta", None, 0),
        ];
        sort_entries(&mut entries, TrackerBrowserSortColumn::Name, true);
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["Alpha", "beta", "zeta"]);
    }

    #[test]
    fn sort_entries_by_name_descending_reverses() {
        let mut entries = vec![make_entry("a", None, 0), make_entry("b", None, 0)];
        sort_entries(&mut entries, TrackerBrowserSortColumn::Name, false);
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["b", "a"]);
    }

    #[test]
    fn sort_entries_by_users_orders_numerically() {
        let mut entries = vec![
            make_entry("a", None, 100),
            make_entry("b", None, 5),
            make_entry("c", None, 50),
        ];
        sort_entries(&mut entries, TrackerBrowserSortColumn::Users, true);
        let counts: Vec<u32> = entries.iter().map(|e| e.user_count).collect();
        assert_eq!(counts, vec![5, 50, 100]);
    }

    #[test]
    fn sort_entries_by_public_orders_by_allows_guest() {
        let mut entries = vec![
            ServerEntry {
                allows_guest: true,
                ..make_entry("a", None, 0)
            },
            ServerEntry {
                allows_guest: false,
                ..make_entry("b", None, 0)
            },
            ServerEntry {
                allows_guest: true,
                ..make_entry("c", None, 0)
            },
        ];
        sort_entries(&mut entries, TrackerBrowserSortColumn::Public, true);
        // Bool ordering: false < true. Ascending → "b" first, then a/c.
        assert!(!entries[0].allows_guest);
        assert!(entries[1].allows_guest);
        assert!(entries[2].allows_guest);
    }

    #[test]
    fn sort_entries_by_description_treats_missing_as_empty() {
        let mut entries = vec![
            make_entry("a", Some("zebras"), 0),
            make_entry("b", None, 0),
            make_entry("c", Some("apples"), 0),
        ];
        sort_entries(&mut entries, TrackerBrowserSortColumn::Description, true);
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        // Empty description sorts first (lexicographically before any letter).
        assert_eq!(names, vec!["b", "c", "a"]);
    }

    #[test]
    fn filter_and_sort_empty_query_returns_all_sorted() {
        let entries = vec![
            make_entry("zulu", None, 0),
            make_entry("alpha", None, 0),
            make_entry("mike", None, 0),
        ];
        let out = filter_and_sort_entries(&entries, "", TrackerBrowserSortColumn::Name, true);
        let names: Vec<&str> = out.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["alpha", "mike", "zulu"]);
    }

    #[test]
    fn filter_and_sort_whitespace_query_treated_as_empty() {
        let entries = vec![make_entry("alpha", None, 0), make_entry("beta", None, 0)];
        let out = filter_and_sort_entries(&entries, "   ", TrackerBrowserSortColumn::Name, true);
        let names: Vec<&str> = out.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["alpha", "beta"]);
    }

    #[test]
    fn filter_and_sort_case_insensitive_unicode_match() {
        // IDN safety: a CJK / Cyrillic / Latin-extended needle should
        // case-fold via `to_lowercase()`, not ASCII-only helpers.
        let entries = vec![
            make_entry("München BBS", None, 0),
            make_entry("Other", None, 0),
        ];
        let out =
            filter_and_sort_entries(&entries, "MÜNCHEN", TrackerBrowserSortColumn::Name, true);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].name, "München BBS");
    }

    #[test]
    fn filter_and_sort_whitespace_is_part_of_the_query() {
        // After the empty-query gate, the needle is the raw query — so " alpha "
        // matches a name that actually contains those spaces, not a bare "alpha".
        let entries = vec![make_entry("alpha", None, 0), make_entry(" alpha ", None, 0)];
        let out =
            filter_and_sort_entries(&entries, " alpha ", TrackerBrowserSortColumn::Name, true);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].name, " alpha ");
    }

    #[test]
    fn filter_and_sort_name_match_precedes_description_only_match() {
        let entries = vec![
            make_entry("nothing relevant", Some("we host pizza here"), 0),
            make_entry("pizza", Some("the best"), 0),
            make_entry("other", None, 0),
        ];
        let out = filter_and_sort_entries(&entries, "pizza", TrackerBrowserSortColumn::Name, true);
        let names: Vec<&str> = out.iter().map(|e| e.name.as_str()).collect();
        // Name-substring hit "pizza" comes first; description-only hit
        // "nothing relevant" comes after. "other" is dropped.
        assert_eq!(names, vec!["pizza", "nothing relevant"]);
    }

    #[test]
    fn filter_and_sort_description_only_match_surfaces() {
        let entries = vec![
            make_entry("Apple Server", Some("welcome to apple"), 0),
            make_entry("Other", Some("an apple a day"), 0),
            make_entry("Unrelated", None, 0),
        ];
        let out = filter_and_sort_entries(&entries, "apple", TrackerBrowserSortColumn::Name, true);
        let names: Vec<&str> = out.iter().map(|e| e.name.as_str()).collect();
        // Both name- and description-only matches surface; "Unrelated" dropped.
        assert_eq!(names, vec!["Apple Server", "Other"]);
    }

    #[test]
    fn filter_and_sort_no_match_returns_empty_vec() {
        let entries = vec![make_entry("alpha", Some("beta"), 0)];
        let out = filter_and_sort_entries(&entries, "gamma", TrackerBrowserSortColumn::Name, true);
        assert!(out.is_empty());
    }

    #[test]
    fn filter_and_sort_preserves_column_sort_within_each_match_group() {
        let entries = vec![
            make_entry("alpha pizza", None, 100),
            make_entry("zeta pizza", None, 5),
            make_entry("beta", Some("we serve pizza"), 50),
            make_entry("yankee", Some("our pizza is great"), 200),
        ];
        let out = filter_and_sort_entries(&entries, "pizza", TrackerBrowserSortColumn::Users, true);
        let names: Vec<&str> = out.iter().map(|e| e.name.as_str()).collect();
        // Group A (name match): zeta(5) then alpha(100), users-asc.
        // Group B (description match): beta(50) then yankee(200), users-asc.
        // Concatenated: A then B.
        assert_eq!(names, vec!["zeta pizza", "alpha pizza", "beta", "yankee"]);
    }
}
