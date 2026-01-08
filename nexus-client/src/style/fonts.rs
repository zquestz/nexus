//! Font constants and sizes for consistent typography
//!
//! All font definitions and text sizes are defined here.

use iced::Font;
use iced::font::{Family, Style, Weight};

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

/// Monospace italic font for action messages (/me)
pub const MONOSPACE_ITALIC_FONT: Font = Font {
    family: Family::Name("SauceCodePro Nerd Font Mono"),
    style: Style::Italic,
    ..Font::MONOSPACE
};

// ============================================================================
// Font Sizes
// ============================================================================

/// Title text size (e.g., form headers)
pub const TITLE_SIZE: f32 = 20.0;

/// Subheading text size (e.g., section headers within panels)
pub const SUBHEADING_SIZE: f32 = 16.0;

/// Standard text and input field size
pub const TEXT_SIZE: f32 = 14.0;

/// Detail/secondary text size (smaller, for metadata and status lines)
pub const DETAIL_TEXT_SIZE: f32 = 12.0;

/// Chat message text size
pub const CHAT_MESSAGE_SIZE: f32 = 14.0;

/// Chat message line height (1.5x for better readability)
pub const CHAT_LINE_HEIGHT: f32 = 1.5;

/// News editor line height (1.5x for better readability)
pub const NEWS_EDITOR_LINE_HEIGHT: f32 = 1.5;

/// Toolbar title text size
pub const TOOLBAR_TITLE_SIZE: f32 = 16.0;

/// Tooltip text size (smaller, less imposing)
pub const TOOLTIP_TEXT_SIZE: f32 = 11.0;

/// Empty view message text size
pub const EMPTY_VIEW_SIZE: f32 = 16.0;

/// Server list section title size
pub const SECTION_TITLE_SIZE: f32 = 14.0;

/// Server list server name text size
pub const SERVER_LIST_TEXT_SIZE: f32 = 13.0;

/// Server list small text size (empty states, action buttons)
pub const SERVER_LIST_SMALL_TEXT_SIZE: f32 = 11.0;

/// User list title size (matches server list SECTION_TITLE_SIZE)
pub const USER_LIST_TITLE_SIZE: f32 = 14.0;

/// User list username text size (matches server list SERVER_LIST_TEXT_SIZE)
pub const USER_LIST_TEXT_SIZE: f32 = 13.0;

/// User list small text size (empty states)
pub const USER_LIST_SMALL_TEXT_SIZE: f32 = 11.0;
