//! Widget style functions
//!
//! Provides consistent styling for Iced widgets across the application.
//! All styles derive from Iced's theme palette for compatibility with
//! all 22 built-in themes.

use super::ui;
use iced::widget::{button, container, text};
use iced::{Background, Border, Color, Theme};

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

// ============================================================================
// Container Styles
// ============================================================================

/// Separator line style
pub fn separator_style(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(ui::sidebar_border(theme))),
        ..Default::default()
    }
}

/// Content area background style
pub fn content_background_style(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(ui::content_background(theme))),
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

/// Alternating row background style (for even rows in lists)
pub fn alt_row_style(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(ui::alt_row_color(theme))),
        ..Default::default()
    }
}

/// Primary color separator style (for toolbar dividers)
pub fn primary_separator_style(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(theme.palette().primary)),
        ..Default::default()
    }
}

// ============================================================================
// Button Styles
// ============================================================================

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
        border: Border::default(),
        shadow: iced::Shadow::default(),
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

/// List item button style - transparent with optional highlight state
/// Used for server list and bookmark items
pub fn list_item_button_style(
    is_highlighted: bool,
) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| button::Style {
        background: None,
        text_color: match status {
            button::Status::Hovered => theme.palette().primary,
            _ if is_highlighted => theme.palette().primary,
            _ => ui::text_color(theme),
        },
        border: Border::default(),
        shadow: iced::Shadow::default(),
    }
}

/// List item button style with error state
/// Used for bookmark items that have connection errors
pub fn list_item_button_style_with_error(
    is_highlighted: bool,
    has_error: bool,
) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| button::Style {
        background: None,
        text_color: match status {
            button::Status::Hovered => theme.palette().primary,
            _ if has_error => theme.palette().danger,
            _ if is_highlighted => theme.palette().primary,
            _ => ui::text_color(theme),
        },
        border: Border::default(),
        shadow: iced::Shadow::default(),
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
        border: Border::default(),
        shadow: iced::Shadow::default(),
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

/// Transparent icon button style - no background, icon color with hover
pub fn transparent_icon_button_style(theme: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: None,
        text_color: match status {
            button::Status::Hovered => theme.palette().primary,
            _ => ui::icon_color(theme),
        },
        border: Border::default(),
        shadow: iced::Shadow::default(),
    }
}

/// Disabled icon button style - no background, dimmed icon
pub fn disabled_icon_button_style(theme: &Theme, _status: button::Status) -> button::Style {
    button::Style {
        background: None,
        text_color: ui::icon_disabled_color(theme),
        border: Border::default(),
        shadow: iced::Shadow::default(),
    }
}

/// Toolbar button style - handles active (selected) and inactive states
pub fn toolbar_button_style(
    is_active: bool,
) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        if is_active {
            // Active state - primary background
            let ext = theme.extended_palette();
            button::Style {
                background: Some(Background::Color(theme.palette().primary)),
                text_color: ext.primary.strong.text,
                border: Border::default(),
                shadow: iced::Shadow::default(),
            }
        } else {
            // Inactive state - transparent with hover
            transparent_icon_button_style(theme, status)
        }
    }
}

/// Inactive chat tab style - no background, just text
pub fn chat_tab_inactive_style() -> fn(&Theme, button::Status) -> button::Style {
    |theme, status| match status {
        button::Status::Hovered => button::primary(theme, status),
        _ => {
            let base = button::text(theme, status);
            button::Style {
                background: None,
                ..base
            }
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

// ============================================================================
// Container Styles
// ============================================================================

/// Modal overlay style (semi-transparent, theme-aware background)
pub fn modal_overlay_style(theme: &Theme) -> container::Style {
    let bg = theme.palette().background;
    container::Style {
        background: Some(Background::Color(Color::from_rgba(bg.r, bg.g, bg.b, 0.8))),
        ..Default::default()
    }
}

// ============================================================================
// Tooltip Style
// ============================================================================

/// Tooltip container style - uses Iced's built-in bordered box style
pub fn tooltip_container_style(theme: &Theme) -> container::Style {
    container::bordered_box(theme)
}
