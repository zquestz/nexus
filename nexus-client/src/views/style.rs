//! Shared UI styling constants for consistent appearance across views
//!
//! This module provides helper functions that return theme-aware colors.
//! All actual color values are defined in the `colors` module.
//!
//! ## Usage
//!
//! Always use these helper functions in view code, never hardcode colors:
//! ```
//! // ✅ GOOD
//! text("Hello").style(|theme| text::Style {
//!     color: Some(section_title_color(theme)),
//! })
//!
//! // ❌ BAD
//! text("Hello").color(Color::from_rgb(0.7, 0.7, 0.7))
//! ```

use super::colors;
use iced::font::Family;
use iced::widget::{container, text};
use iced::{Background, Border, Color, Font, Theme};

// ============================================================================
// Fonts
// ============================================================================

/// Monospace font for text inputs
///
/// Uses embedded "SauceCodePro Nerd Font Mono" for consistent cross-platform rendering.
/// This avoids ligatures and ensures predictable character spacing.
pub const MONOSPACE_FONT: Font = Font {
    family: Family::Name("SauceCodePro Nerd Font Mono"),
    ..Font::MONOSPACE
};

// ============================================================================
// Text Shaping (CJK Support)
// ============================================================================

/// Create a text widget with advanced shaping enabled for CJK character support
///
/// Advanced shaping enables font fallback, allowing the system to find fonts
/// that contain Chinese, Japanese, and Korean characters even if the default
/// font doesn't support them.
///
/// This should be used for ALL text in the application to ensure proper
/// display of internationalized content.
///
/// # Optimization
/// Empty strings (used as spacers) skip advanced shaping since there's no
/// text to render. This is a minor optimization with no functional impact.
///
/// # Example
/// ```
/// use crate::views::style::shaped_text;
///
/// shaped_text("你好世界")  // Chinese characters will display correctly
///     .size(14)
/// ```
pub fn shaped_text<'a>(content: impl Into<String>) -> text::Text<'a> {
    let s = content.into();
    if s.is_empty() {
        text(s)
    } else {
        text(s).shaping(text::Shaping::Advanced)
    }
}

// ============================================================================
// Font Sizes
// ============================================================================

/// Title text size (e.g., form headers)
pub const TITLE_SIZE: u16 = 20;

/// Standard text and input field size
pub const TEXT_SIZE: u16 = 14;

/// Medium vertical spacer (between sections)
pub const SPACER_SIZE_MEDIUM: u16 = 10;

/// Small vertical spacer (between related items)
pub const SPACER_SIZE_SMALL: u16 = 5;

/// Chat message text size
pub const CHAT_MESSAGE_SIZE: u16 = 14;

/// Chat input field size (slightly larger than messages)
pub const CHAT_INPUT_SIZE: u16 = 16;

/// Toolbar title text size
pub const TOOLBAR_TITLE_SIZE: u16 = 16;

/// Toolbar icon size (for collapse/expand icons)
pub const TOOLBAR_ICON_SIZE: u16 = 20;

/// Tooltip text size (smaller, less imposing)
pub const TOOLTIP_TEXT_SIZE: u16 = 11;

/// Tooltip gap (distance between element and tooltip)
pub const TOOLTIP_GAP: u16 = 5;

/// Tooltip padding (internal padding)
pub const TOOLTIP_PADDING: u16 = 4;

/// Tooltip background padding (inside the tooltip box)
pub const TOOLTIP_BACKGROUND_PADDING: u16 = 6;

/// Tooltip background color (semi-transparent black, works in both themes)
pub const TOOLTIP_BACKGROUND_COLOR: Color = colors::TOOLTIP_BACKGROUND;

/// Tooltip border radius (rounded corners)
pub const TOOLTIP_BORDER_RADIUS: f32 = 2.0;

/// Empty view message text size
pub const EMPTY_VIEW_SIZE: u16 = 16;

/// Server list section title size
pub const SECTION_TITLE_SIZE: u16 = 14;

/// Server list server name text size
pub const SERVER_LIST_TEXT_SIZE: u16 = 13;

/// Server list small text size (empty states, action buttons)
pub const SERVER_LIST_SMALL_TEXT_SIZE: u16 = 11;

