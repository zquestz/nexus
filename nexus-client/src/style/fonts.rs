//! Font constants and sizes for consistent typography
//!
//! All font definitions and text sizes are defined here.

use iced::Font;
use iced::font::{Family, Weight};

// ============================================================================
// Font Definitions
// ============================================================================

/// Monospace font for text inputs
///
/// Uses embedded "SauceCodePro Nerd Font Mono" for consistent cross-platform rendering.
/// This avoids ligatures and ensures predictable character spacing.
pub const MONOSPACE_FONT: Font = Font {
    family: Family::Name("SauceCodePro Nerd Font Mono"),
    ..Font::MONOSPACE
};

/// Bold font for emphasis (unread indicators, etc.)
pub const BOLD_FONT: Font = Font {
    weight: Weight::Bold,
    ..Font::DEFAULT
};

// ============================================================================
// Font Sizes
// ============================================================================

/// Title text size (e.g., form headers)
pub const TITLE_SIZE: u16 = 20;

/// Standard text and input field size
pub const TEXT_SIZE: u16 = 14;

/// Chat message text size
pub const CHAT_MESSAGE_SIZE: u16 = 14;

/// Chat message line height (1.6x for better readability)
pub const CHAT_LINE_HEIGHT: f32 = 1.6;

/// Chat input field size (slightly larger than messages)
pub const CHAT_INPUT_SIZE: u16 = 16;

/// Toolbar title text size
pub const TOOLBAR_TITLE_SIZE: u16 = 16;

/// Tooltip text size (smaller, less imposing)
pub const TOOLTIP_TEXT_SIZE: u16 = 11;

/// Empty view message text size
pub const EMPTY_VIEW_SIZE: u16 = 16;

/// Server list section title size
pub const SECTION_TITLE_SIZE: u16 = 14;

/// Server list server name text size
pub const SERVER_LIST_TEXT_SIZE: u16 = 13;

/// Server list small text size (empty states, action buttons)
pub const SERVER_LIST_SMALL_TEXT_SIZE: u16 = 11;

/// User list title size
pub const USER_LIST_TITLE_SIZE: u16 = 16;

/// User list username text size
pub const USER_LIST_TEXT_SIZE: u16 = 12;

/// User list small text size (empty states)
pub const USER_LIST_SMALL_TEXT_SIZE: u16 = 11;

// ============================================================================
// Time Formats
// ============================================================================

/// Chat message timestamp format (hours:minutes:seconds)
pub const CHAT_TIME_FORMAT: &str = "%H:%M:%S";
