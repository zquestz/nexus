//! UI colors derived from Iced's theme palette
//!
//! These functions pull colors from the theme's extended palette,
//! ensuring compatibility with all 22 built-in themes.
//!
//! For chat-specific colors, see the `chat` module.

use iced::{Color, Theme};

// ============================================================================
// Helper Functions
// ============================================================================

/// Adjust brightness of a color by a percentage (0.0 to 1.0)
/// When `lighten` is true, adds white. When false, adds black.
pub fn adjust_brightness(color: Color, amount: f32, lighten: bool) -> Color {
    if lighten {
        Color::from_rgb(
            color.r + (1.0 - color.r) * amount,
            color.g + (1.0 - color.g) * amount,
            color.b + (1.0 - color.b) * amount,
        )
    } else {
        Color::from_rgb(
            color.r * (1.0 - amount),
            color.g * (1.0 - amount),
            color.b * (1.0 - amount),
        )
    }
}

// ============================================================================
// Background Colors
// ============================================================================

/// Toolbar background
/// In dark mode: 5% lighter than base
/// In light mode: 5% darker than base
pub fn toolbar_background(theme: &Theme) -> Color {
    let bg = theme.palette().background;
    let is_dark = theme.extended_palette().is_dark;
    adjust_brightness(bg, 0.05, is_dark)
}

/// Sidebar panel background (darkest in dark mode, lightest in light mode)
/// Uses the base background color directly
pub fn sidebar_background(theme: &Theme) -> Color {
    theme.palette().background
}

/// Sidebar panel border color
pub fn sidebar_border(theme: &Theme) -> Color {
    let bg = theme.palette().background;
    let is_dark = theme.extended_palette().is_dark;
    // Subtle border: 10% lighter in dark mode, 10% darker in light mode
    adjust_brightness(bg, 0.10, is_dark)
}

/// Alternating row background color (for lists)
pub fn alt_row_color(theme: &Theme) -> Color {
    let bg = theme.palette().background;
    let is_dark = theme.extended_palette().is_dark;
    // Very subtle alternation: 3% difference
    adjust_brightness(bg, 0.03, is_dark)
}

// ============================================================================
// Text Colors
// ============================================================================

/// Primary text color (for regular content)
pub fn text_color(theme: &Theme) -> Color {
    theme.palette().text
}

/// Muted/secondary text color (empty states, placeholders, section titles)
pub fn muted_text_color(theme: &Theme) -> Color {
    let text = theme.palette().text;
    let is_dark = theme.extended_palette().is_dark;
    // Dim the text color: in dark mode make it darker, in light mode make it lighter
    adjust_brightness(text, 0.4, !is_dark)
}

// ============================================================================
// Icon Colors
// ============================================================================

/// Default icon color (visible against toolbar/sidebar backgrounds)
pub fn icon_color(theme: &Theme) -> Color {
    let text = theme.palette().text;
    let is_dark = theme.extended_palette().is_dark;
    // Slightly dimmed from full text color
    adjust_brightness(text, 0.2, !is_dark)
}

/// Disabled icon color (more dimmed)
pub fn icon_disabled_color(theme: &Theme) -> Color {
    let text = theme.palette().text;
    let is_dark = theme.extended_palette().is_dark;
    // More significantly dimmed
    adjust_brightness(text, 0.6, !is_dark)
}

// ============================================================================
// Danger Colors
// ============================================================================

/// Danger/error color (destructive actions, form validation errors)
pub fn danger_color(theme: &Theme) -> Color {
    theme.palette().danger
}
