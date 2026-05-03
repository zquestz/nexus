//! Icon size constants for consistent icon rendering
//!
//! All icon size definitions are defined here.

// ============================================================================
// Toolbar Icons
// ============================================================================

/// Toolbar icon size (for collapse/expand icons)
pub const TOOLBAR_ICON_SIZE: f32 = 20.0;

// ============================================================================
// Server List Icons
// ============================================================================

/// Server list disconnect icon size (larger, more prominent)
pub const SERVER_LIST_DISCONNECT_ICON_SIZE: f32 = 18.0;

// ============================================================================
// Action Icons
// ============================================================================

/// Standard small-action icon size used by both toolbar-row buttons
/// (file browser toolbar, user-management tab toolbars) and panel-header
/// singleton action buttons (news, transfers, connection monitor, etc.).
/// The two roles differ only in their button padding — see
/// `TOOLBAR_BUTTON_PADDING` and `HEADING_BUTTON_PADDING` in `style::layout`.
pub const ICON_SIZE: f32 = 18.0;