/// Server list disconnect icon size (larger, more prominent)
pub const SERVER_LIST_DISCONNECT_ICON_SIZE: u16 = 18;

/// User list title size
pub const USER_LIST_TITLE_SIZE: u16 = 16;

/// User list username text size
pub const USER_LIST_TEXT_SIZE: u16 = 12;

/// User list small text size (empty states)
pub const USER_LIST_SMALL_TEXT_SIZE: u16 = 11;

// ============================================================================
// Padding
// ============================================================================

/// Text input field padding
pub const INPUT_PADDING: u16 = 8;

/// Button padding
pub const BUTTON_PADDING: u16 = 10;

/// Form container padding
pub const FORM_PADDING: u16 = 20;

/// Toolbar horizontal padding (matches FORM_PADDING for alignment)
pub const TOOLBAR_PADDING_HORIZONTAL: u16 = 20;

/// Toolbar vertical padding
pub const TOOLBAR_PADDING_VERTICAL: u16 = 8;

/// Icon button padding (vertical)
pub const ICON_BUTTON_PADDING_VERTICAL: u16 = 8;

/// Icon button padding (horizontal)
pub const ICON_BUTTON_PADDING_HORIZONTAL: u16 = 4;

/// Toolbar container padding (horizontal)
pub const TOOLBAR_CONTAINER_PADDING_HORIZONTAL: u16 = 4;

// ============================================================================
// Spacing
// ============================================================================

/// General spacing between form elements
pub const ELEMENT_SPACING: u16 = 10;

/// Spacing between chat messages (compact)
pub const CHAT_SPACING: u16 = 3;

/// Small spacing (general use)
pub const SMALL_SPACING: u16 = 5;

/// Small padding (general use)
pub const SMALL_PADDING: u16 = 5;

/// Toolbar spacing between sections
pub const TOOLBAR_SPACING: u16 = 20;

/// Toolbar icon spacing within icon groups
pub const TOOLBAR_ICON_SPACING: u16 = 0;

/// Panel spacing (no gap between panels)
pub const PANEL_SPACING: u16 = 0;

/// Server list section spacing
pub const SERVER_LIST_SECTION_SPACING: u16 = 5;

/// Server list item spacing
pub const SERVER_LIST_ITEM_SPACING: u16 = 3;

/// No spacing between button and icon (flush)
pub const NO_SPACING: u16 = 0;

/// User list item spacing
pub const USER_LIST_ITEM_SPACING: u16 = 3;

/// User list column spacing
pub const USER_LIST_SPACING: u16 = 8;

// ============================================================================
// Dimensions
// ============================================================================

/// Maximum width for form dialogs
pub const FORM_MAX_WIDTH: u16 = 400;

/// Server list panel width
pub const SERVER_LIST_PANEL_WIDTH: u16 = 220;

/// Server list button height
pub const SERVER_LIST_BUTTON_HEIGHT: u16 = 32;

/// Separator line height
pub const SEPARATOR_HEIGHT: u16 = 1;

/// Border width
pub const BORDER_WIDTH: f32 = 1.0;

/// User list panel width
pub const USER_LIST_PANEL_WIDTH: u16 = 180;

// ============================================================================
// Theme-Aware Color Helper Functions
// ============================================================================
//
// All color values are defined in the `colors` module. These helper functions
// provide a clean API for getting theme-appropriate colors.
// ============================================================================

/// Get toolbar background color for the current theme
pub fn toolbar_background(theme: &Theme) -> Color {
    match theme {
        Theme::Light => colors::TOOLBAR_BACKGROUND_LIGHT,
        _ => colors::TOOLBAR_BACKGROUND_DARK,
    }
}

/// Get sidebar panel background color for the current theme
pub fn sidebar_background(theme: &Theme) -> Color {
    match theme {
        Theme::Light => colors::SIDEBAR_BACKGROUND_LIGHT,
        _ => colors::SIDEBAR_BACKGROUND_DARK,
    }
}

/// Get sidebar panel border color for the current theme
pub fn sidebar_border(theme: &Theme) -> Color {
    match theme {
        Theme::Light => colors::SIDEBAR_BORDER_LIGHT,
        _ => colors::SIDEBAR_BORDER_DARK,
    }
}

