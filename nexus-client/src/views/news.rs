//! News management panel view (list, create, edit, delete news posts)

use chrono::{DateTime, Local, Utc};

use super::constants::{PERMISSION_NEWS_CREATE, PERMISSION_NEWS_DELETE, PERMISSION_NEWS_EDIT};
use super::layout::{scrollable_modal, scrollable_panel};
use crate::i18n::t;
use crate::icon;
use crate::image::CachedImage;
use crate::style::{
    BUTTON_PADDING, ELEMENT_SPACING, FORM_MAX_WIDTH, FORM_PADDING, ICON_BUTTON_PADDING,
    INPUT_PADDING, NEWS_ACTION_BUTTON_SIZE, NEWS_ACTION_ICON_SIZE, NEWS_EDITOR_LINE_HEIGHT,
    NEWS_IMAGE_PREVIEW_SIZE, NEWS_ITEM_SPACING, NEWS_LIST_MAX_WIDTH, NO_SPACING, SCROLLBAR_PADDING,
    SIDEBAR_ACTION_ICON_SIZE, SPACER_SIZE_MEDIUM, SPACER_SIZE_SMALL, TEXT_SIZE, TITLE_SIZE,
    TOOLTIP_BACKGROUND_PADDING, TOOLTIP_GAP, TOOLTIP_PADDING, TOOLTIP_TEXT_SIZE,
    alternating_row_style, chat, content_background_style, danger_icon_button_style,
    error_text_style, muted_text_style, shaped_text, shaped_text_wrapped, tooltip_container_style,
    transparent_icon_button_style,
};
use crate::types::{InputId, Message, NewsManagementMode, NewsManagementState, ServerConnection};
use iced::widget::Id;
use iced::widget::button as btn;
use iced::widget::markdown;
use iced::widget::{
    Column, Row, Space, button, column, container, image, row, scrollable, svg, text_editor,
    tooltip,
};
use iced::{Center, Element, Fill, Length, Theme, alignment};
use nexus_common::protocol::NewsItem;
use std::collections::HashMap;

// ============================================================================
// Helper Functions
// ============================================================================

/// Helper function to create transparent edit icon buttons for news items
fn news_edit_button(icon: iced::widget::Text<'_>, message: Message) -> button::Button<'_, Message> {
    button(icon.size(NEWS_ACTION_ICON_SIZE))
        .on_press(message)
        .width(NEWS_ACTION_BUTTON_SIZE)
        .height(NEWS_ACTION_BUTTON_SIZE)
        .style(transparent_icon_button_style)
}

/// Helper function to create danger icon buttons for news items (delete)
fn news_delete_button(
    icon: iced::widget::Text<'_>,
    message: Message,
) -> button::Button<'_, Message> {
    button(icon.size(NEWS_ACTION_ICON_SIZE))
        .on_press(message)
        .width(NEWS_ACTION_BUTTON_SIZE)
        .height(NEWS_ACTION_BUTTON_SIZE)
        .style(danger_icon_button_style)
}

/// Format a timestamp for display
fn format_timestamp(iso_timestamp: &str) -> String {
    // Parse ISO 8601 timestamp and convert to local time
    if let Ok(utc_time) = DateTime::parse_from_rfc3339(iso_timestamp) {
        let local_time: DateTime<Local> = utc_time.with_timezone(&Local);
        // Format as "Jan 15, 2025 10:30"
        local_time.format("%b %d, %Y %H:%M").to_string()
    } else if let Ok(utc_time) = iso_timestamp.parse::<DateTime<Utc>>() {
        let local_time: DateTime<Local> = utc_time.with_timezone(&Local);
        local_time.format("%b %d, %Y %H:%M").to_string()
    } else {
        // Fallback: just show the raw timestamp but truncated
        iso_timestamp.chars().take(19).collect()
    }
}

/// Check if the current user can edit this news item
fn can_edit_news_item(news_item: &NewsItem, conn: &ServerConnection) -> bool {
    let is_own_post = news_item.author.to_lowercase() == conn.username.to_lowercase();
    let has_edit_perm = conn.has_permission(PERMISSION_NEWS_EDIT);
    let is_admin_post = news_item.author_is_admin;

    // Can edit if: own post OR (has edit permission AND (not admin post OR user is admin))
    is_own_post || (has_edit_perm && (!is_admin_post || conn.is_admin))
}

