//! Widget style functions
//!
//! Provides consistent styling for Iced widgets across the application.
//! All styles derive from Iced's theme palette for compatibility with
//! all 30 themes (22 built-in Iced + 8 custom Celestial).

use iced::widget::{Container, button, container, text};
use iced::{Background, Border, Center, Color, Fill, Theme};

use super::shaping::shaped_text;
use super::ui;
use super::{
    CONTEXT_MENU_BORDER_RADIUS, CONTEXT_MENU_BORDER_WIDTH, CONTEXT_MENU_SHADOW_BLUR,
    CONTEXT_MENU_SHADOW_OFFSET, CONTEXT_MENU_SHADOW_OPACITY, TITLE_ROW_HEIGHT_WITH_ACTION,
    TITLE_SIZE,
};
use crate::types::Message;

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

/// User list item button style - with admin color support
pub fn user_list_item_button_style(
    is_admin: bool,
    admin_color: Color,
) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| button::Style {
        background: None,
        text_color: match status {
            button::Status::Hovered => theme.palette().primary,
            _ if is_admin => admin_color,
            _ => ui::text_color(theme),
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

/// Context menu button style - has visible hover background
///
/// Note: iced_aw's ContextMenu currently doesn't forward hover events to overlay content,
/// so hover states won't display. When this is fixed upstream, hover will work automatically.
pub fn context_menu_button_style(theme: &Theme, status: button::Status) -> button::Style {
    let palette = theme.extended_palette();
    button::Style {
        background: match status {
            button::Status::Hovered | button::Status::Pressed => {
                Some(Background::Color(palette.primary.weak.color))
            }
            _ => None,
        },
        text_color: match status {
            button::Status::Hovered | button::Status::Pressed => palette.primary.weak.text,
            _ => ui::text_color(theme),
        },
        border: Border {
            radius: CONTEXT_MENU_BORDER_RADIUS.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

/// Context menu item style for danger/destructive actions (e.g., delete)
///
/// Uses danger-colored text to indicate a destructive action.
/// No background in normal state for a clean menu appearance.
///
/// Note: iced_aw's ContextMenu currently doesn't forward hover events to overlay content,
/// so hover states won't display. When this is fixed upstream, hover will work automatically.
pub fn context_menu_item_danger_style(theme: &Theme, status: button::Status) -> button::Style {
    let palette = theme.extended_palette();
    button::Style {
        background: match status {
            button::Status::Hovered | button::Status::Pressed => {
                Some(Background::Color(palette.danger.weak.color))
            }
            _ => None,
        },
        text_color: match status {
            button::Status::Hovered | button::Status::Pressed => palette.danger.weak.text,
            _ => theme.palette().danger,
        },
        border: Border {
            radius: CONTEXT_MENU_BORDER_RADIUS.into(),
            ..Default::default()
        },
        ..Default::default()
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
            radius: CONTEXT_MENU_BORDER_RADIUS.into(),
        },
        shadow: iced::Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, CONTEXT_MENU_SHADOW_OPACITY),
            offset: iced::Vector::new(CONTEXT_MENU_SHADOW_OFFSET, CONTEXT_MENU_SHADOW_OFFSET),
            blur_radius: CONTEXT_MENU_SHADOW_BLUR,
        },
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

/// Text/icon style for uploadable folder icons (uses primary color like connected servers)
pub fn upload_folder_style(theme: &Theme) -> text::Style {
    text::Style {
        color: Some(theme.extended_palette().primary.base.color),
    }
}

/// Subheading text style - for section headers within panels
///
/// Uses muted color to create visual hierarchy below the main title.
pub fn subheading_text_style(theme: &Theme) -> text::Style {
    text::Style {
        color: Some(ui::muted_text_color(theme)),
    }
}

// ============================================================================
// Panel Helpers
// ============================================================================

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