/// Get section title color for the current theme
pub fn section_title_color(theme: &Theme) -> Color {
    match theme {
        Theme::Light => colors::SECTION_TITLE_LIGHT,
        _ => colors::SECTION_TITLE_DARK,
    }
}

/// Get empty state text color for the current theme
pub fn empty_state_color(theme: &Theme) -> Color {
    match theme {
        Theme::Light => colors::EMPTY_STATE_LIGHT,
        _ => colors::EMPTY_STATE_DARK,
    }
}

/// Get separator line color for the current theme
pub fn separator_color(theme: &Theme) -> Color {
    match theme {
        Theme::Light => colors::SEPARATOR_LIGHT,
        _ => colors::SEPARATOR_DARK,
    }
}

/// Get alternating row background color for the current theme
pub fn alt_row_color(theme: &Theme) -> Color {
    match theme {
        Theme::Light => colors::ALT_ROW_BACKGROUND_LIGHT,
        _ => colors::ALT_ROW_BACKGROUND_DARK,
    }
}

/// Get default button text color for the current theme
pub fn button_text_color(theme: &Theme) -> Color {
    match theme {
        Theme::Light => colors::BUTTON_TEXT_LIGHT,
        _ => colors::BUTTON_TEXT_DARK,
    }
}

/// Get tooltip text color for the current theme
pub fn tooltip_text_color(theme: &Theme) -> Color {
    match theme {
        Theme::Light => colors::TOOLTIP_TEXT_LIGHT,
        _ => colors::TOOLTIP_TEXT_DARK,
    }
}

/// Create a tooltip container border with rounded corners
pub fn tooltip_border() -> Border {
    Border {
        radius: TOOLTIP_BORDER_RADIUS.into(),
        ..Default::default()
    }
}

/// Get chat message text color for the current theme (regular messages)
pub fn chat_text_color(theme: &Theme) -> Color {
    match theme {
        Theme::Light => colors::CHAT_TEXT_LIGHT,
        _ => colors::CHAT_TEXT_DARK,
    }
}

/// Get system message text color for the current theme
pub fn system_text_color(theme: &Theme) -> Color {
    match theme {
        Theme::Light => colors::SYSTEM_TEXT_LIGHT,
        _ => colors::SYSTEM_TEXT_DARK,
    }
}

/// Get info message text color for the current theme
pub fn info_text_color(theme: &Theme) -> Color {
    match theme {
        Theme::Light => colors::INFO_TEXT_LIGHT,
        _ => colors::INFO_TEXT_DARK,
    }
}

/// Get chat timestamp color for the current theme
pub fn chat_timestamp_color(theme: &Theme) -> Color {
    match theme {
        Theme::Light => colors::CHAT_TIMESTAMP_LIGHT,
        _ => colors::CHAT_TIMESTAMP_DARK,
    }
}

/// Admin user text color in user list (theme-aware red)
pub fn admin_user_text_color(theme: &Theme) -> Color {
    match theme {
        Theme::Light => colors::ADMIN_USER_TEXT_LIGHT,
        _ => colors::ADMIN_USER_TEXT_DARK,
    }
}

/// Get empty view text color for the current theme
pub fn empty_view_text_color(theme: &Theme) -> Color {
    match theme {
        Theme::Light => colors::EMPTY_VIEW_TEXT_LIGHT,
        _ => colors::EMPTY_VIEW_TEXT_DARK,
    }
}

// ============================================================================
// Icon Color Functions - Organized by Icon Group
// ============================================================================
//
// Four icon groups in the application:
// 1. Top toolbar icons (megaphone, user_plus, users, sun, collapse/expand)
// 2. Server list icons (logout/disconnect - red hover for destructive action)
// 3. Bookmark list icons (cog/edit - blue hover for neutral action)
// 4. User list toolbar icons (info, message, kick - mixed behaviors)

/// Base icon color for top toolbar
///
/// Used by: chat, megaphone, user_plus, users, sun, collapse, expand icons
pub fn toolbar_icon_color(theme: &Theme) -> Color {
    match theme {
        Theme::Light => colors::TOOLBAR_ICON_LIGHT,
        _ => colors::TOOLBAR_ICON_DARK,
    }
}

