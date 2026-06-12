//! Widget style functions
//!
//! Provides consistent styling for Iced widgets across the application.
//! All styles derive from Iced's theme palette for compatibility with
//! all 30 themes (22 built-in Iced + 8 custom Celestial).

use super::shaping::shaped_text;
use super::ui;
use super::{
    CONTEXT_MENU_BORDER_WIDTH, STANDARD_BORDER_RADIUS, TITLE_ROW_HEIGHT_WITH_ACTION, TITLE_SIZE,
    TOAST_BORDER_RADIUS, TOAST_BORDER_WIDTH,
};
use crate::types::Message;
use crate::widgets::{MenuButtonStatus, MenuButtonStyle};
use std::rc::Rc;

use iced::widget::{Container, button, container, text};
use iced::{Background, Border, Center, Color, Fill, Shadow, Theme};

// ============================================================================
// Button Styles
// ============================================================================

/// Active chat tab style - stays primary.strong on hover
pub fn chat_tab_active_style() -> fn(&Theme, button::Status) -> button::Style {
    |theme, _status| {
        let ext = theme.extended_palette();
        button::Style {
            background: Some(Background::Color(ext.primary.strong.color)),
            text_color: ext.primary.strong.text,
            ..Default::default()
        }
    }
}

/// Close button style for buttons that appear on primary-colored backgrounds
///
/// Uses the default primary button style, but switches to danger color on hover
/// to indicate destructive action.
pub fn close_button_on_primary_style() -> fn(&Theme, button::Status) -> button::Style {
    |theme, status| {
        let base = button::primary(theme, status);
        match status {
            button::Status::Hovered => button::Style {
                text_color: theme.palette().danger,
                background: None,
                ..base
            },
            _ => button::Style {
                background: None,
                ..base
            },
        }
    }
}

/// Danger icon button style - transparent with danger color on hover
pub fn danger_icon_button_style(theme: &Theme, status: button::Status) -> button::Style {
    let base = transparent_icon_button_style(theme, status);
    button::Style {
        text_color: match status {
            button::Status::Hovered => theme.palette().danger,
            _ => base.text_color,
        },
        ..base
    }
}

/// Disabled icon button style - no background, dimmed icon
pub fn disabled_icon_button_style(theme: &Theme, _status: button::Status) -> button::Style {
    button::Style {
        background: None,
        text_color: ui::icon_disabled_color(theme),
        ..Default::default()
    }
}

/// Icon button style with custom hover color
pub fn icon_button_with_hover_style(
    hover_color: Color,
    normal_color: Color,
) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_theme, status| button::Style {
        background: None,
        text_color: match status {
            button::Status::Hovered => hover_color,
            _ => normal_color,
        },
        ..Default::default()
    }
}

/// List item button style - transparent with optional highlight and error states
/// Used for server list and bookmark items
pub fn list_item_button_style(
    is_highlighted: bool,
    has_error: bool,
) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        let color = theme.extended_palette().primary.base.color;
        button::Style {
            background: None,
            text_color: match status {
                button::Status::Hovered => color,
                _ if has_error => theme.palette().danger,
                _ if is_highlighted => color,
                _ => ui::text_color(theme),
            },
            ..Default::default()
        }
    }
}

/// Toolbar button style - handles active (selected) and inactive states
pub fn toolbar_button_style(is_active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        if is_active {
            // Active state - primary background (matches button::primary)
            let ext = theme.extended_palette();
            button::Style {
                background: Some(Background::Color(ext.primary.strong.color)),
                text_color: ext.primary.strong.text,
                ..Default::default()
            }
        } else {
            // Inactive state - transparent with hover
            transparent_icon_button_style(theme, status)
        }
    }
}

/// Transparent icon button style - no background, icon color with hover
pub fn transparent_icon_button_style(theme: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: None,
        text_color: match status {
            button::Status::Hovered => theme.palette().primary,
            _ => ui::icon_color(theme),
        },
        ..Default::default()
    }
}

/// Clickable text style - transparent background with optional custom text color and hover support
pub fn clickable_text_style(
    default_color: Option<Color>,
) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| button::Style {
        background: None,
        text_color: match status {
            button::Status::Hovered => theme.palette().primary,
            _ => default_color.unwrap_or_else(|| ui::text_color(theme)),
        },
        ..Default::default()
    }
}

// ============================================================================
// Container Styles
// ============================================================================

