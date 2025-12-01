//! Layout constants for consistent UI appearance
//!
//! All numeric constants for dimensions, sizes, spacing, and padding are defined here.
//! Window constants are in the `window` module. Font constants are in the `fonts` module.
//! Color functions are in the `ui` module. Widget styles are in the `widgets` module.

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
pub const ICON_BUTTON_PADDING_VERTICAL: f32 = 8.0;

/// Icon button padding (horizontal)
pub const ICON_BUTTON_PADDING_HORIZONTAL: f32 = 4.0;

/// Icon button padding (combined for symmetric buttons)
pub const ICON_BUTTON_PADDING: iced::Padding = iced::Padding {
    top: ICON_BUTTON_PADDING_VERTICAL,
    right: ICON_BUTTON_PADDING_HORIZONTAL,
    bottom: ICON_BUTTON_PADDING_VERTICAL,
    left: ICON_BUTTON_PADDING_HORIZONTAL,
};

/// Close button padding (left padding only, for close icon in tabs)
pub const CLOSE_BUTTON_PADDING: iced::Padding = iced::Padding {
    top: 0.0,
    right: 0.0,
    bottom: 0.0,
    left: SMALL_PADDING as f32,
};

/// Tab content padding (standard padding with extra right space for close button)
pub const TAB_CONTENT_PADDING: iced::Padding = iced::Padding {
    top: INPUT_PADDING as f32,
    right: SMALL_PADDING as f32,
    bottom: INPUT_PADDING as f32,
    left: INPUT_PADDING as f32,
};

/// Toolbar container padding (horizontal)
pub const TOOLBAR_CONTAINER_PADDING_HORIZONTAL: f32 = 4.0;

/// Toolbar container padding (horizontal only, for flush top/bottom)
pub const TOOLBAR_CONTAINER_PADDING: iced::Padding = iced::Padding {
    top: 0.0,
    right: TOOLBAR_CONTAINER_PADDING_HORIZONTAL,
    bottom: 0.0,
    left: TOOLBAR_CONTAINER_PADDING_HORIZONTAL,
};

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

/// Maximum width for fingerprint dialog (wider to show fingerprints)
pub const FINGERPRINT_DIALOG_MAX_WIDTH: f32 = 600.0;

/// Server list panel width
pub const SERVER_LIST_PANEL_WIDTH: u16 = 220;

/// Server list button height
pub const SERVER_LIST_BUTTON_HEIGHT: u16 = 32;

/// Separator line height
pub const SEPARATOR_HEIGHT: u16 = 1;

/// Border width (standard)
pub const BORDER_WIDTH: f32 = 1.0;

/// User list panel width
pub const USER_LIST_PANEL_WIDTH: u16 = 180;

// ============================================================================
// Fingerprint Dialog Spacing
// ============================================================================

/// Space after fingerprint dialog title
pub const FINGERPRINT_SPACE_AFTER_TITLE: u16 = 10;

/// Space after server info line in fingerprint dialog
pub const FINGERPRINT_SPACE_AFTER_SERVER_INFO: u16 = 10;

/// Space after warning text in fingerprint dialog
pub const FINGERPRINT_SPACE_AFTER_WARNING: u16 = 10;

/// Space after fingerprint labels (tight coupling with value)
pub const FINGERPRINT_SPACE_AFTER_LABEL: u16 = 0;

/// Space between fingerprint sections (expected vs received)
pub const FINGERPRINT_SPACE_BETWEEN_SECTIONS: u16 = 8;

/// Space before button row in fingerprint dialog
pub const FINGERPRINT_SPACE_BEFORE_BUTTONS: u16 = 10;
