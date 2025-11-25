//! Color constants for the Nexus BBS Client
//!
//! All color values are defined here as constants, organized by UI element.
//! Each color has both a dark theme and light theme variant, placed side-by-side
//! for easy comparison and maintenance.
//!
//! Helper functions in `style.rs` reference these constants to provide theme-aware colors.

use iced::Color;

// ============================================================================
// Toolbar Colors
// ============================================================================

/// Toolbar background - Dark theme
/// A dark gray that separates the toolbar from the main content area
pub const TOOLBAR_BACKGROUND_DARK: Color = Color::from_rgb(0.15, 0.15, 0.15);

/// Toolbar background - Light theme
/// A light gray that provides subtle contrast in light mode
pub const TOOLBAR_BACKGROUND_LIGHT: Color = Color::from_rgb(0.92, 0.92, 0.92);

/// Toolbar icon color (enabled) - Dark theme
/// Light gray for good contrast on dark toolbar
pub const TOOLBAR_ICON_DARK: Color = Color::from_rgb(0.7, 0.7, 0.7);

/// Toolbar icon color (enabled) - Light theme
/// Dark gray for good contrast on light toolbar
pub const TOOLBAR_ICON_LIGHT: Color = Color::from_rgb(0.3, 0.3, 0.3);

/// Toolbar icon color (disabled) - Dark theme
/// Dimmed dark gray to indicate unavailable actions
pub const TOOLBAR_ICON_DISABLED_DARK: Color = Color::from_rgb(0.3, 0.3, 0.3);

/// Toolbar icon color (disabled) - Light theme
/// Dimmed light gray to indicate unavailable actions
pub const TOOLBAR_ICON_DISABLED_LIGHT: Color = Color::from_rgb(0.7, 0.7, 0.7);

// ============================================================================
// Sidebar Colors (Server List & User List)
// ============================================================================

/// Sidebar panel background - Dark theme
/// Very dark gray, slightly darker than toolbar for visual hierarchy
pub const SIDEBAR_BACKGROUND_DARK: Color = Color::from_rgb(0.12, 0.12, 0.12);

/// Sidebar panel background - Light theme
/// Very light gray, slightly lighter than toolbar
pub const SIDEBAR_BACKGROUND_LIGHT: Color = Color::from_rgb(0.95, 0.95, 0.95);

/// Sidebar panel border - Dark theme
/// Subtle border to define panel edges
pub const SIDEBAR_BORDER_DARK: Color = Color::from_rgb(0.2, 0.2, 0.2);

/// Sidebar panel border - Light theme
/// Subtle border visible on light background
pub const SIDEBAR_BORDER_LIGHT: Color = Color::from_rgb(0.8, 0.8, 0.8);

/// Section title color (e.g., "Connected", "Bookmarks", "Users") - Dark theme
/// Light gray for good readability
pub const SECTION_TITLE_DARK: Color = Color::from_rgb(0.7, 0.7, 0.7);

/// Section title color (e.g., "Connected", "Bookmarks", "Users") - Light theme
/// Dark gray for strong contrast
pub const SECTION_TITLE_LIGHT: Color = Color::from_rgb(0.3, 0.3, 0.3);

/// Empty state text (e.g., "No connections", "No bookmarks") - Dark theme
/// Dimmed gray to indicate inactive/empty state
pub const EMPTY_STATE_DARK: Color = Color::from_rgb(0.4, 0.4, 0.4);

/// Empty state text (e.g., "No connections", "No bookmarks") - Light theme
/// Medium gray for subtle empty state indication
pub const EMPTY_STATE_LIGHT: Color = Color::from_rgb(0.6, 0.6, 0.6);

/// Alternating row background color - Dark theme
/// Slightly lighter than sidebar background for zebra striping
pub const ALT_ROW_BACKGROUND_DARK: Color = Color::from_rgb(0.15, 0.15, 0.15);