/// Disabled icon color for top toolbar
pub fn toolbar_icon_disabled_color(theme: &Theme) -> Color {
    match theme {
        Theme::Light => colors::TOOLBAR_ICON_DISABLED_LIGHT,
        _ => colors::TOOLBAR_ICON_DISABLED_DARK,
    }
}

/// Base icon color for server list (logout/disconnect icons)
///
/// Used by: logout icons in connected servers list
pub fn disconnect_icon_color(theme: &Theme) -> Color {
    match theme {
        Theme::Light => colors::DISCONNECT_ICON_LIGHT,
        _ => colors::DISCONNECT_ICON_DARK,
    }
}

/// Hover color for disconnect icons (red for destructive action)
pub fn disconnect_icon_hover_color(theme: &Theme) -> Color {
    match theme {
        Theme::Light => colors::DISCONNECT_ICON_HOVER_LIGHT,
        _ => colors::DISCONNECT_ICON_HOVER_DARK,
    }
}

/// Base icon color for bookmark list (cog/edit icons)
///
/// Returns gray by default, blue on hover
pub fn edit_icon_color(theme: &Theme) -> Color {
    match theme {
        Theme::Light => colors::SIDEBAR_ICON_LIGHT,
        _ => colors::SIDEBAR_ICON_DARK,
    }
}

/// Hover color for bookmark list icons
pub fn edit_icon_hover_color(theme: &Theme) -> Color {
    match theme {
        Theme::Light => colors::SIDEBAR_ICON_HOVER_LIGHT,
        _ => colors::SIDEBAR_ICON_HOVER_DARK,
    }
}

/// Base icon color for user list toolbar (info, message, kick icons)
///
/// Returns gray by default, blue on hover
pub fn sidebar_icon_color(theme: &Theme) -> Color {
    match theme {
        Theme::Light => colors::SIDEBAR_ICON_LIGHT,
        _ => colors::SIDEBAR_ICON_DARK,
    }
}

/// Hover color for user list toolbar icons
pub fn sidebar_icon_hover_color(theme: &Theme) -> Color {
    match theme {
        Theme::Light => colors::SIDEBAR_ICON_HOVER_LIGHT,
        _ => colors::SIDEBAR_ICON_HOVER_DARK,
    }
}

/// Get interactive element hover color (theme-independent signature blue)
///
/// Used for button hover states, active server/bookmark selection, and clickable items.
/// This is our signature blue color used throughout the application for consistency.
pub fn interactive_hover_color() -> Color {
    colors::INTERACTIVE_HOVER
}

/// Get error message text color for chat messages
pub fn error_message_color() -> Color {
    colors::ERROR_MESSAGE
}

/// Broadcast message text color (theme-aware red)
pub fn broadcast_message_color(theme: &Theme) -> Color {
    match theme {
        Theme::Light => colors::BROADCAST_TEXT_LIGHT,
        _ => colors::BROADCAST_TEXT_DARK,
    }
}

/// Get error text color for form validation messages
pub fn form_error_color() -> Color {
    colors::FORM_ERROR
}

/// Get error color for bookmarks with connection errors
pub fn bookmark_error_color() -> Color {
    colors::ERROR_MESSAGE
}

/// Get primary action button background color
pub fn primary_action_background() -> Color {
    colors::PRIMARY_ACTION_BG
}

/// Get primary action button background color when hovered
pub fn primary_action_background_hovered() -> Color {
    colors::PRIMARY_ACTION_BG_HOVER
}

/// Get primary action button background color when pressed
pub fn primary_action_background_pressed() -> Color {
    colors::PRIMARY_ACTION_BG_PRESSED
}

/// Get disabled button background color
pub fn disabled_action_background() -> Color {
    colors::DISABLED_ACTION_BG
}

/// Get disabled button text color
pub fn disabled_action_text() -> Color {
    colors::DISABLED_ACTION_TEXT
}

/// Get text color for buttons with colored backgrounds
pub fn action_button_text() -> Color {
    colors::ACTION_BUTTON_TEXT
}