/// Alternating row background style (for even rows in lists)
fn alt_row_style(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(ui::alt_row_color(theme))),
        ..Default::default()
    }
}

/// Content area background style (for forms and popups)
pub fn content_background_style(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(theme.palette().background)),
        ..Default::default()
    }
}

/// Alternating row style - returns alt_row_style for even rows, default for odd
pub fn alternating_row_style(is_even: bool) -> impl Fn(&Theme) -> container::Style {
    move |theme| {
        if is_even {
            alt_row_style(theme)
        } else {
            container::Style::default()
        }
    }
}

/// Modal overlay style (semi-transparent, theme-aware background)
pub fn modal_overlay_style(theme: &Theme) -> container::Style {
    let bg = theme.palette().background;
    container::Style {
        background: Some(Background::Color(Color::from_rgba(bg.r, bg.g, bg.b, 0.9))),
        ..Default::default()
    }
}

/// User toolbar separator style (for user list toolbar dividers)
/// Uses extended palette to match button::primary styling
pub fn user_toolbar_separator_style(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(
            theme.extended_palette().primary.base.color,
        )),
        ..Default::default()
    }
}

/// Separator line style
pub fn separator_style(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(ui::sidebar_border(theme))),
        ..Default::default()
    }
}

/// Sidebar panel background style with border
pub fn sidebar_panel_style(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(ui::sidebar_background(theme))),
        border: Border {
            color: ui::sidebar_border(theme),
            width: 1.0,
            ..Default::default()
        },
        ..Default::default()
    }
}

/// Toolbar background style
pub fn toolbar_background_style(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(ui::toolbar_background(theme))),
        ..Default::default()
    }
}

/// Tooltip container style - uses Iced's built-in bordered box style
pub fn tooltip_container_style(theme: &Theme) -> container::Style {
    container::bordered_box(theme)
}

/// Menu button style with working hover states for context menus
///
/// Use this with `MenuButton` instead of `button()` in context menus
/// to get proper hover highlighting.
pub fn menu_button_style(theme: &Theme, status: MenuButtonStatus) -> MenuButtonStyle {
    let palette = theme.extended_palette();
    MenuButtonStyle {
        background: match status {
            MenuButtonStatus::Hovered | MenuButtonStatus::Pressed => {
                Some(Background::Color(palette.primary.weak.color))
            }
            MenuButtonStatus::Active => None,
        },
        text_color: match status {
            MenuButtonStatus::Hovered | MenuButtonStatus::Pressed => palette.primary.weak.text,
            MenuButtonStatus::Active => ui::text_color(theme),
        },
        border: Border {
            radius: STANDARD_BORDER_RADIUS.into(),
            ..Default::default()
        },
    }
}

/// Menu button danger style with working hover states for context menus
///
/// Use this with `MenuButton` for destructive actions (e.g., delete)
/// to get proper hover highlighting.
pub fn menu_button_danger_style(theme: &Theme, status: MenuButtonStatus) -> MenuButtonStyle {
    let palette = theme.extended_palette();
    MenuButtonStyle {
        background: match status {
            MenuButtonStatus::Hovered | MenuButtonStatus::Pressed => {
                Some(Background::Color(palette.danger.weak.color))
            }
            MenuButtonStatus::Active => None,
        },
        text_color: match status {
            MenuButtonStatus::Hovered | MenuButtonStatus::Pressed => palette.danger.weak.text,
            MenuButtonStatus::Active => theme.palette().danger,
        },
        border: Border {
            radius: STANDARD_BORDER_RADIUS.into(),
            ..Default::default()
        },
    }
}

/// Context menu container style - popup menu with border and shadow
pub fn context_menu_container_style(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();
    container::Style {
        background: Some(Background::Color(palette.background.base.color)),
        border: Border {
            color: palette.background.strong.color,
            width: CONTEXT_MENU_BORDER_WIDTH,
            radius: STANDARD_BORDER_RADIUS.into(),
        },
        shadow: iced::Shadow::default(),
        ..Default::default()
    }
}

// ============================================================================
// Text Styles
// ============================================================================

/// Error text style - uses danger color
pub fn error_text_style(theme: &Theme) -> text::Style {
    text::Style {
        color: Some(ui::danger_color(theme)),
    }
}

/// Muted text style - for section titles and secondary info
pub fn muted_text_style(theme: &Theme) -> text::Style {
    text::Style {
        color: Some(ui::muted_text_color(theme)),
    }
}