/// Alternating row background color - Light theme
/// Slightly darker than sidebar background for zebra striping
pub const ALT_ROW_BACKGROUND_LIGHT: Color = Color::from_rgb(0.90, 0.90, 0.90);

/// Button text color on transparent buttons - Dark theme
/// White text on dark backgrounds
pub const BUTTON_TEXT_DARK: Color = Color::WHITE;

/// Button text color on transparent buttons - Light theme
/// Black text on light backgrounds
pub const BUTTON_TEXT_LIGHT: Color = Color::BLACK;

/// Separator line color - Dark theme
/// Subtle line to divide sections
pub const SEPARATOR_DARK: Color = Color::from_rgb(0.3, 0.3, 0.3);

/// Separator line color - Light theme
/// Subtle line visible on light background
pub const SEPARATOR_LIGHT: Color = Color::from_rgb(0.7, 0.7, 0.7);

// ============================================================================
// Icon Colors (Disconnect, Edit, etc.)
// ============================================================================

/// Disconnect icon default color - Dark theme
/// Light gray for visibility on dark background
pub const DISCONNECT_ICON_DARK: Color = Color::from_rgb(0.6, 0.6, 0.6);

/// Disconnect icon default color - Light theme
/// Medium gray for visibility on light background
pub const DISCONNECT_ICON_LIGHT: Color = Color::from_rgb(0.4, 0.4, 0.4);

/// Disconnect icon hover color - Dark theme
/// Bright red to indicate destructive action
pub const DISCONNECT_ICON_HOVER_DARK: Color = Color::from_rgb(1.0, 0.3, 0.3);

/// Disconnect icon hover color - Light theme
/// Darker red for better contrast on light background
pub const DISCONNECT_ICON_HOVER_LIGHT: Color = Color::from_rgb(0.8, 0.2, 0.2);

/// Edit/cog icon default color - Dark theme
/// Light gray for visibility on dark background
pub const EDIT_ICON_DARK: Color = Color::from_rgb(0.6, 0.6, 0.6);

/// Edit/cog icon default color - Light theme
/// Medium gray for visibility on light background
pub const EDIT_ICON_LIGHT: Color = Color::from_rgb(0.4, 0.4, 0.4);

/// Edit/cog icon hover color - Dark theme
/// Light blue to indicate interactive element
pub const EDIT_ICON_HOVER_DARK: Color = Color::from_rgb(0.5, 0.7, 1.0);

/// Edit/cog icon hover color - Light theme
/// Darker blue for better contrast on light background
pub const EDIT_ICON_HOVER_LIGHT: Color = Color::from_rgb(0.2, 0.4, 0.8);

// ============================================================================
// Chat Message Colors
// ============================================================================

/// Regular chat message text - Dark theme
/// Pure white for maximum readability
pub const CHAT_TEXT_DARK: Color = Color::WHITE;

/// Regular chat message text - Light theme
/// Pure black for maximum readability
pub const CHAT_TEXT_LIGHT: Color = Color::BLACK;

/// System message text (e.g., [SYS] user connected) - Dark theme
/// Lighter gray, subdued compared to regular messages
pub const SYSTEM_TEXT_DARK: Color = Color::from_rgb(0.7, 0.7, 0.7);

/// System message text (e.g., [SYS] user connected) - Light theme
/// Darker gray, subdued and less prominent than regular messages
pub const SYSTEM_TEXT_LIGHT: Color = Color::from_rgb(0.35, 0.35, 0.35);

/// Info message text (e.g., [INFO] notifications) - Dark theme
/// Light blue to stand out as informational
pub const INFO_TEXT_DARK: Color = Color::from_rgb(0.5, 0.8, 1.0);

/// Info message text (e.g., [INFO] notifications) - Light theme
/// Dark blue for good contrast and readability
pub const INFO_TEXT_LIGHT: Color = Color::from_rgb(0.2, 0.5, 0.8);