/// Create styled primary button (blue background, white text)
///
/// This creates a button with the same blue background used for active toolbar buttons,
/// providing visual consistency throughout the application.
pub fn primary_button_style()
-> fn(&Theme, iced::widget::button::Status) -> iced::widget::button::Style {
    |_theme, status| {
        use iced::widget::button::Status;

        match status {
            Status::Active => iced::widget::button::Style {
                background: Some(Background::from(primary_action_background())),
                text_color: action_button_text(),
                border: Border::default(),
                shadow: iced::Shadow::default(),
            },
            Status::Hovered => iced::widget::button::Style {
                background: Some(Background::from(primary_action_background_hovered())),
                text_color: action_button_text(),
                border: Border::default(),
                shadow: iced::Shadow::default(),
            },
            Status::Pressed => iced::widget::button::Style {
                background: Some(Background::from(primary_action_background_pressed())),
                text_color: action_button_text(),
                border: Border::default(),
                shadow: iced::Shadow::default(),
            },
            Status::Disabled => iced::widget::button::Style {
                background: Some(Background::from(disabled_action_background())),
                text_color: disabled_action_text(),
                border: Border::default(),
                shadow: iced::Shadow::default(),
            },
        }
    }
}

/// Create styled button for active chat tab (blue background)
pub fn chat_tab_active_style()
-> fn(&Theme, iced::widget::button::Status) -> iced::widget::button::Style {
    |_theme, _status| iced::widget::button::Style {
        background: Some(Background::from(primary_action_background())),
        text_color: action_button_text(),
        border: Border::default(),
        shadow: iced::Shadow::default(),
    }
}

/// Create styled button for inactive chat tab (transparent with hover)
pub fn chat_tab_inactive_style()
-> fn(&Theme, iced::widget::button::Status) -> iced::widget::button::Style {
    |theme, status| {
        use iced::widget::button::Status;

        match status {
            Status::Hovered => iced::widget::button::Style {
                background: Some(Background::from(interactive_hover_color())),
                text_color: action_button_text(),
                border: Border::default(),
                shadow: iced::Shadow::default(),
            },
            _ => iced::widget::button::Style {
                background: None,
                text_color: chat_text_color(theme),
                border: Border::default(),
                shadow: iced::Shadow::default(),
            },
        }
    }
}

/// Create styled checkbox (blue accent color when checked)
///
/// This creates a checkbox with the same blue used for primary buttons and active toolbar buttons,
/// providing visual consistency throughout the application.
pub fn primary_checkbox_style()
-> fn(&Theme, iced::widget::checkbox::Status) -> iced::widget::checkbox::Style {
    |_theme, status| {
        use iced::widget::checkbox::Status;

        match status {
            Status::Active { is_checked } => iced::widget::checkbox::Style {
                background: if is_checked {
                    Background::from(primary_action_background())
                } else {
                    Background::from(match _theme {
                        Theme::Light => colors::CHECKBOX_UNCHECKED_BG_LIGHT,
                        _ => colors::CHECKBOX_UNCHECKED_BG_DARK,
                    })
                },
                icon_color: action_button_text(),
                border: Border {
                    color: if is_checked {
                        primary_action_background()
                    } else {
                        match _theme {
                            Theme::Light => colors::CHECKBOX_UNCHECKED_BORDER_LIGHT,
                            _ => colors::CHECKBOX_UNCHECKED_BORDER_DARK,
                        }
                    },
                    width: 1.0,
                    radius: 2.0.into(),
                },
                text_color: None,
            },
            Status::Hovered { is_checked } => iced::widget::checkbox::Style {
                background: if is_checked {
                    Background::from(primary_action_background())
                } else {
                    Background::from(match _theme {
                        Theme::Light => colors::CHECKBOX_UNCHECKED_BG_HOVER_LIGHT,
                        _ => colors::CHECKBOX_UNCHECKED_BG_HOVER_DARK,
                    })
                },
                icon_color: action_button_text(),
                border: Border {
                    color: primary_action_background(),
                    width: 1.0,
                    radius: 2.0.into(),
                },
                text_color: None,
            },
            Status::Disabled { is_checked } => iced::widget::checkbox::Style {
                background: if is_checked {
                    Background::from(disabled_action_background())
                } else {
                    Background::from(match _theme {
                        Theme::Light => colors::CHECKBOX_UNCHECKED_BG_LIGHT,
                        _ => colors::CHECKBOX_UNCHECKED_BG_DARK,
                    })
                },
                icon_color: match _theme {
                    Theme::Light => colors::CHECKBOX_DISABLED_ICON_LIGHT,
                    _ => colors::CHECKBOX_DISABLED_ICON_DARK,
                },
                border: Border {
                    color: match _theme {
                        Theme::Light => colors::CHECKBOX_DISABLED_BORDER_LIGHT,
                        _ => colors::CHECKBOX_DISABLED_BORDER_DARK,
                    },
                    width: 1.0,
                    radius: 2.0.into(),
                },
                text_color: Some(match _theme {
                    Theme::Light => colors::CHECKBOX_DISABLED_TEXT_LIGHT,
                    _ => colors::CHECKBOX_DISABLED_TEXT_DARK,
                }),
            },
        }
    }
}

