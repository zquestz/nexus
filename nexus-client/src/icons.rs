//! Icon constants for the Nexus client
//! 
//! This module provides Unicode symbols for icons throughout the UI.
//! These symbols are widely supported across platforms and fonts.

#![allow(dead_code)]

/// Connection status icons
pub const ICON_CONNECTED: &str = "●";
pub const ICON_DISCONNECTED: &str = "○";

/// Navigation arrows
pub const ICON_ARROW_LEFT: &str = "◄";
pub const ICON_ARROW_RIGHT: &str = "►";
pub const ICON_ARROW_UP: &str = "▲";
pub const ICON_ARROW_DOWN: &str = "▼";

/// Action icons
pub const ICON_ADD: &str = "＋";
pub const ICON_REMOVE: &str = "－";
pub const ICON_DELETE: &str = "✗";
pub const ICON_EDIT: &str = "✎";
pub const ICON_CHECK: &str = "✓";
pub const ICON_CROSS: &str = "×";

/// Status and notification icons
pub const ICON_WARNING: &str = "⚠";
pub const ICON_ERROR: &str = "✗";
pub const ICON_INFO: &str = "ℹ";
pub const ICON_SUCCESS: &str = "✓";

/// UI element icons
pub const ICON_SETTINGS: &str = "⚙";
pub const ICON_HOME: &str = "⌂";
pub const ICON_POWER: &str = "⚡";
pub const ICON_FLAG: &str = "⚐";

/// User and communication icons
pub const ICON_USER: &str = "⚉";
pub const ICON_USERS: &str = "❖";
pub const ICON_MESSAGE: &str = "✉";
pub const ICON_CHAT: &str = "💬";

/// Stars and ratings
pub const ICON_STAR_FILLED: &str = "★";
pub const ICON_STAR_EMPTY: &str = "☆";

/// Geometric shapes
pub const ICON_CIRCLE_FILLED: &str = "●";
pub const ICON_CIRCLE_EMPTY: &str = "○";
pub const ICON_CIRCLE_DOT: &str = "◉";
pub const ICON_SQUARE_FILLED: &str = "■";
pub const ICON_SQUARE_EMPTY: &str = "□";

/// Card suits (useful for decorative elements)
pub const ICON_HEART: &str = "♥";
pub const ICON_DIAMOND: &str = "♦";
pub const ICON_CLUB: &str = "♣";
pub const ICON_SPADE: &str = "♠";

/// Keyboard symbols
pub const ICON_ENTER: &str = "⏎";
pub const ICON_TAB: &str = "⇥";
pub const ICON_ESCAPE: &str = "⎋";
pub const ICON_BACKSPACE: &str = "⌫";

/// System messages prefix
pub const PREFIX_SYSTEM: &str = "***";

/// Helper function to combine icon with text
/// 
/// # Examples
/// ```
/// use nexus_client::icons::{with_icon, ICON_CONNECTED};
/// 
/// let label = with_icon(ICON_CONNECTED, "Server");
/// assert_eq!(label, "● Server");
/// ```
pub fn with_icon(icon: &str, text: &str) -> String {
    format!("{} {}", icon, text)
}

/// Helper function to combine icon with text, no space
pub fn with_icon_tight(icon: &str, text: &str) -> String {
    format!("{}{}", icon, text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_with_icon() {
        assert_eq!(with_icon(ICON_CONNECTED, "Server"), "● Server");
        assert_eq!(with_icon(ICON_USER, "Admin"), "⚉ Admin");
    }

    #[test]
    fn test_with_icon_tight() {
        assert_eq!(with_icon_tight(ICON_CONNECTED, "Online"), "●Online");
    }

    #[test]
    fn test_icons_not_empty() {
        assert!(!ICON_CONNECTED.is_empty());
        assert!(!ICON_WARNING.is_empty());
        assert!(!ICON_USER.is_empty());
    }
}