/// Check if the current user can delete this news item
fn can_delete_news_item(news_item: &NewsItem, conn: &ServerConnection) -> bool {
    let is_own_post = news_item.author.to_lowercase() == conn.username.to_lowercase();
    let has_delete_perm = conn.has_permission(PERMISSION_NEWS_DELETE);
    let is_admin_post = news_item.author_is_admin;

    // Can delete if: own post OR (has delete permission AND (not admin post OR user is admin))
    is_own_post || (has_delete_perm && (!is_admin_post || conn.is_admin))
}

/// Render a cached image
fn render_cached_image<'a>(cached: &CachedImage) -> Element<'a, Message> {
    match cached {
        CachedImage::Raster(handle) => image(handle.clone())
            .width(Length::Fill)
            .content_fit(iced::ContentFit::ScaleDown)
            .into(),
        CachedImage::Svg(handle) => svg(handle.clone())
            .width(Length::Fill)
            .content_fit(iced::ContentFit::ScaleDown)
            .into(),
    }
}

/// Render a cached image for preview (smaller size)
fn render_cached_image_preview<'a>(cached: &CachedImage) -> Element<'a, Message> {
    match cached {
        CachedImage::Raster(handle) => image(handle.clone())
            .width(NEWS_IMAGE_PREVIEW_SIZE)
            .height(NEWS_IMAGE_PREVIEW_SIZE)
            .content_fit(iced::ContentFit::ScaleDown)
            .into(),
        CachedImage::Svg(handle) => svg(handle.clone())
            .width(NEWS_IMAGE_PREVIEW_SIZE)
            .height(NEWS_IMAGE_PREVIEW_SIZE)
            .content_fit(iced::ContentFit::ScaleDown)
            .into(),
    }
}

// ============================================================================
// List View
// ============================================================================