/// Create styled text input (blue border when focused)
///
/// This creates a text input with the same blue used for primary buttons,
/// providing visual consistency throughout the application.
pub fn primary_text_input_style()
-> fn(&Theme, iced::widget::text_input::Status) -> iced::widget::text_input::Style {
    |_theme, status| {
        use iced::widget::text_input::Status;

        match status {
            Status::Active => iced::widget::text_input::Style {
                background: Background::from(match _theme {
                    Theme::Light => colors::TEXT_INPUT_BG_LIGHT,
                    _ => colors::TEXT_INPUT_BG_DARK,
                }),
                border: Border {
                    color: match _theme {
                        Theme::Light => colors::TEXT_INPUT_BORDER_LIGHT,
                        _ => colors::TEXT_INPUT_BORDER_DARK,
                    },
                    width: 1.0,
                    radius: 2.0.into(),
                },
                icon: match _theme {
                    Theme::Light => colors::TEXT_INPUT_ICON_LIGHT,
                    _ => colors::TEXT_INPUT_ICON_DARK,
                },
                placeholder: match _theme {
                    Theme::Light => colors::TEXT_INPUT_PLACEHOLDER_LIGHT,
                    _ => colors::TEXT_INPUT_PLACEHOLDER_DARK,
                },
                value: match _theme {
                    Theme::Light => colors::TEXT_INPUT_VALUE_LIGHT,
                    _ => colors::TEXT_INPUT_VALUE_DARK,
                },
                selection: primary_action_background(),
            },
            Status::Focused => iced::widget::text_input::Style {
                background: Background::from(match _theme {
                    Theme::Light => colors::TEXT_INPUT_BG_LIGHT,
                    _ => colors::TEXT_INPUT_BG_DARK,
                }),
                border: Border {
                    color: primary_action_background(),
                    width: 2.0,
                    radius: 2.0.into(),
                },
                icon: match _theme {
                    Theme::Light => colors::TEXT_INPUT_ICON_LIGHT,
                    _ => colors::TEXT_INPUT_ICON_DARK,
                },
                placeholder: match _theme {
                    Theme::Light => colors::TEXT_INPUT_PLACEHOLDER_LIGHT,
                    _ => colors::TEXT_INPUT_PLACEHOLDER_DARK,
                },
                value: match _theme {
                    Theme::Light => colors::TEXT_INPUT_VALUE_LIGHT,
                    _ => colors::TEXT_INPUT_VALUE_DARK,
                },
                selection: primary_action_background(),
            },
            Status::Hovered => iced::widget::text_input::Style {
                background: Background::from(match _theme {
                    Theme::Light => colors::TEXT_INPUT_BG_LIGHT,
                    _ => colors::TEXT_INPUT_BG_DARK,
                }),
                border: Border {
                    color: primary_action_background(),
                    width: 1.0,
                    radius: 2.0.into(),
                },
                icon: match _theme {
                    Theme::Light => colors::TEXT_INPUT_ICON_LIGHT,
                    _ => colors::TEXT_INPUT_ICON_DARK,
                },
                placeholder: match _theme {
                    Theme::Light => colors::TEXT_INPUT_PLACEHOLDER_LIGHT,
                    _ => colors::TEXT_INPUT_PLACEHOLDER_DARK,
                },
                value: match _theme {
                    Theme::Light => colors::TEXT_INPUT_VALUE_LIGHT,
                    _ => colors::TEXT_INPUT_VALUE_DARK,
                },
                selection: primary_action_background(),
            },
            Status::Disabled => iced::widget::text_input::Style {
                background: Background::from(match _theme {
                    Theme::Light => colors::TEXT_INPUT_DISABLED_BG_LIGHT,
                    _ => colors::TEXT_INPUT_DISABLED_BG_DARK,
                }),
                border: Border {
                    color: match _theme {
                        Theme::Light => colors::TEXT_INPUT_DISABLED_BORDER_LIGHT,
                        _ => colors::TEXT_INPUT_DISABLED_BORDER_DARK,
                    },
                    width: 1.0,
                    radius: 2.0.into(),
                },
                icon: match _theme {
                    Theme::Light => colors::TEXT_INPUT_DISABLED_ICON_LIGHT,
                    _ => colors::TEXT_INPUT_DISABLED_ICON_DARK,
                },
                placeholder: match _theme {
                    Theme::Light => colors::TEXT_INPUT_DISABLED_PLACEHOLDER_LIGHT,
                    _ => colors::TEXT_INPUT_DISABLED_PLACEHOLDER_DARK,
                },
                value: match _theme {
                    Theme::Light => colors::TEXT_INPUT_DISABLED_VALUE_LIGHT,
                    _ => colors::TEXT_INPUT_DISABLED_VALUE_DARK,
                },
                selection: match _theme {
                    Theme::Light => colors::TEXT_INPUT_DISABLED_SELECTION_LIGHT,
                    _ => colors::TEXT_INPUT_DISABLED_SELECTION_DARK,
                },
            },
        }
    }
}