/// Broadcast message text (e.g., [BROADCAST] announcements) - Dark theme
/// Bright red to stand out as important announcements
pub const BROADCAST_TEXT_DARK: Color = Color::from_rgb(1.0, 0.3, 0.3);

/// Broadcast message text (e.g., [BROADCAST] announcements) - Light theme
/// Dark red for visibility and importance
pub const BROADCAST_TEXT_LIGHT: Color = Color::from_rgb(0.8, 0.0, 0.0);

/// Admin user text in user list - Dark theme
/// Red to indicate admin status
pub const ADMIN_USER_TEXT_DARK: Color = Color::from_rgb(1.0, 0.3, 0.3);

/// Admin user text in user list - Light theme
/// Dark red to indicate admin status
pub const ADMIN_USER_TEXT_LIGHT: Color = Color::from_rgb(0.8, 0.0, 0.0);

// ============================================================================
// Empty View Colors
// ============================================================================

/// Empty view text (e.g., "Select a server to connect") - Dark theme
/// Medium gray for centered placeholder text
pub const EMPTY_VIEW_TEXT_DARK: Color = Color::from_rgb(0.5, 0.5, 0.5);

/// Empty view text (e.g., "Select a server to connect") - Light theme
/// Same medium gray works well on both backgrounds
pub const EMPTY_VIEW_TEXT_LIGHT: Color = Color::from_rgb(0.5, 0.5, 0.5);

// ============================================================================
// Theme-Independent Colors
// ============================================================================
// These colors are the same in both themes because they represent semantic
// meanings (error = red, primary action = blue) that should be consistent.

/// Error message text in chat - Theme-independent
/// Bright red for maximum visibility and urgency
pub const ERROR_MESSAGE: Color = Color::from_rgb(1.0, 0.0, 0.0);

/// Form validation error text - Theme-independent
/// Slightly softer red for form errors
pub const FORM_ERROR: Color = Color::from_rgb(1.0, 0.3, 0.3);

/// Primary action button background - Theme-independent
/// Our signature blue used throughout the app
pub const PRIMARY_ACTION_BG: Color = Color::from_rgb(0.3, 0.5, 0.7);

/// Primary action button hover state - Theme-independent
/// Lighter blue to indicate hover
pub const PRIMARY_ACTION_BG_HOVER: Color = Color::from_rgb(0.35, 0.55, 0.75);

/// Primary action button pressed state - Theme-independent
/// Darker blue to indicate pressed/active
pub const PRIMARY_ACTION_BG_PRESSED: Color = Color::from_rgb(0.25, 0.45, 0.65);

/// Disabled button background - Theme-independent
/// Gray to indicate disabled state
pub const DISABLED_ACTION_BG: Color = Color::from_rgb(0.5, 0.5, 0.5);

/// Disabled button text - Theme-independent
/// Light gray for low contrast on disabled buttons
pub const DISABLED_ACTION_TEXT: Color = Color::from_rgb(0.8, 0.8, 0.8);

/// Button text on colored backgrounds - Theme-independent
/// White text works on all our colored buttons (blue, gray)
pub const ACTION_BUTTON_TEXT: Color = Color::WHITE;

/// Interactive hover color (buttons, selections) - Theme-independent
/// Our signature blue used for hover states and active selections
pub const INTERACTIVE_HOVER: Color = Color::from_rgb(0.3, 0.5, 0.7);

/// Tooltip background - Theme-independent
/// Semi-transparent black works well on both light and dark backgrounds
pub const TOOLTIP_BACKGROUND: Color = Color::from_rgba(0.0, 0.0, 0.0, 0.8);

/// Tooltip text - Dark theme
/// White text for readability on dark tooltip background
pub const TOOLTIP_TEXT_DARK: Color = Color::WHITE;

/// Tooltip text - Light theme
/// White text also works on semi-transparent black background
pub const TOOLTIP_TEXT_LIGHT: Color = Color::WHITE;