/// Build the news list view
fn list_view<'a>(
    conn: &'a ServerConnection,
    news_management: &'a NewsManagementState,
    theme: &Theme,
    news_image_cache: &'a HashMap<i64, CachedImage>,
) -> Element<'a, Message> {
    // Check permissions
    let can_create = conn.has_permission(PERMISSION_NEWS_CREATE);

    // Build scrollable content (news list or status message)
    let scroll_content_inner: Element<'a, Message> = match &news_management.news_items {
        None => {
            // Loading state
            shaped_text(t("news-loading"))
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
        Some(Ok(items)) => {
            if items.is_empty() {
                shaped_text(t("news-no-posts"))
                    .size(TEXT_SIZE)
                    .width(Fill)
                    .align_x(Center)
                    .style(muted_text_style)
                    .into()
            } else {
                // Build news item rows (newest first for display)
                let mut news_rows = Column::new().spacing(NEWS_ITEM_SPACING);

                // Reverse to show newest first (server returns oldest first)
                for (index, item) in items.iter().rev().enumerate() {
                    let news_row = build_news_item_row(
                        item,
                        conn,
                        theme,
                        index,
                        news_image_cache,
                        &conn.news_markdown_cache,
                    );
                    news_rows = news_rows.push(news_row);
                }

                news_rows.width(Fill).into()
            }
        }
    };

    let scroll_content = scroll_content_inner;

    // Create post button (icon style like add bookmark)
    let create_btn: Option<Element<'a, Message>> = if can_create {
        let add_icon = container(icon::plus().size(SIDEBAR_ACTION_ICON_SIZE))
            .width(SIDEBAR_ACTION_ICON_SIZE)
            .height(SIDEBAR_ACTION_ICON_SIZE)
            .align_x(alignment::Horizontal::Center)
            .align_y(alignment::Vertical::Center);

        Some(
            tooltip(
                button(add_icon)
                    .on_press(Message::NewsShowCreate)
                    .padding(ICON_BUTTON_PADDING)
                    .style(transparent_icon_button_style),
                container(shaped_text(t("tooltip-create-news")).size(TOOLTIP_TEXT_SIZE))
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

    // Title row with create button on the right
    // We add an invisible spacer on the left to balance the button width for proper centering
    let button_width =
        SIDEBAR_ACTION_ICON_SIZE + ICON_BUTTON_PADDING.left + ICON_BUTTON_PADDING.right;
    let title_row: Element<'a, Message> = if let Some(create_btn) = create_btn {
        row![
            Space::new().width(SCROLLBAR_PADDING),
            Space::new().width(button_width), // Balance the create button on the right
            shaped_text(t("title-news"))
                .size(TITLE_SIZE)
                .width(Fill)
                .align_x(Center),
            create_btn,
            Space::new().width(SCROLLBAR_PADDING),
        ]
        .align_y(Center)
        .into()
    } else {
        row![
            Space::new().width(SCROLLBAR_PADDING),
            shaped_text(t("title-news"))
                .size(TITLE_SIZE)
                .width(Fill)
                .align_x(Center),
            Space::new().width(SCROLLBAR_PADDING),
        ]
        .into()
    };

    // Error message (shown below title if present)
    let error_element: Option<Element<'a, Message>> =
        news_management.list_error.as_ref().map(|error| {
            shaped_text_wrapped(error)
                .size(TEXT_SIZE)
                .width(Fill)
                .align_x(Center)
                .style(error_text_style)
                .into()
        });

    // Scrollable content with symmetric padding for scrollbar space
    let padded_scroll_content = row![
        Space::new().width(SCROLLBAR_PADDING),
        container(scroll_content).width(Fill),
        Space::new().width(SCROLLBAR_PADDING),
    ];

    // Build the form with max_width constraint
    let form = column![
        title_row,
        if let Some(err) = error_element {
            Element::from(err)
        } else {
            Element::from(Space::new().height(SPACER_SIZE_SMALL))
        },
        container(scrollable(padded_scroll_content)).height(Fill),
    ]
    .spacing(ELEMENT_SPACING)
    .align_x(Center)
    .padding(iced::Padding {
        top: FORM_PADDING,
        right: FORM_PADDING - SCROLLBAR_PADDING,
        bottom: FORM_PADDING,
        left: FORM_PADDING - SCROLLBAR_PADDING,
    })
    .max_width(NEWS_LIST_MAX_WIDTH + SCROLLBAR_PADDING * 2.0)
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

/// Build a single news item row
fn build_news_item_row<'a>(
    item: &'a NewsItem,
    conn: &'a ServerConnection,
    theme: &Theme,
    index: usize,
    news_image_cache: &'a HashMap<i64, CachedImage>,
    news_markdown_cache: &'a HashMap<i64, Vec<markdown::Item>>,
) -> Element<'a, Message> {
    let admin_color = chat::admin(theme);

    let author_text = if item.author_is_admin {
        shaped_text(&item.author).size(TEXT_SIZE).color(admin_color)
    } else {
        shaped_text(&item.author).size(TEXT_SIZE)
    };

    // Action buttons (right-aligned)
    let can_edit = can_edit_news_item(item, conn);
    let can_delete = can_delete_news_item(item, conn);

    let mut action_row = Row::new().spacing(NO_SPACING);

    if can_edit {
        let edit_btn = tooltip(
            news_edit_button(icon::edit(), Message::NewsEditClicked(item.id)),
            container(shaped_text(t("tooltip-edit")).size(TOOLTIP_TEXT_SIZE))
                .padding(TOOLTIP_BACKGROUND_PADDING)
                .style(tooltip_container_style),
            tooltip::Position::Top,
        )
        .gap(TOOLTIP_GAP)
        .padding(TOOLTIP_PADDING);
        action_row = action_row.push(edit_btn);
    }

    if can_delete {
        let delete_btn = tooltip(
            news_delete_button(icon::trash(), Message::NewsDeleteClicked(item.id)),
            container(shaped_text(t("tooltip-delete")).size(TOOLTIP_TEXT_SIZE))
                .padding(TOOLTIP_BACKGROUND_PADDING)
                .style(tooltip_container_style),
            tooltip::Position::Top,
        )
        .gap(TOOLTIP_GAP)
        .padding(TOOLTIP_PADDING);
        action_row = action_row.push(delete_btn);
    }

    // Author row with actions on the right (fixed height for consistency)
    let author_row = container(
        row![author_text, Space::new().width(Fill), action_row]
            .align_y(alignment::Vertical::Center),
    )
    .height(NEWS_ACTION_BUTTON_SIZE)
    .align_y(alignment::Vertical::Center);

    // Timestamp on its own line (with optional update tooltip)
    let timestamp_text = shaped_text(format_timestamp(&item.created_at))
        .size(TEXT_SIZE)
        .style(muted_text_style);

    let timestamp_element: Element<'a, Message> = if let Some(updated_at) = &item.updated_at {
        let tooltip_text = format!("{}: {}", t("news-updated"), format_timestamp(updated_at));
        tooltip(
            timestamp_text,
            container(shaped_text(tooltip_text).size(TOOLTIP_TEXT_SIZE))
                .padding(TOOLTIP_BACKGROUND_PADDING)
                .style(tooltip_container_style),
            tooltip::Position::Top,
        )
        .gap(TOOLTIP_GAP)
        .padding(TOOLTIP_PADDING)
        .into()
    } else {
        timestamp_text.into()
    };

    // Group author and timestamp together with minimal spacing
    let header_group = Column::new()
        .spacing(NO_SPACING)
        .push(author_row)
        .push(timestamp_element);

    // Build content column
    let mut content_col = Column::new()
        .width(Fill)
        .spacing(ELEMENT_SPACING)
        .padding(INPUT_PADDING)
        .push(header_group);

    // Add image if present (from cache)
    if item.image.is_some()
        && let Some(cached) = news_image_cache.get(&item.id)
    {
        content_col = content_col.push(render_cached_image(cached));
    }

    // Add body as markdown if present (from cache)
    if item.body.is_some()
        && let Some(markdown_items) = news_markdown_cache.get(&item.id)
    {
        // Create markdown settings with appropriate text size
        let md_settings = markdown::Settings::with_text_size(TEXT_SIZE, theme);

        // Render markdown and map link clicks to OpenUrl message
        let md_view: Element<'a, Message> =
            markdown::view(markdown_items, md_settings).map(Message::OpenUrl);

        content_col = content_col.push(md_view);
    }

    // Alternating row backgrounds
    let is_even = index.is_multiple_of(2);
    container(content_col)
        .width(Fill)
        .style(alternating_row_style(is_even))
        .into()
}