/// Create styled scrollbar (gray with theme-aware hover/active colors)
///
/// Uses consistent theme-aware colors for scrollbar states to match the application theme.
pub fn primary_scrollbar_style()
-> fn(&Theme, iced::widget::scrollable::Status) -> iced::widget::scrollable::Style {
    |theme, status| {
        use iced::widget::scrollable::Status;

        let scroller_color = match status {
            Status::Active => match theme {
                Theme::Light => colors::SIDEBAR_BORDER_LIGHT,
                _ => colors::SIDEBAR_BORDER_DARK,
            },
            Status::Hovered {
                is_horizontal_scrollbar_hovered,
                is_vertical_scrollbar_hovered,
            } => {
                if is_horizontal_scrollbar_hovered || is_vertical_scrollbar_hovered {
                    interactive_hover_color()
                } else {
                    match theme {
                        Theme::Light => colors::SIDEBAR_BORDER_LIGHT,
                        _ => colors::SIDEBAR_BORDER_DARK,
                    }
                }
            }
            Status::Dragged {
                is_horizontal_scrollbar_dragged,
                is_vertical_scrollbar_dragged,
            } => {
                if is_horizontal_scrollbar_dragged || is_vertical_scrollbar_dragged {
                    interactive_hover_color()
                } else {
                    match theme {
                        Theme::Light => colors::SIDEBAR_BORDER_LIGHT,
                        _ => colors::SIDEBAR_BORDER_DARK,
                    }
                }
            }
        };

        iced::widget::scrollable::Style {
            container: iced::widget::container::Style::default(),
            vertical_rail: iced::widget::scrollable::Rail {
                background: None,
                border: Border::default(),
                scroller: iced::widget::scrollable::Scroller {
                    color: scroller_color,
                    border: Border {
                        radius: 2.0.into(),
                        ..Default::default()
                    },
                },
            },
            horizontal_rail: iced::widget::scrollable::Rail {
                background: None,
                border: Border::default(),
                scroller: iced::widget::scrollable::Scroller {
                    color: scroller_color,
                    border: Border {
                        radius: 2.0.into(),
                        ..Default::default()
                    },
                },
            },
            gap: None,
        }
    }
}

/// Modal overlay container style (semi-transparent dark background)
pub fn modal_overlay_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(iced::Background::Color(iced::Color::from_rgba(
            0.0, 0.0, 0.0, 0.7,
        ))),
        ..Default::default()
    }
}
