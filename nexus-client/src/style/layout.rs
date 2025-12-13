//! Layout constants for consistent UI appearance
//!
//! All numeric constants for dimensions, sizes, spacing, and padding are defined here.
//! Window constants are in the `window` module. Font constants are in the `fonts` module.
//! Color functions are in the `ui` module. Widget styles are in the `widgets` module.

// ============================================================================
// Padding
// ============================================================================

/// Text input field padding
pub const INPUT_PADDING: f32 = 8.0;

/// Button padding
pub const BUTTON_PADDING: f32 = 10.0;

/// Form container padding
pub const FORM_PADDING: f32 = 20.0;

/// Toolbar horizontal padding (matches FORM_PADDING for alignment)
pub const TOOLBAR_PADDING_HORIZONTAL: f32 = 20.0;

/// Toolbar vertical padding
pub const TOOLBAR_PADDING_VERTICAL: f32 = 8.0;

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
    left: SMALL_PADDING,
};

/// Tab content padding (standard padding with extra right space for close button)
pub const TAB_CONTENT_PADDING: iced::Padding = iced::Padding {
    top: INPUT_PADDING,
    right: SMALL_PADDING,
    bottom: INPUT_PADDING,
    left: INPUT_PADDING,
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
pub const TOOLTIP_GAP: f32 = 5.0;

/// Tooltip padding (internal padding)
pub const TOOLTIP_PADDING: f32 = 4.0;

/// Tooltip background padding (inside the tooltip box)
pub const TOOLTIP_BACKGROUND_PADDING: f32 = 6.0;

/// Small padding (general use)
pub const SMALL_PADDING: f32 = 5.0;

// ============================================================================
// Spacing
// ============================================================================

/// General spacing between form elements
pub const ELEMENT_SPACING: f32 = 10.0;

/// Spacing between chat messages (compact)
pub const CHAT_SPACING: f32 = 3.0;

/// Small spacing (general use)
pub const SMALL_SPACING: f32 = 5.0;

/// Medium vertical spacer (between sections)
pub const SPACER_SIZE_MEDIUM: f32 = 10.0;

/// Small vertical spacer (between related items)
pub const SPACER_SIZE_SMALL: f32 = 5.0;

/// Toolbar spacing between sections
pub const TOOLBAR_SPACING: f32 = 20.0;

/// Toolbar icon spacing within icon groups
pub const TOOLBAR_ICON_SPACING: f32 = 0.0;

/// Panel spacing (no gap between panels)
pub const PANEL_SPACING: f32 = 0.0;

/// Server list section spacing
pub const SERVER_LIST_SECTION_SPACING: f32 = 5.0;

/// Server list item spacing
pub const SERVER_LIST_ITEM_SPACING: f32 = 3.0;

/// No spacing between button and icon (flush)
pub const NO_SPACING: f32 = 0.0;

/// User list item spacing
pub const USER_LIST_ITEM_SPACING: f32 = 3.0;

/// User list column spacing (matches server list section spacing)
pub const USER_LIST_SPACING: f32 = 5.0;

// ============================================================================
// Dimensions
// ============================================================================

/// Maximum width for form dialogs
pub const FORM_MAX_WIDTH: f32 = 400.0;

/// Maximum width for fingerprint dialog (wider to show fingerprints)
pub const FINGERPRINT_DIALOG_MAX_WIDTH: f32 = 600.0;

/// Server list panel width
pub const SERVER_LIST_PANEL_WIDTH: f32 = 220.0;

/// Server list button height
pub const SERVER_LIST_BUTTON_HEIGHT: f32 = 32.0;

/// Separator line height
pub const SEPARATOR_HEIGHT: f32 = 1.0;

/// Border width (standard)
pub const BORDER_WIDTH: f32 = 1.0;

/// User list panel width
pub const USER_LIST_PANEL_WIDTH: f32 = 180.0;

/// Avatar preview size in settings panel
pub const AVATAR_PREVIEW_SIZE: f32 = 48.0;

/// Avatar size in user list sidebar
pub const USER_LIST_AVATAR_SIZE: f32 = 28.0;

/// Spacing between avatar and username in user list
pub const USER_LIST_AVATAR_SPACING: f32 = 8.0;

/// Avatar size in user info panel
pub const USER_INFO_AVATAR_SIZE: f32 = 64.0;

/// Spacing between avatar and username in user info panel
pub const USER_INFO_AVATAR_SPACING: f32 = 12.0;

/// Maximum size to cache avatars at (matches largest display size)
///
/// Raster images are resized to this dimension before caching to save memory.
/// SVGs are not resized (vector graphics scale without quality loss).
pub const AVATAR_MAX_CACHE_SIZE: u32 = 64;

/// Server image preview size in edit mode (matches avatar preview size)
pub const SERVER_IMAGE_PREVIEW_SIZE: f32 = 48.0;

/// Maximum width to cache server images at (matches form width minus padding)
///
/// Server images are resized to this width before caching to save memory.
/// Height scales proportionally to preserve aspect ratio.
/// SVGs are not resized (vector graphics scale without quality loss).
pub const SERVER_IMAGE_MAX_CACHE_WIDTH: u32 = FORM_MAX_WIDTH as u32;

// ============================================================================
// Fingerprint Dialog Spacing
// ============================================================================

/// Space after fingerprint dialog title
pub const FINGERPRINT_SPACE_AFTER_TITLE: f32 = 10.0;

/// Space after server info line in fingerprint dialog
pub const FINGERPRINT_SPACE_AFTER_SERVER_INFO: f32 = 10.0;

/// Space after warning text in fingerprint dialog
pub const FINGERPRINT_SPACE_AFTER_WARNING: f32 = 10.0;

/// Space after fingerprint labels (tight coupling with value)
pub const FINGERPRINT_SPACE_AFTER_LABEL: f32 = 0.0;

/// Space between fingerprint sections (expected vs received)
pub const FINGERPRINT_SPACE_BETWEEN_SECTIONS: f32 = 8.0;

/// Space before button row in fingerprint dialog
pub const FINGERPRINT_SPACE_BEFORE_BUTTONS: f32 = 10.0;