// ============================================================================
// Form View (used for both Create and Edit)
// ============================================================================

/// Build the news form (create or edit)
fn form_view<'a>(
    news_management: &'a NewsManagementState,
    body_content: Option<&'a text_editor::Content>,
    is_edit: bool,
) -> Element<'a, Message> {
    let title = shaped_text(if is_edit {
        t("title-news-edit")
    } else {
        t("title-news-create")
    })
    .size(TITLE_SIZE)
    .width(Fill)
    .align_x(Center);

    // Check if we have content (body from editor or image)
    let body_text = body_content.map(|c| c.text()).unwrap_or_default();
    let has_content = !body_text.trim().is_empty() || !news_management.form_image.is_empty();

    // Body text editor
    let body_editor: Element<'a, Message> = if let Some(content) = body_content {
        text_editor(content)
            .id(Id::from(InputId::NewsBody))
            .placeholder(t("placeholder-news-body"))
            .on_action(Message::NewsBodyAction)
            .padding(FORM_PADDING / 2.0)
            .size(TEXT_SIZE)
            .line_height(NEWS_EDITOR_LINE_HEIGHT)
            .height(Length::Fixed(150.0))
            .into()
    } else {
        // Fallback if no content (shouldn't happen in practice)
        shaped_text(t("news-loading"))
            .size(TEXT_SIZE)
            .style(muted_text_style)
            .into()
    };

    // Image picker
    let pick_image_button = button(shaped_text(t("button-choose-image")).size(TEXT_SIZE))
        .on_press(Message::NewsPickImagePressed)
        .padding(BUTTON_PADDING)
        .style(btn::secondary);

    let clear_image_button = if !news_management.form_image.is_empty() {
        button(shaped_text(t("button-clear-image")).size(TEXT_SIZE))
            .on_press(Message::NewsClearImagePressed)
            .padding(BUTTON_PADDING)
            .style(btn::secondary)
    } else {
        button(shaped_text(t("button-clear-image")).size(TEXT_SIZE))
            .padding(BUTTON_PADDING)
            .style(btn::secondary)
    };

    let image_buttons = row![pick_image_button, clear_image_button].spacing(ELEMENT_SPACING);

    // Image preview if present
    let image_row: Element<'a, Message> = if let Some(cached) = &news_management.cached_form_image {
        let image_preview = render_cached_image_preview(cached);
        row![image_preview, image_buttons]
            .spacing(ELEMENT_SPACING)
            .align_y(Center)
            .into()
    } else {
        image_buttons.into()
    };

    // Submit button (Create or Save)
    let submit_button = if has_content {
        button(
            shaped_text(if is_edit {
                t("button-save")
            } else {
                t("button-create")
            })
            .size(TEXT_SIZE),
        )
        .on_press(Message::NewsSubmitPressed)
        .padding(BUTTON_PADDING)
    } else {
        button(
            shaped_text(if is_edit {
                t("button-save")
            } else {
                t("button-create")
            })
            .size(TEXT_SIZE),
        )
        .padding(BUTTON_PADDING)
    };

    let cancel_button = button(shaped_text(t("button-cancel")).size(TEXT_SIZE))
        .on_press(Message::CancelNews)
        .padding(BUTTON_PADDING)
        .style(btn::secondary);

    let mut items: Vec<Element<'a, Message>> = vec![title.into()];

    // Show error if present
    if let Some(error) = &news_management.form_error {
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
        image_row,
        Space::new().height(SPACER_SIZE_SMALL).into(),
        body_editor,
        Space::new().height(SPACER_SIZE_MEDIUM).into(),
        row![Space::new().width(Fill), cancel_button, submit_button]
            .spacing(ELEMENT_SPACING)
            .into(),
    ]);

    let form = Column::with_children(items)
        .spacing(ELEMENT_SPACING)
        .padding(FORM_PADDING)
        .max_width(NEWS_LIST_MAX_WIDTH);

    scrollable_panel(form)
}

