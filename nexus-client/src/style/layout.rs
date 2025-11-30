//! Layout constants for consistent UI appearance
//!
//! All numeric constants for dimensions, sizes, spacing, and padding are defined here.
//! Window constants are in the `window` module. Font constants are in the `fonts` module.
//! Color constants are in the `palette` module.

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

/// Tooltip gap (distance between element and tooltip)
pub const TOOLTIP_GAP: u16 = 5;

/// Tooltip padding (internal padding)
pub const TOOLTIP_PADDING: u16 = 4;

/// Tooltip background padding (inside the tooltip box)
pub const TOOLTIP_BACKGROUND_PADDING: u16 = 6;

/// Small padding (general use)
pub const SMALL_PADDING: u16 = 5;

// ============================================================================
// Spacing
// ============================================================================

/// General spacing between form elements
pub const ELEMENT_SPACING: u16 = 10;

/// Spacing between chat messages (compact)
pub const CHAT_SPACING: u16 = 3;

/// Small spacing (general use)
pub const SMALL_SPACING: u16 = 5;

/// Medium vertical spacer (between sections)
pub const SPACER_SIZE_MEDIUM: u16 = 10;

/// Small vertical spacer (between related items)
pub const SPACER_SIZE_SMALL: u16 = 5;

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

/// Border width (standard)
pub const BORDER_WIDTH: f32 = 1.0;

/// Border width when focused (thicker for visibility)
pub const BORDER_WIDTH_FOCUSED: f32 = 2.0;

/// Widget border radius (checkboxes, text inputs, scrollbars, tooltips)
pub const WIDGET_BORDER_RADIUS: f32 = 2.0;

/// User list panel width
pub const USER_LIST_PANEL_WIDTH: u16 = 180;
