//! Chat message colors
//!
//! Custom color palette for the chat message area. Uses `is_dark` from
//! the extended palette to select appropriate variants for any theme.
//!
//! These colors are intentionally kept separate from the UI palette to allow
//! fine-grained control over chat message appearance across all themes.

use iced::{Color, Theme};

// ============================================================================
// Color Constants
// ============================================================================

// Timestamps - subtle gray, de-emphasized
const TIMESTAMP_LIGHT: Color = Color::from_rgb(0.5, 0.5, 0.5);
const TIMESTAMP_DARK: Color = Color::from_rgb(0.6, 0.6, 0.6);

// Admin - distinctive red for admin users (NOT danger/destructive)
const ADMIN_LIGHT: Color = Color::from_rgb(0.8, 0.0, 0.0); // Crimson
const ADMIN_DARK: Color = Color::from_rgb(1.0, 0.3, 0.3); // Coral

// Broadcast - same as admin (broadcasts are admin-only action)
const BROADCAST_LIGHT: Color = Color::from_rgb(0.8, 0.0, 0.0); // Crimson
const BROADCAST_DARK: Color = Color::from_rgb(1.0, 0.3, 0.3); // Coral

// System - de-emphasized gray for system messages
const SYSTEM_LIGHT: Color = Color::from_rgb(0.35, 0.35, 0.35); // Dark slate
const SYSTEM_DARK: Color = Color::from_rgb(0.7, 0.7, 0.7); // Silver

// Shared account users - muted/weak color to distinguish from regular users
const SHARED_LIGHT: Color = Color::from_rgb(0.5, 0.5, 0.5); // Gray
const SHARED_DARK: Color = Color::from_rgb(0.55, 0.55, 0.55); // Dim gray

// ============================================================================
// Helper
// ============================================================================

/// Select color based on theme darkness (works with all 22 built-in themes)
#[inline]
fn for_theme(theme: &Theme, light: Color, dark: Color) -> Color {
    if theme.extended_palette().is_dark {
        dark
    } else {
        light
    }
}

// ============================================================================
// Chat Color Functions
// ============================================================================

/// Regular chat message text color
///
/// Uses the theme's text color for optimal readability on any background.
pub fn text(theme: &Theme) -> Color {
    theme.palette().text
}

/// Chat timestamp color
///
/// Subtle gray to de-emphasize timestamps relative to message content.
pub fn timestamp(theme: &Theme) -> Color {
    for_theme(theme, TIMESTAMP_LIGHT, TIMESTAMP_DARK)
}

/// Admin username color
///
/// Distinctive red to highlight admin users. This is separate from
/// `palette.danger` to distinguish admin indicators from destructive actions.
pub fn admin(theme: &Theme) -> Color {
    for_theme(theme, ADMIN_LIGHT, ADMIN_DARK)
}

/// Broadcast message color
///
/// Same red as admin since broadcasts are admin-only actions.
pub fn broadcast(theme: &Theme) -> Color {
    for_theme(theme, BROADCAST_LIGHT, BROADCAST_DARK)
}

/// System message color ([SYS])
///
/// De-emphasized gray for connection notifications, topic changes, etc.
pub fn system(theme: &Theme) -> Color {
    for_theme(theme, SYSTEM_LIGHT, SYSTEM_DARK)
}

/// Shared account user color
///
/// Muted gray to distinguish shared account users from regular users.
/// Less prominent than regular users but still readable.
pub fn shared(theme: &Theme) -> Color {
    for_theme(theme, SHARED_LIGHT, SHARED_DARK)
}

/// Info message color ([INFO])
///
/// Uses the theme's primary color for informational messages and command responses.
pub fn info(theme: &Theme) -> Color {
    theme.palette().primary
}

/// Error message color ([ERR])
///
/// Uses theme's danger color for consistency with UI error indicators.
pub fn error(theme: &Theme) -> Color {
    theme.palette().danger
}
