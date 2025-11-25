//! Shared UI styling constants for consistent appearance across views
//!
//! Note: Some constants may have the same numeric values but represent
//! semantically different UI elements. This allows independent changes
//! to specific UI components without affecting others.

use iced::Color;

// ============================================================================
// Font Sizes
// ============================================================================

/// Title text size (e.g., form headers)
pub const TITLE_SIZE: u16 = 20;

/// Standard text and input field size
pub const TEXT_SIZE: u16 = 14;

/// Large vertical spacer (between title and form)
pub const SPACER_SIZE_LARGE: u16 = 15;

/// Medium vertical spacer (between sections)
pub const SPACER_SIZE_MEDIUM: u16 = 10;

/// Small vertical spacer (between related items)
pub const SPACER_SIZE_SMALL: u16 = 5;

/// Chat message text size (compact for readability)
pub const CHAT_MESSAGE_SIZE: u16 = 12;

/// Chat input field size (slightly larger than messages)
pub const CHAT_INPUT_SIZE: u16 = 13;

/// Toolbar title text size
pub const TOOLBAR_TITLE_SIZE: u16 = 16;

/// Toolbar button text size
pub const TOOLBAR_BUTTON_SIZE: u16 = 12;

/// Toolbar icon size (for collapse/expand icons)
pub const TOOLBAR_ICON_SIZE: u16 = 20;

/// Tooltip text size (smaller, less imposing)
pub const TOOLTIP_TEXT_SIZE: u16 = 11;

/// Tooltip gap (distance between element and tooltip)
pub const TOOLTIP_GAP: u16 = 5;

/// Tooltip padding (internal padding)
pub const TOOLTIP_PADDING: u16 = 4;

/// Tooltip background padding (inside the tooltip box)
pub const TOOLTIP_BACKGROUND_PADDING: u16 = 2;

/// Tooltip background color (semi-transparent black)
pub const TOOLTIP_BACKGROUND_COLOR: Color = Color::from_rgba(0.0, 0.0, 0.0, 0.8);

/// Empty view message text size
pub const EMPTY_VIEW_SIZE: u16 = 16;

/// Server list section title size
pub const SECTION_TITLE_SIZE: u16 = 14;

/// Server list server name text size
pub const SERVER_LIST_TEXT_SIZE: u16 = 13;

/// Server list button text size
pub const SERVER_LIST_BUTTON_SIZE: u16 = 12;

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

/// Toolbar button padding
pub const TOOLBAR_BUTTON_PADDING: u16 = 8;

/// Toolbar horizontal padding (matches FORM_PADDING for alignment)
pub const TOOLBAR_PADDING_HORIZONTAL: u16 = 20;

/// Toolbar vertical padding
pub const TOOLBAR_PADDING_VERTICAL: u16 = 8;

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
// Colors
// ============================================================================

/// Error message color (red) - used in forms
pub const ERROR_COLOR: Color = Color::from_rgb(1.0, 0.3, 0.3);

/// Error text color (bright red) - used in chat
pub const ERROR_TEXT_COLOR: Color = Color::from_rgb(1.0, 0.0, 0.0);

/// System message color (gray)
pub const SYSTEM_TEXT_COLOR: Color = Color::from_rgb(0.7, 0.7, 0.7);

/// Info message color (light blue)
pub const INFO_TEXT_COLOR: Color = Color::from_rgb(0.5, 0.8, 1.0);

/// Toolbar background color (dark gray)
pub const TOOLBAR_BACKGROUND_COLOR: Color = Color::from_rgb(0.15, 0.15, 0.15);

/// Empty view text color (light gray)
pub const EMPTY_VIEW_TEXT_COLOR: Color = Color::from_rgb(0.5, 0.5, 0.5);

/// Section title color (medium gray)
pub const SECTION_TITLE_COLOR: Color = Color::from_rgb(0.7, 0.7, 0.7);

/// Empty state text color (dark gray)
pub const EMPTY_STATE_COLOR: Color = Color::from_rgb(0.4, 0.4, 0.4);

/// Separator line color (dark gray)
pub const SEPARATOR_COLOR: Color = Color::from_rgb(0.3, 0.3, 0.3);

/// Server list panel background color (very dark gray)
pub const SERVER_LIST_BACKGROUND_COLOR: Color = Color::from_rgb(0.12, 0.12, 0.12);

/// Server list border color (dark gray)
pub const SERVER_LIST_BORDER_COLOR: Color = Color::from_rgb(0.2, 0.2, 0.2);

/// Bookmark row alternate background color (slightly lighter than sidebar)
pub const BOOKMARK_ROW_ALT_COLOR: Color = Color::from_rgb(0.15, 0.15, 0.15);

/// Bookmark button hover color (bright blue)
pub const BOOKMARK_BUTTON_HOVER_COLOR: Color = Color::from_rgb(0.3, 0.5, 0.7);

/// Toolbar icon default color (light gray)
pub const TOOLBAR_ICON_COLOR: Color = Color::from_rgb(0.7, 0.7, 0.7);

/// Toolbar icon hover color (bright blue)
pub const TOOLBAR_ICON_HOVER_COLOR: Color = Color::from_rgb(0.5, 0.7, 1.0);

/// Toolbar icon disabled color (dimmed gray)
pub const TOOLBAR_ICON_DISABLED_COLOR: Color = Color::from_rgb(0.3, 0.3, 0.3);

/// Disconnect icon default color (light gray)
pub const DISCONNECT_ICON_COLOR: Color = Color::from_rgb(0.6, 0.6, 0.6);

/// Disconnect icon hover color (red)
pub const DISCONNECT_ICON_HOVER_COLOR: Color = Color::from_rgb(1.0, 0.3, 0.3);

/// Edit/cog icon default color (light gray)
pub const EDIT_ICON_COLOR: Color = Color::from_rgb(0.6, 0.6, 0.6);

/// Edit/cog icon hover color (blue)
pub const EDIT_ICON_HOVER_COLOR: Color = Color::from_rgb(0.5, 0.7, 1.0);