// ============================================================================
// Checkbox Widget Colors
// ============================================================================

/// Checkbox unchecked background - Dark theme
/// Medium gray background for unchecked checkboxes in dark mode
pub const CHECKBOX_UNCHECKED_BG_DARK: Color = Color::from_rgb(0.3, 0.3, 0.3);

/// Checkbox unchecked background - Light theme
/// Light gray background for unchecked checkboxes in light mode
pub const CHECKBOX_UNCHECKED_BG_LIGHT: Color = Color::from_rgb(0.9, 0.9, 0.9);

/// Checkbox unchecked border - Dark theme
/// Lighter gray border for visibility in dark mode
pub const CHECKBOX_UNCHECKED_BORDER_DARK: Color = Color::from_rgb(0.5, 0.5, 0.5);

/// Checkbox unchecked border - Light theme
/// Medium gray border for unchecked checkboxes in light mode
pub const CHECKBOX_UNCHECKED_BORDER_LIGHT: Color = Color::from_rgb(0.6, 0.6, 0.6);

/// Checkbox unchecked background hover - Dark theme
/// Slightly lighter gray when hovering in dark mode
pub const CHECKBOX_UNCHECKED_BG_HOVER_DARK: Color = Color::from_rgb(0.35, 0.35, 0.35);

/// Checkbox unchecked background hover - Light theme
/// Slightly lighter gray when hovering in light mode
pub const CHECKBOX_UNCHECKED_BG_HOVER_LIGHT: Color = Color::from_rgb(0.95, 0.95, 0.95);

/// Checkbox disabled icon - Dark theme
/// Dimmed gray for disabled checkbox icon in dark mode
pub const CHECKBOX_DISABLED_ICON_DARK: Color = Color::from_rgb(0.4, 0.4, 0.4);

/// Checkbox disabled icon - Light theme
/// Light gray for disabled checkbox icon in light mode
pub const CHECKBOX_DISABLED_ICON_LIGHT: Color = Color::from_rgb(0.8, 0.8, 0.8);

/// Checkbox disabled border - Dark theme
/// Dimmed border for disabled checkboxes in dark mode
pub const CHECKBOX_DISABLED_BORDER_DARK: Color = Color::from_rgb(0.35, 0.35, 0.35);

/// Checkbox disabled border - Light theme
/// Gray border for disabled checkboxes in light mode
pub const CHECKBOX_DISABLED_BORDER_LIGHT: Color = Color::from_rgb(0.7, 0.7, 0.7);

/// Checkbox disabled text - Dark theme
/// Dimmed text for disabled checkbox labels in dark mode
pub const CHECKBOX_DISABLED_TEXT_DARK: Color = Color::from_rgb(0.4, 0.4, 0.4);

/// Checkbox disabled text - Light theme
/// Medium gray for disabled checkbox labels in light mode
pub const CHECKBOX_DISABLED_TEXT_LIGHT: Color = Color::from_rgb(0.6, 0.6, 0.6);

// ============================================================================
// Text Input Widget Colors
// ============================================================================

/// Text input background - Dark theme
/// Dark gray background for text inputs in dark mode
pub const TEXT_INPUT_BG_DARK: Color = Color::from_rgb(0.15, 0.15, 0.15);

/// Text input background - Light theme
/// White background for text inputs in light mode
pub const TEXT_INPUT_BG_LIGHT: Color = Color::WHITE;

/// Text input border (active) - Dark theme
/// Medium gray border for active text inputs
pub const TEXT_INPUT_BORDER_DARK: Color = Color::from_rgb(0.4, 0.4, 0.4);

/// Text input border (active) - Light theme
/// Medium gray border for active text inputs
pub const TEXT_INPUT_BORDER_LIGHT: Color = Color::from_rgb(0.6, 0.6, 0.6);

/// Text input icon - Dark theme
/// Medium gray for input field icons in dark mode
pub const TEXT_INPUT_ICON_DARK: Color = Color::from_rgb(0.5, 0.5, 0.5);

