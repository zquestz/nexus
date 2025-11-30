//! Theme-aware color helper functions
//!
//! These functions return the appropriate color for the current theme.
//! Color names in the function bodies are self-documenting - they describe
//! the actual colors used for each theme variant.

use super::palette;
use iced::{Color, Theme};

// ============================================================================
// Helper
// ============================================================================

/// Select a color based on the current theme.
///
/// Returns `light` color for `Theme::Light`, `dark` color for all other themes.
/// This is an internal helper used by color functions and widget styles.
#[inline]
pub(crate) fn theme_color(theme: &Theme, light: Color, dark: Color) -> Color {
    match theme {
        Theme::Light => light,
        _ => dark,
    }
}

// ============================================================================
// Layout Colors (Toolbar, Sidebar, Content)
// ============================================================================

/// Toolbar background color
pub fn toolbar_background(theme: &Theme) -> Color {
    theme_color(theme, palette::PLATINUM, palette::CHARCOAL)
}

/// Sidebar panel background color
pub fn sidebar_background(theme: &Theme) -> Color {
    theme_color(theme, palette::SMOKE, palette::EBONY)
}

/// Content area background color (matches the default window background)
pub fn content_background(theme: &Theme) -> Color {
    theme.palette().background
}

/// Sidebar panel border color
pub fn sidebar_border(theme: &Theme) -> Color {
    theme_color(theme, palette::GAINSBORO, palette::JET)
}

/// Section title color (e.g., "Connected", "Bookmarks", "Users")
pub fn section_title_color(theme: &Theme) -> Color {
    theme_color(theme, palette::DIM_GRAY, palette::SILVER)
}

/// Sidebar empty state text color (e.g., "No connections", "No bookmarks")
pub fn sidebar_empty_color(theme: &Theme) -> Color {
    theme_color(theme, palette::LIGHT_SLATE, palette::GRANITE)
}

/// Separator line color
pub fn separator_color(theme: &Theme) -> Color {
    theme_color(theme, palette::SILVER, palette::DIM_GRAY)
}

/// Alternating row background color
pub fn alt_row_color(theme: &Theme) -> Color {
    theme_color(theme, palette::PEWTER, palette::CHARCOAL)
}

// ============================================================================
// Text Colors
// ============================================================================

/// Button text color on transparent buttons
pub fn button_text_color(theme: &Theme) -> Color {
    theme_color(theme, palette::BLACK, palette::WHITE)
}

/// Tooltip text color
pub fn tooltip_text_color(_theme: &Theme) -> Color {
    // Always white since tooltip background is dark in both themes
    palette::WHITE
}

/// Chat message text color (regular messages)
pub fn chat_text_color(theme: &Theme) -> Color {
    theme_color(theme, palette::BLACK, palette::WHITE)
}

/// System message text color (e.g., [SYS] user connected)
pub fn system_text_color(theme: &Theme) -> Color {
    theme_color(theme, palette::DARK_SLATE, palette::SILVER)
}

/// Info message text color (e.g., [INFO] notifications)
pub fn info_text_color(theme: &Theme) -> Color {
    theme_color(theme, palette::AZURE, palette::SKY_BLUE)
}

/// Chat timestamp color
pub fn chat_timestamp_color(_theme: &Theme) -> Color {
    // Same gray works well on both backgrounds
    palette::SLATE
}

/// Admin user text color (red to indicate admin status)
pub fn admin_user_text_color(theme: &Theme) -> Color {
    theme_color(theme, palette::CRIMSON, palette::CORAL)
}

/// Content area empty state text color (e.g., "Select a server to connect")
pub fn content_empty_color(theme: &Theme) -> Color {
    theme_color(theme, palette::LIGHT_SLATE, palette::GRANITE)
}

/// Broadcast message text color
pub fn broadcast_message_color(theme: &Theme) -> Color {
    theme_color(theme, palette::CRIMSON, palette::CORAL)
}

// ============================================================================
// Icon Colors
// ============================================================================

/// Toolbar icon color (enabled)
pub fn toolbar_icon_color(theme: &Theme) -> Color {
    theme_color(theme, palette::DIM_GRAY, palette::SILVER)
}

/// Toolbar icon color (disabled)
pub fn toolbar_icon_disabled_color(theme: &Theme) -> Color {
    theme_color(theme, palette::SILVER, palette::DIM_GRAY)
}

/// Disconnect icon default color
pub fn disconnect_icon_color(theme: &Theme) -> Color {
    theme_color(theme, palette::GRANITE, palette::LIGHT_SLATE)
}

/// Disconnect icon hover color (red for destructive action)
pub fn disconnect_icon_hover_color(theme: &Theme) -> Color {
    theme_color(theme, palette::CRIMSON, palette::CORAL)
}

/// Sidebar icon default color (bookmark cog, user list toolbar icons)
pub fn sidebar_icon_color(theme: &Theme) -> Color {
    theme_color(theme, palette::GRANITE, palette::LIGHT_SLATE)
}

/// Sidebar icon hover color
pub fn sidebar_icon_hover_color(theme: &Theme) -> Color {
    theme_color(theme, palette::COBALT, palette::CORNFLOWER)
}

/// Sidebar icon disabled color
pub fn sidebar_icon_disabled_color() -> Color {
    palette::DIM_GRAY
}

// ============================================================================
// Theme-Independent Colors
// ============================================================================

/// Interactive element hover color (our signature blue)
pub fn interactive_hover_color() -> Color {
    palette::STEEL_BLUE
}

/// Error text color (form validation, chat errors, bookmark errors)
pub fn error_color(theme: &Theme) -> Color {
    theme_color(theme, palette::CRIMSON, palette::CORAL)
}

/// Primary action button background color
pub fn primary_action_background() -> Color {
    palette::STEEL_BLUE
}

/// Primary action button background color (hovered)
pub fn primary_action_background_hovered() -> Color {
    palette::STEEL_BLUE_PALE
}

/// Primary action button background color (pressed)
pub fn primary_action_background_pressed() -> Color {
    palette::STEEL_BLUE_DARK
}

/// Disabled button background color
pub fn disabled_action_background() -> Color {
    palette::SLATE
}

/// Disabled button text color
pub fn disabled_action_text() -> Color {
    palette::GAINSBORO
}

/// Text color for buttons with colored backgrounds (always white)
pub fn action_button_text() -> Color {
    palette::WHITE
}