// ============================================================================
// Delete Confirmation Modal
// ============================================================================

/// Build the delete confirmation modal
fn confirm_delete_modal<'a>() -> Element<'a, Message> {
    let title = shaped_text(t("title-confirm-delete"))
        .size(TITLE_SIZE)
        .width(Fill)
        .align_x(Center);

    let message = shaped_text_wrapped(t("confirm-delete-news"))
        .size(TEXT_SIZE)
        .width(Fill)
        .align_x(Center);

    let confirm_button = button(shaped_text(t("button-delete")).size(TEXT_SIZE))
        .on_press(Message::NewsConfirmDelete)
        .padding(BUTTON_PADDING)
        .style(btn::danger);

    let cancel_button = button(shaped_text(t("button-cancel")).size(TEXT_SIZE))
        .on_press(Message::NewsCancelDelete)
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
// Main View Function
// ============================================================================

/// Displays the news management panel
///
/// Shows one of four views based on mode:
/// - List: Shows all news items with edit/delete buttons
/// - Create: Form to create a new news item
/// - Edit: Form to edit an existing news item
/// - ConfirmDelete: Modal to confirm news item deletion
pub fn news_view<'a>(
    conn: &'a ServerConnection,
    news_management: &'a NewsManagementState,
    theme: &Theme,
    body_content: Option<&'a text_editor::Content>,
) -> Element<'a, Message> {
    match &news_management.mode {
        NewsManagementMode::List => list_view(conn, news_management, theme, &conn.news_image_cache),
        NewsManagementMode::Create => form_view(news_management, body_content, false),
        NewsManagementMode::Edit { .. } => form_view(news_management, body_content, true),
        NewsManagementMode::ConfirmDelete { .. } => confirm_delete_modal(),
    }
}