/// Success text/icon style — uses theme success color. Used for
/// affirmative indicators (e.g. the ✓ in the tracker discovery panel's
/// Public column).
pub fn success_text_style(theme: &Theme) -> text::Style {
    text::Style {
        color: Some(theme.extended_palette().success.base.color),
    }
}

/// Text/icon style for uploadable folder icons (uses primary color like connected servers)
pub fn upload_folder_style(theme: &Theme) -> text::Style {
    text::Style {
        color: Some(theme.extended_palette().primary.base.color),
    }
}

/// Container style for drag-and-drop overlay
///
/// Semi-transparent background (normal background at 85% opacity) so the content
/// is slightly visible underneath while the upload icon and text remain readable.
pub fn drop_overlay_style(theme: &Theme) -> container::Style {
    let ext = theme.extended_palette();
    container::Style {
        background: Some(Background::Color(Color {
            a: 0.85,
            ..ext.background.base.color
        })),
        text_color: Some(ext.background.base.text),
        ..Default::default()
    }
}

// ============================================================================
// Panel Helpers
// ============================================================================

/// Badge style for notification count overlays
///
/// Creates a small pill-shaped badge with the primary color background.
/// Used for showing counts on toolbar buttons (e.g., active transfers).
pub fn badge_style(theme: &Theme) -> container::Style {
    let ext = theme.extended_palette();
    container::Style {
        background: Some(Background::Color(ext.primary.strong.color)),
        text_color: Some(ext.primary.strong.text),
        border: Border {
            radius: super::BADGE_BORDER_RADIUS.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

/// Voice bar container style - subtle background for the voice status bar
///
/// Uses a slightly tinted background to distinguish the voice bar from the
/// rest of the chat area while remaining visually unobtrusive.
pub fn voice_bar_style(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(ui::toolbar_background(theme))),
        text_color: Some(ui::text_color(theme)),
        border: Border {
            radius: STANDARD_BORDER_RADIUS.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

/// Voice deafen button style - transparent icon button for the voice bar
///
/// Uses the same pattern as bookmark icons: icon color normally, blue on hover, no background.
pub fn voice_deafen_button_style(theme: &Theme, status: button::Status) -> button::Style {
    transparent_icon_button_style(theme, status)
}

/// Speaking indicator style - highlights users who are currently speaking
///
/// Uses the success color (green) to indicate active voice transmission, no background.
pub fn speaking_indicator_style(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();
    container::Style {
        background: None,
        text_color: Some(palette.success.base.color),
        ..Default::default()
    }
}

/// Toast notification style — theme-aware with square corners
///
/// Used by the `ToastContainer` to style all toast notifications.
/// Returns a closure compatible with `ToastContainer::style()`.
pub fn toast_style(theme: &Theme) -> iced_toasts::Style<'_> {
    let palette = theme.extended_palette();
    iced_toasts::Style {
        text_color: Some(palette.background.base.text),
        background: Some(Background::Color(palette.background.base.color)),
        border: Border {
            color: palette.background.strong.color,
            width: TOAST_BORDER_WIDTH,
            radius: TOAST_BORDER_RADIUS.into(),
        },
        shadow: Shadow::default(),
        level_to_color: Rc::new(move |level| match level {
            iced_toasts::ToastLevel::Success => Some(palette.success.strong.color),
            iced_toasts::ToastLevel::Warning => Some(palette.danger.strong.color),
            iced_toasts::ToastLevel::Error => Some(palette.danger.strong.color),
            iced_toasts::ToastLevel::Info => Some(palette.primary.strong.color),
        }),
    }
}

/// Build a centered panel title row
///
/// Creates a consistent title row for panel headers with proper sizing
/// and alignment. Used by Bookmark, Broadcast, Connection, Files, Fingerprint,
/// News, Server Info, Settings, User Info, and User Management panels.
///
/// # Example
/// ```ignore
/// use crate::style::panel_title;
///
/// let title_row = panel_title(t("files-panel-title"));
/// ```
pub fn panel_title(title: impl Into<String>) -> Container<'static, Message> {
    container(
        shaped_text(title)
            .size(TITLE_SIZE)
            .width(Fill)
            .align_x(Center),
    )
    .height(TITLE_ROW_HEIGHT_WITH_ACTION)
    .align_y(Center)
}