/// Text input icon - Light theme
/// Medium gray for input field icons in light mode
pub const TEXT_INPUT_ICON_LIGHT: Color = Color::from_rgb(0.5, 0.5, 0.5);

/// Text input placeholder - Dark theme
/// Dimmed gray for placeholder text in dark mode
pub const TEXT_INPUT_PLACEHOLDER_DARK: Color = Color::from_rgb(0.5, 0.5, 0.5);

/// Text input placeholder - Light theme
/// Medium gray for placeholder text in light mode
pub const TEXT_INPUT_PLACEHOLDER_LIGHT: Color = Color::from_rgb(0.7, 0.7, 0.7);

/// Text input value (text content) - Dark theme
/// White text for input content in dark mode
pub const TEXT_INPUT_VALUE_DARK: Color = Color::WHITE;

/// Text input value (text content) - Light theme
/// Black text for input content in light mode
pub const TEXT_INPUT_VALUE_LIGHT: Color = Color::BLACK;

/// Text input disabled background - Dark theme
/// Darker gray for disabled inputs in dark mode
pub const TEXT_INPUT_DISABLED_BG_DARK: Color = Color::from_rgb(0.25, 0.25, 0.25);

/// Text input disabled background - Light theme
/// Very light gray for disabled inputs in light mode
pub const TEXT_INPUT_DISABLED_BG_LIGHT: Color = Color::from_rgb(0.95, 0.95, 0.95);

/// Text input disabled border - Dark theme
/// Dimmed border for disabled inputs in dark mode
pub const TEXT_INPUT_DISABLED_BORDER_DARK: Color = Color::from_rgb(0.35, 0.35, 0.35);

/// Text input disabled border - Light theme
/// Light gray border for disabled inputs in light mode
pub const TEXT_INPUT_DISABLED_BORDER_LIGHT: Color = Color::from_rgb(0.8, 0.8, 0.8);

/// Text input disabled icon - Dark theme
/// Dimmed gray icon for disabled input fields in dark mode
pub const TEXT_INPUT_DISABLED_ICON_DARK: Color = Color::from_rgb(0.35, 0.35, 0.35);

/// Text input disabled icon - Light theme
/// Light gray icon for disabled input fields in light mode
pub const TEXT_INPUT_DISABLED_ICON_LIGHT: Color = Color::from_rgb(0.7, 0.7, 0.7);

/// Text input disabled placeholder - Dark theme
/// Dimmed placeholder text for disabled inputs in dark mode
pub const TEXT_INPUT_DISABLED_PLACEHOLDER_DARK: Color = Color::from_rgb(0.4, 0.4, 0.4);

/// Text input disabled placeholder - Light theme
/// Light gray placeholder for disabled inputs in light mode
pub const TEXT_INPUT_DISABLED_PLACEHOLDER_LIGHT: Color = Color::from_rgb(0.8, 0.8, 0.8);

/// Text input disabled value - Dark theme
/// Dimmed text for disabled input content in dark mode
pub const TEXT_INPUT_DISABLED_VALUE_DARK: Color = Color::from_rgb(0.5, 0.5, 0.5);

/// Text input disabled value - Light theme
/// Medium gray text for disabled input content in light mode
pub const TEXT_INPUT_DISABLED_VALUE_LIGHT: Color = Color::from_rgb(0.6, 0.6, 0.6);

/// Text input disabled selection - Dark theme
/// Dimmed selection highlight for disabled inputs in dark mode
pub const TEXT_INPUT_DISABLED_SELECTION_DARK: Color = Color::from_rgb(0.4, 0.4, 0.4);

/// Text input disabled selection - Light theme
/// Gray selection highlight for disabled inputs in light mode
pub const TEXT_INPUT_DISABLED_SELECTION_LIGHT: Color = Color::from_rgb(0.7, 0.7, 0.7);
